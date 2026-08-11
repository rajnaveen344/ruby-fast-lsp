//! Simple forward type tracker - replaces CFG with straightforward traversal.
//!
//! This module provides type tracking through Ruby methods using a simple forward
//! traversal of the AST. It handles:
//! - Local variable type tracking
//! - Control flow merging (if/case/while)
//! - Method return type inference
//!
//! Unlike the CFG-based approach, this is a single-pass traversal that creates
//! type snapshots at each statement, with explicit offset ranges showing where
//! each type is valid.

mod narrow;

use crate::control_flow;
use crate::core::method_return_equation::MethodReturnBase;
use crate::core::{
    FullyQualifiedName, MethodReturnEquation, NamespaceKind, RubyConstant, RubyMethod,
    TypeInferenceOutcome, UnknownReason,
};
use crate::engine::{AnalysisEngine, AnalysisQuery, AnalysisQueryCache};
use crate::inference::method::recursive::MAX_RECURSIVE_RETURN_ITERATIONS;
use crate::r#type::literal::LiteralAnalyzer;
use crate::r#type::ruby::RubyType;
use parking_lot::RwLock;
use ruby_prism::*;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

/// Private lattice value used while solving a recursive method return.
///
/// `Bottom` is the empty approximation for a recursive type variable. It is
/// intentionally not a `RubyType` variant, so it cannot escape into engine
/// facts, hover, inlay hints, or CLI output. `Unknown` is the opposite: a
/// required premise was not proven and therefore absorbs the equation.
#[derive(Debug, Clone, PartialEq, Eq)]
enum RecursiveReturnApproximation {
    Bottom,
    Proven(RubyType),
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Truthiness {
    AlwaysTruthy,
    AlwaysFalsy,
    Conditional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShortCircuitOperator {
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RightExecution {
    Always,
    Never,
    Conditional,
}

#[derive(Debug, Default)]
struct RescueEntryTypes {
    locals: HashMap<String, RubyType>,
}

impl RescueEntryTypes {
    fn observe(&mut self, name: &str, ruby_type: &RubyType) {
        self.locals
            .entry(name.to_string())
            .and_modify(|observed| {
                *observed = RubyType::union([observed.clone(), ruby_type.clone()]);
            })
            .or_insert_with(|| ruby_type.clone());
    }

    fn environment_from(
        &self,
        environment_before: &HashMap<String, RubyType>,
    ) -> HashMap<String, RubyType> {
        let mut environment = environment_before.clone();
        for (name, ruby_type) in &self.locals {
            environment.insert(name.clone(), ruby_type.clone());
        }
        environment
    }
}

/// One exact local-variable read solved by the forward flow tracker.
///
/// Offsets remain parser-native until FactCollector attaches the owning
/// `SourceFileId`. This keeps the reusable inference layer independent of LSP
/// positions and of workspace file registration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalReadType {
    pub start_offset: usize,
    pub end_offset: usize,
    pub name: String,
    pub ruby_type: RubyType,
}

impl RecursiveReturnApproximation {
    fn from_ruby_type(ruby_type: RubyType) -> Self {
        if ruby_type == RubyType::Unknown {
            Self::Unknown
        } else {
            Self::Proven(ruby_type)
        }
    }

    fn as_ruby_type(&self) -> RubyType {
        match self {
            Self::Proven(ruby_type) => ruby_type.clone(),
            Self::Bottom | Self::Unknown => RubyType::Unknown,
        }
    }

    fn into_outcome(self, unknown_reason: UnknownReason) -> TypeInferenceOutcome {
        match self {
            Self::Proven(ruby_type) => TypeInferenceOutcome::proven(ruby_type),
            Self::Bottom | Self::Unknown => TypeInferenceOutcome::unknown(unknown_reason),
        }
    }

    fn into_equation_base(self) -> MethodReturnBase {
        match self {
            Self::Bottom => MethodReturnBase::Bottom,
            Self::Proven(ruby_type) => MethodReturnBase::Proven(ruby_type),
            Self::Unknown => MethodReturnBase::Unknown(UnknownReason::UnresolvedMethodReturn),
        }
    }
}

/// Simple forward type tracker with control flow merging.
///
/// Performs a single forward pass through a method's AST, tracking variable
/// types and creating snapshots at each statement. Handles control flow by
/// cloning the environment for branches and merging at join points.
pub struct TypeTracker<'a> {
    /// Current type environment (variable name → type)
    vars: HashMap<String, RubyType>,

    /// Explicit parameter contracts for the method being tracked. They seed
    /// the flow environment before the body is visited; a later assignment can
    /// still replace or invalidate that proof normally.
    parameter_types: HashMap<String, RubyType>,

    /// Variable types at each offset (for queries)
    /// Key = offset where state was recorded, Value = all variables and their types
    var_types: BTreeMap<usize, HashMap<String, RubyType>>,

    /// Exact local-read results requested by FactCollector. Appending during
    /// traversal keeps the interactive path allocation-light; extraction
    /// sorts and collapses repeated bounded-loop visits to their final result.
    local_read_types: Vec<LocalReadType>,
    record_local_read_types: bool,
    /// Set on the first branch, rescue, or loop in the current method. Exact
    /// read evidence is only useful inside or after control flow; straight-line
    /// reads are already represented by the scope's assignment facts.
    has_seen_control_flow: bool,

    /// Source code (for offset calculations)
    #[allow(dead_code)]
    source: &'a [u8],

    /// Literal analyzer (for static type inference)
    literal_analyzer: LiteralAnalyzer,

    /// Engine for method return type lookups on analysis path
    analysis_engine: Option<Arc<RwLock<AnalysisEngine>>>,
    analysis_query_cache: Option<Arc<AnalysisQueryCache>>,

    /// Max loop iterations (to prevent infinite loops)
    max_loop_iterations: usize,

    /// Current lexical loop nesting depth. Only the outer loop performs
    /// stabilization iterations; nested loops receive one semantic pass.
    loop_depth: usize,

    #[cfg(test)]
    loop_body_passes: usize,

    /// Current class/module context for resolving implicit self
    current_class: Option<FullyQualifiedName>,

    /// Current method context for resolving `super`.
    current_method: Option<RubyMethod>,

    /// Same-file method return facts already collected before this method.
    local_method_returns: HashMap<FullyQualifiedName, RubyType>,

    /// Same-file methods whose complete current-pass declaration set proves
    /// public explicit-receiver access. This lets return inference use local
    /// method results without guessing through visibility before engine facts
    /// are installed.
    local_public_method_candidates: Arc<HashSet<FullyQualifiedName>>,

    /// Same-file superclass edges already collected before this method.
    local_superclasses: HashMap<FullyQualifiedName, FullyQualifiedName>,

    /// Same-file methods that contain `yield`, keyed by method FQN.
    yield_param_types_by_method: HashMap<FullyQualifiedName, Vec<RubyType>>,

    /// Local variables assigned lambda/proc literals, keyed by local name.
    proc_return_types_by_local: HashMap<String, RubyType>,

    /// Private same-file return dependencies carried through straight-line
    /// local aliases such as `value = helper; value`.
    ///
    /// Control-flow nodes clear this map conservatively until dependency terms
    /// participate in the full branch environment. It must never become a
    /// public Ruby type or survive a method pass.
    local_return_terms: HashMap<String, (FullyQualifiedName, RecursiveReturnApproximation)>,

    /// Dependency aliases are retained only through straight-line statements.
    /// Branch/loop environments do not yet join private dependency terms, so
    /// aliases created inside them are deliberately discarded.
    inside_control_flow: bool,

    /// Current private approximation for direct calls back to the method being
    /// solved. This value must never be projected as a concrete `RubyType`.
    recursive_return_approximation: Option<RecursiveReturnApproximation>,

    /// Same-file methods eligible to become return-equation dependencies.
    local_method_candidates: Arc<HashSet<FullyQualifiedName>>,

    /// Exact same-file calls observed as explicit or fallthrough return terms.
    observed_return_dependencies: BTreeSet<FullyQualifiedName>,

    /// Call locations whose result was proven directly from a modeled block or
    /// proc body. These are expression proofs, not dependencies on the
    /// ordinary return equation of the invoked method.
    direct_call_return_proofs: HashSet<usize>,

    /// Explicit `return` values found during the current method pass. Ruby
    /// methods return from these paths as well as from their fallthrough tail.
    explicit_return_types: Vec<RecursiveReturnApproximation>,

    /// Set while the ordinary method traversal observes a direct recursive
    /// call. This avoids a separate pre-scan of every method body.
    saw_direct_recursive_call: bool,

    /// Possible local values at every active protected body's rescue entry.
    ///
    /// Each ordinary local assignment records its value both before evaluating
    /// the RHS and after a successful write. An exception can therefore enter
    /// rescue on either side of that write. Nested protected bodies retain one
    /// accumulator each; a write is visible to every enclosing rescue frame
    /// because an inner exception may propagate outward.
    rescue_entry_types: Vec<RescueEntryTypes>,
}

impl<'a> TypeTracker<'a> {
    /// Create a new type tracker for the given source.
    pub fn new(source: &'a [u8]) -> Self {
        Self {
            vars: HashMap::new(),
            parameter_types: HashMap::new(),
            var_types: BTreeMap::new(),
            local_read_types: Vec::new(),
            record_local_read_types: false,
            has_seen_control_flow: false,
            source,
            literal_analyzer: LiteralAnalyzer::new(),
            analysis_engine: None,
            analysis_query_cache: None,
            max_loop_iterations: 10,
            loop_depth: 0,
            #[cfg(test)]
            loop_body_passes: 0,
            current_class: None,
            current_method: None,
            local_method_returns: HashMap::new(),
            local_public_method_candidates: Arc::new(HashSet::new()),
            local_superclasses: HashMap::new(),
            yield_param_types_by_method: HashMap::new(),
            proc_return_types_by_local: HashMap::new(),
            local_return_terms: HashMap::new(),
            inside_control_flow: false,
            recursive_return_approximation: None,
            local_method_candidates: Arc::new(HashSet::new()),
            observed_return_dependencies: BTreeSet::new(),
            direct_call_return_proofs: HashSet::new(),
            explicit_return_types: Vec::new(),
            saw_direct_recursive_call: false,
            rescue_entry_types: Vec::new(),
        }
    }

    pub fn with_analysis_engine(mut self, analysis_engine: Arc<RwLock<AnalysisEngine>>) -> Self {
        self.analysis_engine = Some(analysis_engine);
        self
    }

    pub fn with_analysis_query_cache(mut self, cache: Arc<AnalysisQueryCache>) -> Self {
        self.analysis_query_cache = Some(cache);
        self
    }

    pub fn with_local_method_returns(
        mut self,
        local_method_returns: HashMap<FullyQualifiedName, RubyType>,
    ) -> Self {
        self.local_method_returns = local_method_returns;
        self
    }

    pub(crate) fn with_local_public_method_candidates(
        mut self,
        local_public_method_candidates: Arc<HashSet<FullyQualifiedName>>,
    ) -> Self {
        self.local_public_method_candidates = local_public_method_candidates;
        self
    }

    pub fn with_local_superclasses(
        mut self,
        local_superclasses: HashMap<FullyQualifiedName, FullyQualifiedName>,
    ) -> Self {
        self.local_superclasses = local_superclasses;
        self
    }

    pub(crate) fn with_parameter_types(
        mut self,
        parameter_types: HashMap<String, RubyType>,
    ) -> Self {
        self.parameter_types = parameter_types;
        self
    }

    pub fn with_yield_param_types(
        mut self,
        yield_param_types_by_method: HashMap<FullyQualifiedName, Vec<RubyType>>,
    ) -> Self {
        self.yield_param_types_by_method = yield_param_types_by_method;
        self
    }

    /// Set the current class/module context for resolving implicit self
    pub fn set_current_class(&mut self, fqn: Option<FullyQualifiedName>) {
        self.current_class = fqn;
    }

    /// Get variable types map (for storing in RubyDocument)
    pub fn into_var_types(self) -> BTreeMap<usize, HashMap<String, RubyType>> {
        self.var_types
    }

    pub(crate) fn with_local_read_types(mut self) -> Self {
        self.record_local_read_types = true;
        self
    }

    pub(crate) fn take_local_read_types(&mut self) -> Vec<LocalReadType> {
        let mut reads = std::mem::take(&mut self.local_read_types);
        reads.sort_by_key(|read| (read.start_offset, read.end_offset));

        let mut deduplicated: Vec<LocalReadType> = Vec::with_capacity(reads.len());
        for read in reads {
            if deduplicated.last().is_some_and(|previous| {
                previous.start_offset == read.start_offset && previous.end_offset == read.end_offset
            }) {
                *deduplicated.last_mut().expect(
                    "INVARIANT VIOLATED: the final local-read entry disappeared after it was checked. This is a bug because no mutation occurs between the check and replacement. Fix: keep repeated-read collapse atomic.",
                ) = read;
            } else {
                deduplicated.push(read);
            }
        }
        deduplicated
    }

    /// Record current variable state at an offset
    fn record_state(&mut self, offset: usize) {
        // Only record if there are variables to track
        if !self.vars.is_empty() {
            self.var_types.insert(offset, self.vars.clone());
        }
    }

    /// Track a method definition and return its inferred return type
    ///
    /// This is the main entry point for type tracking. It:
    /// Track a program's top-level statements (outside of methods)
    ///
    /// This tracks variable assignments and control flow at the top level.
    pub fn track_program(&mut self, program: &ProgramNode) -> RubyType {
        let stmts = program.statements();
        self.track_statements(&stmts)
    }

    /// 1. Adds method parameters to the type environment
    /// 2. Tracks the method body, creating snapshots along the way
    /// 3. Returns the inferred return type (type of last expression)
    pub fn track_method(&mut self, method: &DefNode) -> RubyType {
        self.track_method_outcome(method).into_ruby_type()
    }

    /// Infer a method return while retaining why proof was withheld.
    ///
    /// Direct recursion is solved as a bounded least fixed point. The private
    /// bottom value starts with no possible return and is ignored by unions;
    /// public `Unknown` remains absorbing. A cycle with no proven base, an
    /// incomplete premise, or a non-converging equation therefore stays
    /// explainable Unknown instead of becoming a guessed concrete type.
    pub fn track_method_outcome(&mut self, method: &DefNode) -> TypeInferenceOutcome {
        let mut approximation = RecursiveReturnApproximation::Bottom;
        for _iteration in 0..MAX_RECURSIVE_RETURN_ITERATIONS {
            let next = self.track_method_once(method, Some(approximation.clone()));
            if !self.saw_direct_recursive_call {
                return next.into_outcome(UnknownReason::UnresolvedMethodReturn);
            }
            if next == approximation {
                return next.into_outcome(UnknownReason::UnprovenRecursiveCycle);
            }
            if next == RecursiveReturnApproximation::Unknown {
                return TypeInferenceOutcome::unknown(UnknownReason::UnprovenRecursiveCycle);
            }
            approximation = next;
        }

        TypeInferenceOutcome::unknown(UnknownReason::UnprovenRecursiveCycle)
    }

    /// Collect a compact same-file return equation during the existing AST
    /// traversal. Exact returned calls become dependencies; unsupported uses
    /// remain absorbing Unknown rather than being mistaken for recursion.
    pub(crate) fn track_method_equation(
        &mut self,
        method: &DefNode,
        method_fqn: FullyQualifiedName,
        local_method_candidates: Arc<HashSet<FullyQualifiedName>>,
    ) -> MethodReturnEquation {
        self.local_method_candidates = local_method_candidates;

        let base = self.track_method_once(method, None).into_equation_base();
        let dependencies = std::mem::take(&mut self.observed_return_dependencies);
        self.local_method_candidates = Arc::new(HashSet::new());

        MethodReturnEquation::new(method_fqn, base, dependencies)
    }

    fn track_method_once(
        &mut self,
        method: &DefNode,
        recursive_return_approximation: Option<RecursiveReturnApproximation>,
    ) -> RecursiveReturnApproximation {
        assert!(
            self.rescue_entry_types.is_empty(),
            "INVARIANT VIOLATED: a rescue-entry accumulator escaped a previous method traversal. This is a bug because protected-body state is lexical and cannot cross method boundaries. Fix: pop every accumulator immediately after tracking its protected expression."
        );
        self.vars.clear();
        self.var_types.clear();
        self.local_read_types.clear();
        self.has_seen_control_flow = false;
        self.proc_return_types_by_local.clear();
        self.local_return_terms.clear();
        self.explicit_return_types.clear();
        self.observed_return_dependencies.clear();
        self.direct_call_return_proofs.clear();
        self.saw_direct_recursive_call = false;
        self.recursive_return_approximation = recursive_return_approximation;

        let previous_method = self.current_method.clone();
        self.current_method = Some(normalized_method_name(method));

        // Add parameters to environment
        if let Some(params) = method.parameters() {
            self.add_parameters(&params);
        }

        // Track method body
        let fallthrough_type = if let Some(body) = method.body() {
            self.track_node(&body)
        } else {
            RubyType::nil_class()
        };

        // Record final state at method end
        if let Some(body) = method.body() {
            let end_offset = body.location().end_offset();
            self.record_state(end_offset);
        }

        let fallthrough_term = method
            .body()
            .and_then(|body| self.return_term_dependency_for_node(&body));
        let fallthrough = match method.body() {
            Some(body) if control_flow::diverges(&body) => RecursiveReturnApproximation::Bottom,
            Some(_)
                if fallthrough_term.as_ref().is_some_and(|(dependency, _)| {
                    self.should_track_return_dependency(
                        dependency,
                        &fallthrough_type,
                        self.saw_direct_recursive_call,
                    )
                }) =>
            {
                let (dependency, approximation) = fallthrough_term.expect(
                    "INVARIANT VIOLATED: checked return term disappeared before use. This is a bug because the local result is immutable. Fix: destructure the option once instead of mutating dependency state between checks.",
                );
                self.observed_return_dependencies.insert(dependency);
                approximation
            }
            Some(_) if fallthrough_type == RubyType::Unknown => {
                RecursiveReturnApproximation::Unknown
            }
            Some(_) | None => RecursiveReturnApproximation::from_ruby_type(fallthrough_type),
        };

        let mut alternatives = std::mem::take(&mut self.explicit_return_types);
        alternatives.push(fallthrough);
        let return_type = join_recursive_return_approximations(alternatives);

        assert!(
            self.rescue_entry_types.is_empty(),
            "INVARIANT VIOLATED: method traversal finished with an active rescue-entry accumulator. This is a bug because every protected body must restore the accumulator stack before publishing inferred types. Fix: balance the push/pop in begin and rescue-modifier tracking."
        );

        self.current_method = previous_method;
        self.recursive_return_approximation = None;
        return_type
    }

    /// Add method parameters to the type environment
    fn add_parameters(&mut self, _params: &ParametersNode) {
        for (name, ruby_type) in &self.parameter_types {
            self.vars.insert(name.clone(), ruby_type.clone());
        }
    }

    /// Track a node and return its type
    ///
    /// This is the main dispatcher that routes to specific tracking methods
    /// based on the node type.
    fn track_node(&mut self, node: &Node) -> RubyType {
        match node {
            // Statements node - track sequence of statements
            _ if node.as_statements_node().is_some() => {
                let stmts = node.as_statements_node().unwrap();
                self.track_statements(&stmts)
            }

            // Local variable assignment
            _ if node.as_local_variable_write_node().is_some() => {
                let write = node.as_local_variable_write_node().unwrap();
                self.track_assignment(&write)
            }

            // If/unless conditionals
            _ if node.as_if_node().is_some() => {
                let if_node = node.as_if_node().unwrap();
                self.track_control_flow(|tracker| tracker.track_if(&if_node))
            }

            _ if node.as_unless_node().is_some() => {
                let unless_node = node.as_unless_node().unwrap();
                self.track_control_flow(|tracker| tracker.track_unless(&unless_node))
            }

            // Case statement
            _ if node.as_case_node().is_some() => {
                let case_node = node.as_case_node().unwrap();
                self.track_control_flow(|tracker| tracker.track_case(&case_node))
            }

            _ if node.as_case_match_node().is_some() => {
                let case_match_node = node.as_case_match_node().unwrap();
                self.track_control_flow(|tracker| tracker.track_case_match(&case_match_node))
            }

            // Short-circuit boolean expressions are control flow: the right
            // operand may mutate locals, but only one of the skipped/executed
            // environments reaches the following expression.
            _ if node.as_and_node().is_some() => {
                let and_node = node.as_and_node().unwrap();
                self.track_control_flow(|tracker| {
                    tracker.track_short_circuit(
                        &and_node.left(),
                        &and_node.right(),
                        ShortCircuitOperator::And,
                    )
                })
            }

            _ if node.as_or_node().is_some() => {
                let or_node = node.as_or_node().unwrap();
                self.track_control_flow(|tracker| {
                    tracker.track_short_circuit(
                        &or_node.left(),
                        &or_node.right(),
                        ShortCircuitOperator::Or,
                    )
                })
            }

            _ if node.as_super_node().is_some() || node.as_forwarding_super_node().is_some() => {
                self.infer_super_return_type()
            }

            // Begin/rescue/ensure expressions
            _ if node.as_begin_node().is_some() => {
                let begin_node = node.as_begin_node().unwrap();
                self.track_control_flow(|tracker| tracker.track_begin(&begin_node))
            }

            _ if node.as_rescue_modifier_node().is_some() => {
                let rescue_modifier = node.as_rescue_modifier_node().unwrap();
                self.track_control_flow(|tracker| tracker.track_rescue_modifier(&rescue_modifier))
            }

            // Loops
            _ if node.as_while_node().is_some() => {
                let while_node = node.as_while_node().unwrap();
                self.track_control_flow(|tracker| tracker.track_while(&while_node))
            }

            _ if node.as_until_node().is_some() => {
                let until_node = node.as_until_node().unwrap();
                self.track_control_flow(|tracker| tracker.track_until(&until_node))
            }

            // Default: try to infer expression type
            _ => self.infer_expression(node),
        }
    }

    fn track_control_flow(&mut self, track: impl FnOnce(&mut Self) -> RubyType) -> RubyType {
        self.local_return_terms.clear();
        let was_inside_control_flow = self.inside_control_flow;
        self.inside_control_flow = true;
        self.has_seen_control_flow = true;
        let ruby_type = track(self);
        self.inside_control_flow = was_inside_control_flow;
        self.local_return_terms.clear();
        ruby_type
    }

    /// Track a sequence of statements and return last expression type
    fn track_statements(&mut self, stmts: &StatementsNode) -> RubyType {
        let mut last_type = RubyType::nil_class();

        for stmt in stmts.body().iter() {
            // Process the statement (this updates self.vars)
            last_type = self.track_node(&stmt);

            // Record state after each statement
            let stmt_end = stmt.location().end_offset();
            self.record_state(stmt_end);
        }

        last_type
    }

    /// Track a local variable assignment
    ///
    /// Infers the type from the value expression and updates the type environment.
    fn track_assignment(&mut self, write: &LocalVariableWriteNode) -> RubyType {
        // Get variable name
        let var_name = String::from_utf8_lossy(write.name().as_slice()).to_string();

        // The RHS may raise before the local write commits. Every enclosing
        // rescue can therefore observe the prior value (or Ruby's implicit nil
        // for a syntactically declared local that has not yet been assigned).
        if !self.rescue_entry_types.is_empty() {
            let prior_type = self
                .vars
                .get(&var_name)
                .cloned()
                .unwrap_or_else(RubyType::nil_class);
            self.observe_rescue_entry_type(&var_name, &prior_type);
        }

        // Infer type from value
        let value = write.value();
        let is_direct_recursive_value = value
            .as_call_node()
            .is_some_and(|call| call_is_direct_recursive(&call, self.current_method.as_ref()));
        let var_type = self.track_node(&value);
        let dependency = value
            .as_call_node()
            .and_then(|call| self.return_term_dependency_for_call(&call));
        if let Some(return_type) = self.infer_proc_literal_return_type(&value) {
            self.proc_return_types_by_local
                .insert(var_name.clone(), return_type);
        } else {
            self.proc_return_types_by_local.remove(&var_name);
        }

        if let Some((dependency, approximation)) = dependency.filter(|(dependency, _)| {
            !self.inside_control_flow
                && self.should_track_return_dependency(
                    dependency,
                    &var_type,
                    is_direct_recursive_value,
                )
        }) {
            self.local_return_terms
                .insert(var_name.clone(), (dependency, approximation));
        } else {
            self.local_return_terms.remove(&var_name);
        }

        // Update environment
        self.vars.insert(var_name.clone(), var_type.clone());
        if !self.rescue_entry_types.is_empty() {
            self.observe_rescue_entry_type(&var_name, &var_type);
        }

        // Return the assigned type (assignments return their value in Ruby)
        var_type
    }

    fn observe_rescue_entry_type(&mut self, name: &str, ruby_type: &RubyType) {
        for entry_types in &mut self.rescue_entry_types {
            entry_types.observe(name, ruby_type);
        }
    }

    /// Track an if statement with branch merging
    ///
    /// Clones the environment for each branch, tracks them separately,
    /// then merges the results at the join point.
    fn track_if(&mut self, if_node: &IfNode) -> RubyType {
        // Track the predicate (for potential side effects)
        let predicate = if_node.predicate();
        self.track_node(&predicate);

        let env_before = self.vars.clone();

        // Then branch
        let then_diverges = if_node
            .statements()
            .map(|s| control_flow::diverges(&s.as_node()))
            .unwrap_or(false);
        let then_type = if let Some(statements) = if_node.statements() {
            self.track_node(&statements.as_node())
        } else {
            RubyType::nil_class()
        };
        let then_env = self.vars.clone();

        self.vars = env_before.clone();

        // Else branch
        let else_diverges = if_node
            .subsequent()
            .map(|n| control_flow::diverges(&n))
            .unwrap_or(false);
        let else_type = if let Some(subsequent) = if_node.subsequent() {
            match &subsequent {
                _ if subsequent.as_else_node().is_some() => {
                    let else_node = subsequent.as_else_node().unwrap();
                    if let Some(statements) = else_node.statements() {
                        self.track_node(&statements.as_node())
                    } else {
                        RubyType::nil_class()
                    }
                }
                _ if subsequent.as_if_node().is_some() => {
                    let elsif_node = subsequent.as_if_node().unwrap();
                    self.track_if(&elsif_node)
                }
                _ => RubyType::nil_class(),
            }
        } else {
            RubyType::nil_class()
        };
        let else_env = self.vars.clone();

        // Merge envs — diverging branches never reach the join point.
        match (then_diverges, else_diverges) {
            (true, true) => self.vars = env_before,
            (true, false) => {
                // then exited → predicate was false at the join.
                self.vars = else_env;
                narrow::narrow(&mut self.vars, &predicate, false);
            }
            (false, true) => {
                // else exited → predicate was true at the join.
                self.vars = then_env;
                narrow::narrow(&mut self.vars, &predicate, true);
            }
            (false, false) => {
                self.vars = then_env;
                self.merge_env(&else_env, if_node.subsequent().is_none());
            }
        }

        // Type union — exclude diverging branches.
        join_branch_types(&[(then_type, then_diverges), (else_type, else_diverges)])
    }

    /// Track a case statement with branch merging
    ///
    /// Each when clause is tracked separately, then all branches
    /// (including else) are merged at the join point.
    fn track_case(&mut self, case_node: &CaseNode) -> RubyType {
        // Track the predicate (the value being matched)
        if let Some(predicate) = case_node.predicate() {
            self.track_node(&predicate);
        }

        let env_before = self.vars.clone();

        // (env, type, diverges) per branch.
        let mut branches: Vec<(HashMap<String, RubyType>, RubyType, bool)> = Vec::new();

        for condition in case_node.conditions().iter() {
            if let Some(when_node) = condition.as_when_node() {
                self.vars = env_before.clone();
                let diverges = when_node
                    .statements()
                    .map(|s| control_flow::diverges(&s.as_node()))
                    .unwrap_or(false);
                let branch_type = if let Some(statements) = when_node.statements() {
                    self.track_node(&statements.as_node())
                } else {
                    RubyType::nil_class()
                };
                branches.push((self.vars.clone(), branch_type, diverges));
            }
        }

        if let Some(else_clause) = case_node.else_clause() {
            self.vars = env_before.clone();
            let diverges = else_clause
                .statements()
                .map(|s| control_flow::diverges(&s.as_node()))
                .unwrap_or(false);
            let else_type = if let Some(statements) = else_clause.statements() {
                self.track_node(&statements.as_node())
            } else {
                RubyType::nil_class()
            };
            branches.push((self.vars.clone(), else_type, diverges));
        } else {
            push_unmatched_ordinary_case_path(&mut branches, &env_before);
        }

        if branches.is_empty() {
            return RubyType::nil_class();
        }

        // Pick post-state from non-diverging branches only.
        let surviving_envs: Vec<&HashMap<String, RubyType>> = branches
            .iter()
            .filter(|(_, _, d)| !*d)
            .map(|(env, _, _)| env)
            .collect();

        if surviving_envs.is_empty() {
            // All branches diverge — code after is unreachable. Keep pre-state.
            self.vars = env_before;
        } else {
            self.vars = surviving_envs[0].clone();
            for env in &surviving_envs[1..] {
                self.merge_env(env, false);
            }
        }

        // Type union — exclude diverging branches.
        let typed_branches: Vec<(RubyType, bool)> =
            branches.into_iter().map(|(_, ty, d)| (ty, d)).collect();
        join_branch_types(&typed_branches)
    }

    fn track_case_match(&mut self, case_node: &CaseMatchNode) -> RubyType {
        let predicate = case_node.predicate();
        if let Some(predicate) = &predicate {
            self.track_node(predicate);
        }

        let env_before = self.vars.clone();
        let mut branches: Vec<(HashMap<String, RubyType>, RubyType, bool)> = Vec::new();

        for condition in case_node.conditions().iter() {
            let Some(in_node) = condition.as_in_node() else {
                continue;
            };

            self.vars = env_before.clone();
            if let Some(predicate) = &predicate {
                let captures = self.pattern_capture_types_for_value(&in_node.pattern(), predicate);
                for (name, ty) in captures {
                    if ty != RubyType::Unknown {
                        self.vars.insert(name, ty);
                    }
                }
            }

            let diverges = in_node
                .statements()
                .map(|s| control_flow::diverges(&s.as_node()))
                .unwrap_or(false);
            let branch_type = if let Some(statements) = in_node.statements() {
                self.track_node(&statements.as_node())
            } else {
                RubyType::nil_class()
            };
            branches.push((self.vars.clone(), branch_type, diverges));
        }

        let has_else = case_node.else_clause().is_some();
        if has_else {
            self.vars = env_before.clone();
            let else_clause = case_node.else_clause().unwrap();
            let diverges = else_clause
                .statements()
                .map(|s| control_flow::diverges(&s.as_node()))
                .unwrap_or(false);
            let else_type = if let Some(statements) = else_clause.statements() {
                self.track_node(&statements.as_node())
            } else {
                RubyType::nil_class()
            };
            branches.push((self.vars.clone(), else_type, diverges));
        }

        if branches.is_empty() {
            return RubyType::nil_class();
        }

        let surviving_envs: Vec<&HashMap<String, RubyType>> = branches
            .iter()
            .filter(|(_, _, d)| !*d)
            .map(|(env, _, _)| env)
            .collect();

        if surviving_envs.is_empty() {
            self.vars = env_before;
        } else {
            self.vars = surviving_envs[0].clone();
            for env in &surviving_envs[1..] {
                self.merge_env(env, false);
            }
        }

        // Unlike an ordinary `case ... when`, a `case ... in` expression with
        // no matching pattern and no `else` raises NoMatchingPatternError.
        // That path cannot reach the environment after the case and therefore
        // must not add NilClass to bindings from the surviving `in` branches.
        // `merge_env` above still adds NilClass when a binding is absent from a
        // different reachable branch or from an explicit `else`.

        let typed_branches: Vec<(RubyType, bool)> =
            branches.into_iter().map(|(_, ty, d)| (ty, d)).collect();
        join_branch_types(&typed_branches)
    }

    fn pattern_capture_types_for_value(
        &mut self,
        pattern: &Node<'_>,
        value: &Node<'_>,
    ) -> HashMap<String, RubyType> {
        let mut captures = HashMap::new();
        self.collect_pattern_capture_types(pattern, value, &mut captures);
        captures
    }

    fn collect_pattern_capture_types(
        &mut self,
        pattern: &Node<'_>,
        value: &Node<'_>,
        captures: &mut HashMap<String, RubyType>,
    ) {
        if let Some(target) = pattern.as_local_variable_target_node() {
            let name = String::from_utf8_lossy(target.name().as_slice()).to_string();
            captures.insert(name, self.infer_expression(value));
            return;
        }

        if let Some(pattern_hash) = pattern.as_hash_pattern_node() {
            let Some(value_hash) = value.as_hash_node() else {
                return;
            };
            let value_elements = value_hash
                .elements()
                .iter()
                .filter_map(|element| {
                    let assoc = element.as_assoc_node()?;
                    Some((pattern_symbol_key(&assoc.key())?, assoc.value()))
                })
                .collect::<Vec<_>>();

            for element in pattern_hash.elements().iter() {
                let Some(assoc) = element.as_assoc_node() else {
                    continue;
                };
                let Some(key) = pattern_symbol_key(&assoc.key()) else {
                    continue;
                };
                let Some((_, value_node)) = value_elements
                    .iter()
                    .find(|(value_key, _)| value_key == &key)
                else {
                    continue;
                };
                self.collect_pattern_capture_types(&assoc.value(), value_node, captures);
            }
            return;
        }

        if let Some(pattern_array) = pattern.as_array_pattern_node() {
            let Some(value_array) = value.as_array_node() else {
                return;
            };
            let value_elements = value_array.elements().iter().collect::<Vec<_>>();
            for (index, required) in pattern_array.requireds().iter().enumerate() {
                let Some(value_node) = value_elements.get(index) else {
                    continue;
                };
                self.collect_pattern_capture_types(&required, value_node, captures);
            }
        }
    }

    fn track_begin(&mut self, begin_node: &BeginNode) -> RubyType {
        let env_before = self.vars.clone();

        let has_rescue = begin_node.rescue_clause().is_some();
        if has_rescue {
            self.rescue_entry_types.push(RescueEntryTypes::default());
        }

        self.vars = env_before.clone();
        let body_diverges = begin_node
            .statements()
            .map(|statements| control_flow::diverges(&statements.as_node()))
            .unwrap_or(false);
        let body_type = begin_node
            .statements()
            .map(|statements| self.track_node(&statements.as_node()))
            .unwrap_or_else(RubyType::nil_class);
        let body_env = self.vars.clone();
        let rescue_entry_types = has_rescue.then(|| {
            self.rescue_entry_types.pop().expect(
                "INVARIANT VIOLATED: a begin/rescue protected-body accumulator disappeared before rescue analysis. This is a bug because only the matching begin frame may pop it. Fix: keep rescue accumulator ownership stack-disciplined.",
            )
        });

        self.vars = body_env;
        let else_diverges = begin_node
            .else_clause()
            .and_then(|else_node| else_node.statements())
            .map(|statements| control_flow::diverges(&statements.as_node()))
            .unwrap_or(false);
        let normal_type = begin_node
            .else_clause()
            .and_then(|else_node| else_node.statements())
            .map(|statements| self.track_node(&statements.as_node()))
            .unwrap_or(body_type);
        let normal_env = self.vars.clone();
        let normal_diverges = body_diverges || else_diverges;

        let mut branches = vec![(normal_env, normal_type, normal_diverges)];
        let mut rescue_clause = begin_node.rescue_clause();
        while let Some(rescue_node) = rescue_clause {
            self.vars = rescue_entry_types
                .as_ref()
                .expect(
                    "INVARIANT VIOLATED: a rescue clause has no protected-body entry evidence. This is a bug because has_rescue and the immutable rescue chain came from the same Prism begin node. Fix: create one accumulator whenever a rescue clause exists.",
                )
                .environment_from(&env_before);
            let diverges = rescue_node
                .statements()
                .map(|statements| control_flow::diverges(&statements.as_node()))
                .unwrap_or(false);
            let branch_type = rescue_node
                .statements()
                .map(|statements| self.track_node(&statements.as_node()))
                .unwrap_or_else(RubyType::nil_class);
            branches.push((self.vars.clone(), branch_type, diverges));
            rescue_clause = rescue_node.subsequent();
        }

        let surviving_envs = branches
            .iter()
            .filter(|(_, _, diverges)| !*diverges)
            .map(|(env, _, _)| env)
            .collect::<Vec<_>>();
        if surviving_envs.is_empty() {
            self.vars = env_before;
        } else {
            self.vars = surviving_envs[0].clone();
            for env in &surviving_envs[1..] {
                self.merge_env(env, false);
            }
        }

        if let Some(ensure_clause) = begin_node.ensure_clause() {
            if let Some(statements) = ensure_clause.statements() {
                self.track_node(&statements.as_node());
            }
        }

        let typed_branches = branches
            .into_iter()
            .map(|(_, ty, diverges)| (ty, diverges))
            .collect::<Vec<_>>();
        join_branch_types(&typed_branches)
    }

    fn track_rescue_modifier(&mut self, rescue_modifier: &RescueModifierNode) -> RubyType {
        let env_before = self.vars.clone();

        let expression = rescue_modifier.expression();
        self.vars = env_before.clone();
        self.rescue_entry_types.push(RescueEntryTypes::default());
        let expression_type = self.track_node(&expression);
        let expression_env = self.vars.clone();
        let rescue_entry_types = self.rescue_entry_types.pop().expect(
            "INVARIANT VIOLATED: a rescue-modifier protected-expression accumulator disappeared before rescue analysis. This is a bug because only the matching rescue modifier may pop it. Fix: keep rescue accumulator ownership stack-disciplined.",
        );

        let rescue_expression = rescue_modifier.rescue_expression();
        self.vars = rescue_entry_types.environment_from(&env_before);
        let rescue_type = self.track_node(&rescue_expression);
        let rescue_env = self.vars.clone();

        self.vars = expression_env;
        self.merge_env(&rescue_env, false);

        join_branch_types(&[
            (expression_type, control_flow::diverges(&expression)),
            (rescue_type, control_flow::diverges(&rescue_expression)),
        ])
    }

    /// Track a while loop with limited iterations
    ///
    /// Iterates the loop body a few times to allow types to stabilize,
    /// then merges with the pre-loop state (since loop might not execute).
    fn track_while(&mut self, while_node: &WhileNode) -> RubyType {
        // Track the predicate
        let predicate = while_node.predicate();
        self.track_node(&predicate);

        // Save pre-loop state
        let env_before = self.vars.clone();

        // Iterate only the outer loop to avoid exponential work for generated
        // code containing deeply nested loops.
        let iterations = if self.loop_depth == 0 {
            self.max_loop_iterations
        } else {
            1
        };
        self.loop_depth += 1;
        let mut last_type = RubyType::nil_class();
        for _iteration in 0..iterations {
            #[cfg(test)]
            {
                self.loop_body_passes += 1;
            }
            if let Some(statements) = while_node.statements() {
                last_type = self.track_node(&statements.as_node());
            }
        }
        self.loop_depth -= 1;

        // Save post-loop state
        let loop_env = self.vars.clone();

        // Merge with pre-loop state (loop might not execute at all)
        self.vars = env_before.clone();
        self.merge_env(&loop_env, true); // true = loop might not run

        last_type
    }

    /// Track an until loop (inverse of while)
    fn track_until(&mut self, until_node: &UntilNode) -> RubyType {
        // Track the predicate
        let predicate = until_node.predicate();
        self.track_node(&predicate);

        // Save pre-loop state
        let env_before = self.vars.clone();

        let iterations = if self.loop_depth == 0 {
            self.max_loop_iterations
        } else {
            1
        };
        self.loop_depth += 1;
        let mut last_type = RubyType::nil_class();
        for _iteration in 0..iterations {
            #[cfg(test)]
            {
                self.loop_body_passes += 1;
            }
            if let Some(statements) = until_node.statements() {
                last_type = self.track_node(&statements.as_node());
            }
        }
        self.loop_depth -= 1;

        // Save post-loop state
        let loop_env = self.vars.clone();

        // Merge with pre-loop state (loop might not execute at all)
        self.vars = env_before.clone();
        self.merge_env(&loop_env, true); // true = loop might not run

        last_type
    }

    /// Track an unless statement (inverse of if)
    fn track_unless(&mut self, unless_node: &UnlessNode) -> RubyType {
        // Track the predicate (for potential side effects)
        let predicate = unless_node.predicate();
        self.track_node(&predicate);

        let env_before = self.vars.clone();

        // Then branch (executes when predicate is false)
        let then_diverges = unless_node
            .statements()
            .map(|s| control_flow::diverges(&s.as_node()))
            .unwrap_or(false);
        let then_type = if let Some(statements) = unless_node.statements() {
            self.track_node(&statements.as_node())
        } else {
            RubyType::nil_class()
        };
        let then_env = self.vars.clone();

        self.vars = env_before.clone();

        // Else branch
        let else_diverges = unless_node
            .else_clause()
            .and_then(|e| e.statements())
            .map(|s| control_flow::diverges(&s.as_node()))
            .unwrap_or(false);
        let else_type = if let Some(else_clause) = unless_node.else_clause() {
            if let Some(statements) = else_clause.statements() {
                self.track_node(&statements.as_node())
            } else {
                RubyType::nil_class()
            }
        } else {
            RubyType::nil_class()
        };
        let else_env = self.vars.clone();

        match (then_diverges, else_diverges) {
            (true, true) => self.vars = env_before,
            (true, false) => {
                // unless body (executes when predicate FALSE) exited → at join, predicate was true.
                self.vars = else_env;
                narrow::narrow(&mut self.vars, &predicate, true);
            }
            (false, true) => {
                // else (executes when predicate TRUE) exited → at join, predicate was false.
                self.vars = then_env;
                narrow::narrow(&mut self.vars, &predicate, false);
            }
            (false, false) => {
                self.vars = then_env;
                self.merge_env(&else_env, unless_node.else_clause().is_none());
            }
        }

        join_branch_types(&[(then_type, then_diverges), (else_type, else_diverges)])
    }

    /// Track Ruby's value-returning short-circuit operators.
    ///
    /// The left operand always executes. Depending on its proven truthiness,
    /// the right operand executes always, never, or along one reachable path.
    /// Conditional execution joins the environment immediately after the left
    /// operand with the environment after the right operand. This prevents a
    /// syntactically later assignment in the right operand from being treated
    /// as unconditional by downstream receiver queries.
    fn track_short_circuit(
        &mut self,
        left: &Node<'_>,
        right: &Node<'_>,
        operator: ShortCircuitOperator,
    ) -> RubyType {
        let left_type = self.track_node(left);
        let left_env = self.vars.clone();
        let truthiness = ruby_truthiness(&left_type);
        let right_execution = match (operator, truthiness) {
            (ShortCircuitOperator::And, Truthiness::AlwaysTruthy)
            | (ShortCircuitOperator::Or, Truthiness::AlwaysFalsy) => RightExecution::Always,
            (ShortCircuitOperator::And, Truthiness::AlwaysFalsy)
            | (ShortCircuitOperator::Or, Truthiness::AlwaysTruthy) => RightExecution::Never,
            (ShortCircuitOperator::And | ShortCircuitOperator::Or, Truthiness::Conditional) => {
                RightExecution::Conditional
            }
        };

        match right_execution {
            RightExecution::Never => left_type,
            RightExecution::Always => self.track_node(right),
            RightExecution::Conditional => {
                self.vars = left_env.clone();
                let right_type = self.track_node(right);
                let right_env = self.vars.clone();

                self.vars = left_env;
                self.merge_env(&right_env, false);

                short_circuit_result_type(left_type, right_type, operator)
            }
        }
    }

    /// Infer the type of an expression
    ///
    /// Uses literal analyzer for static types, and handles variable reads
    /// by looking up their type from the current environment.
    fn infer_expression(&mut self, node: &Node) -> RubyType {
        // Try literal analysis first
        if let Some(ty) = self.literal_analyzer.analyze_literal(node) {
            return ty;
        }

        // Handle local variable reads
        if let Some(read) = node.as_local_variable_read_node() {
            let var_name = String::from_utf8_lossy(read.name().as_slice()).to_string();
            let ruby_type = self
                .vars
                .get(&var_name)
                .cloned()
                .unwrap_or(RubyType::Unknown);
            if self.record_local_read_types && self.has_seen_control_flow {
                let location = read.location();
                self.local_read_types.push(LocalReadType {
                    start_offset: location.start_offset(),
                    end_offset: location.end_offset(),
                    name: var_name,
                    ruby_type: ruby_type.clone(),
                });
            }
            return ruby_type;
        }

        // Handle method calls
        if let Some(call) = node.as_call_node() {
            return self.infer_call(&call);
        }

        // Handle return statements
        if let Some(ret) = node.as_return_node() {
            return self.infer_return(&ret);
        }

        // Handle constant reads (class references)
        if let Some(const_read) = node.as_constant_read_node() {
            let const_name = String::from_utf8_lossy(const_read.name().as_slice()).to_string();
            if let Ok(constant) = RubyConstant::new(&const_name) {
                return RubyType::ClassReference(FullyQualifiedName::constant(vec![constant]));
            }
        }

        // Handle constant path (namespaced constants like Foo::Bar)
        if let Some(const_path) = node.as_constant_path_node() {
            if let Some(fqn) = Self::resolve_constant_path(&const_path) {
                return RubyType::ClassReference(fqn);
            }
        }

        // Handle parenthesized expressions
        if let Some(parens) = node.as_parentheses_node() {
            if let Some(body) = parens.body() {
                return self.track_node(&body);
            }
            return RubyType::nil_class();
        }

        // Handle interpolated strings
        if node.as_interpolated_string_node().is_some() {
            return RubyType::string();
        }

        RubyType::Unknown
    }

    /// Infer the return type of a method call
    fn infer_call(&mut self, call: &CallNode) -> RubyType {
        let method_name = String::from_utf8_lossy(call.name().as_slice()).to_string();

        // A statically modeled yielding/proc call proves the block result
        // directly. Do not replace that proof with the callee's ordinary
        // method-return equation: Ruby yielding APIs intentionally return the
        // block value even when their own body contains an otherwise unknown
        // `yield` expression.
        if let Some(block_return_type) = self.infer_yielding_block_return_type(call, &method_name) {
            self.direct_call_return_proofs
                .insert(call.location().start_offset());
            return block_return_type;
        }
        if let Some(proc_return_type) = self.infer_proc_call_return_type(call, &method_name) {
            self.direct_call_return_proofs
                .insert(call.location().start_offset());
            return proc_return_type;
        }

        if call_is_direct_recursive(call, self.current_method.as_ref()) {
            self.saw_direct_recursive_call = true;
            if let Some(approximation) = self.recursive_return_approximation.as_ref() {
                return approximation.as_ruby_type();
            }
        }

        // Handle .new specially - it returns an instance of the class
        if method_name == "new" {
            if let Some(receiver) = call.receiver() {
                if let Some(const_read) = receiver.as_constant_read_node() {
                    let class_name =
                        String::from_utf8_lossy(const_read.name().as_slice()).to_string();
                    if let Ok(constant) = RubyConstant::new(&class_name) {
                        let fqn = FullyQualifiedName::constant(vec![constant]);
                        return RubyType::Class(fqn);
                    }
                }
                // Handle namespaced constant like Foo::Bar.new
                if let Some(const_path) = receiver.as_constant_path_node() {
                    if let Some(fqn) = Self::resolve_constant_path(&const_path) {
                        return RubyType::Class(fqn);
                    }
                }
            }
        }

        // Get receiver type
        let receiver_type = if let Some(receiver) = call.receiver() {
            self.infer_expression(&receiver)
        } else {
            // Implicit self - use current class context
            if let Some(ref fqn) = self.current_class {
                RubyType::Class(fqn.clone())
            } else {
                RubyType::Unknown
            }
        };

        // If receiver is Unknown, propagate Unknown (no global lookup)
        if receiver_type == RubyType::Unknown {
            return RubyType::Unknown;
        }

        let allow_private = call.receiver().is_none();
        if let RubyType::Union(members) = &receiver_type {
            return crate::inference::method::return_type::resolve_proven_union(
                members,
                |member| {
                    self.resolve_method_return_type_from_analysis(
                        member,
                        &method_name,
                        allow_private,
                    )
                    .or_else(|| self.resolve_rbs_method_return_type(member, &method_name))
                },
            )
            .unwrap_or(RubyType::Unknown);
        }
        if let Some(return_type) = self.resolve_method_return_type_from_analysis(
            &receiver_type,
            &method_name,
            allow_private,
        ) {
            return return_type;
        }

        self.resolve_rbs_method_return_type(&receiver_type, &method_name)
            .unwrap_or(RubyType::Unknown)
    }

    fn infer_proc_call_return_type(&self, call: &CallNode, method_name: &str) -> Option<RubyType> {
        if method_name != "call" {
            return None;
        }
        let receiver = call.receiver()?;
        let local = receiver.as_local_variable_read_node()?;
        let name = String::from_utf8_lossy(local.name().as_slice()).to_string();
        self.proc_return_types_by_local
            .get(&name)
            .filter(|ty| **ty != RubyType::Unknown)
            .cloned()
    }

    fn infer_proc_literal_return_type(&mut self, value: &Node) -> Option<RubyType> {
        if let Some(lambda) = value.as_lambda_node() {
            return self.infer_isolated_proc_body(lambda.body());
        }

        let call = value.as_call_node()?;
        if call.name().as_slice() != b"new" {
            return None;
        }
        let receiver = call.receiver()?;
        let constant = receiver.as_constant_read_node()?;
        if constant.name().as_slice() != b"Proc" {
            return None;
        }
        let block = call.block()?.as_block_node()?;
        self.infer_isolated_proc_body(block.body())
    }

    fn infer_isolated_proc_body(&mut self, body: Option<Node<'_>>) -> Option<RubyType> {
        let previous_vars = self.vars.clone();
        let explicit_return_count = self.explicit_return_types.len();
        let return_type = body
            .map(|body| self.track_node(&body))
            .unwrap_or_else(RubyType::nil_class);
        self.vars = previous_vars;
        self.explicit_return_types.truncate(explicit_return_count);
        (return_type != RubyType::Unknown).then_some(return_type)
    }

    fn resolve_rbs_method_return_type(
        &self,
        receiver_type: &RubyType,
        method_name: &str,
    ) -> Option<RubyType> {
        let is_singleton = matches!(
            receiver_type,
            RubyType::ClassReference(_) | RubyType::ModuleReference(_)
        );
        let class_name = match receiver_type {
            RubyType::Class(fqn)
            | RubyType::ClassReference(fqn)
            | RubyType::Module(fqn)
            | RubyType::ModuleReference(fqn) => fqn.namespace_parts().last().map(|c| c.to_string()),
            RubyType::Array(_) => Some("Array".to_string()),
            RubyType::Hash(_, _) => Some("Hash".to_string()),
            RubyType::Union(_) | RubyType::Unknown => None,
        }?;
        crate::rbs::get_rbs_method_return_type_as_ruby_type(&class_name, method_name, is_singleton)
    }

    fn resolve_method_return_type_from_analysis(
        &self,
        receiver_type: &RubyType,
        method_name: &str,
        allow_private: bool,
    ) -> Option<RubyType> {
        let analysis_engine = self.analysis_engine.as_ref()?;
        let method = crate::core::RubyMethod::new(method_name).ok()?;
        if let Some(return_type) =
            self.local_method_return_type_for_receiver(receiver_type, &method, !allow_private)
        {
            return Some(return_type);
        }
        let (receiver_fqn, namespace_kind) = match receiver_type {
            RubyType::Class(fqn) | RubyType::Module(fqn) => {
                (fqn.clone(), crate::core::NamespaceKind::Instance)
            }
            RubyType::ClassReference(fqn) | RubyType::ModuleReference(fqn) => {
                (fqn.clone(), crate::core::NamespaceKind::Singleton)
            }
            RubyType::Array(_) | RubyType::Hash(_, _) | RubyType::Union(_) | RubyType::Unknown => {
                return None;
            }
        };
        let namespace =
            FullyQualifiedName::namespace_with_kind(receiver_fqn.namespace_parts(), namespace_kind);
        let engine = analysis_engine.read();
        let query = AnalysisQuery::new(&engine);
        if allow_private {
            self.analysis_query_cache.as_ref().map_or_else(
                || query.method_return_type_for_receiver(&namespace, &method),
                |cache| query.method_return_type_for_receiver_cached(&namespace, &method, cache),
            )
        } else if let Some(current_class) = self.current_class.as_ref() {
            let caller_namespace = FullyQualifiedName::namespace_with_kind(
                current_class.namespace_parts(),
                crate::core::NamespaceKind::Instance,
            );
            self.analysis_query_cache.as_ref().map_or_else(
                || {
                    query.method_return_type_for_protected_receiver(
                        &namespace,
                        &method,
                        &caller_namespace,
                    )
                },
                |cache| {
                    query.method_return_type_for_protected_receiver_cached(
                        &namespace,
                        &method,
                        &caller_namespace,
                        cache,
                    )
                },
            )
        } else {
            self.analysis_query_cache.as_ref().map_or_else(
                || query.method_return_type_for_public_receiver(&namespace, &method),
                |cache| {
                    query.method_return_type_for_public_receiver_cached(&namespace, &method, cache)
                },
            )
        }
    }

    fn infer_yielding_block_return_type(
        &mut self,
        call: &CallNode,
        method_name: &str,
    ) -> Option<RubyType> {
        let block = call.block()?.as_block_node()?;
        let method = RubyMethod::new(method_name).ok()?;
        let method_fqn = match call.receiver() {
            None => FullyQualifiedName::method(
                self.current_class
                    .as_ref()
                    .map(|fqn| fqn.namespace_parts())
                    .unwrap_or_default(),
                method,
            ),
            Some(receiver) if receiver.as_self_node().is_some() => FullyQualifiedName::method(
                self.current_class
                    .as_ref()
                    .map(|fqn| fqn.namespace_parts())
                    .unwrap_or_default(),
                method,
            ),
            Some(receiver) => {
                let receiver_type = self.infer_expression(&receiver);
                let parts = match receiver_type {
                    RubyType::Class(fqn)
                    | RubyType::Module(fqn)
                    | RubyType::ClassReference(fqn)
                    | RubyType::ModuleReference(fqn) => fqn.namespace_parts(),
                    RubyType::Array(_)
                    | RubyType::Hash(_, _)
                    | RubyType::Union(_)
                    | RubyType::Unknown => {
                        return None;
                    }
                };
                FullyQualifiedName::method(parts, method)
            }
        };
        let param_types = self.yield_param_types_by_method.get(&method_fqn)?.clone();
        if param_types.iter().all(|ty| *ty == RubyType::Unknown) {
            return None;
        }
        let param_names = block_parameter_names(&block);

        let env_before = self.vars.clone();
        for (index, name) in param_names.iter().enumerate() {
            if let Some(param_type) = param_types.get(index) {
                if *param_type != RubyType::Unknown {
                    self.vars.insert(name.clone(), param_type.clone());
                }
            }
        }
        let return_type = block
            .body()
            .map(|body| self.track_node(&body))
            .unwrap_or_else(RubyType::nil_class);
        self.vars = env_before;

        (return_type != RubyType::Unknown).then_some(return_type)
    }

    fn local_method_return_type_for_receiver(
        &self,
        receiver_type: &RubyType,
        method: &RubyMethod,
        require_public: bool,
    ) -> Option<RubyType> {
        let parts = match receiver_type {
            RubyType::Class(fqn)
            | RubyType::Module(fqn)
            | RubyType::ClassReference(fqn)
            | RubyType::ModuleReference(fqn) => fqn.namespace_parts(),
            RubyType::Array(_) | RubyType::Hash(_, _) | RubyType::Union(_) | RubyType::Unknown => {
                return None;
            }
        };
        let method_fqn = FullyQualifiedName::method(parts, method.clone());
        if require_public && !self.local_public_method_candidates.contains(&method_fqn) {
            return None;
        }
        self.local_method_returns
            .get(&method_fqn)
            .filter(|ty| **ty != RubyType::Unknown)
            .cloned()
    }

    fn infer_super_return_type(&self) -> RubyType {
        let Some(current_class) = self.current_class.as_ref() else {
            return RubyType::Unknown;
        };
        let Some(method) = self.current_method.as_ref() else {
            return RubyType::Unknown;
        };
        let namespace = FullyQualifiedName::namespace_with_kind(
            current_class.namespace_parts(),
            NamespaceKind::Instance,
        );

        if let Some(superclass) = self.local_superclasses.get(&namespace) {
            let super_method =
                FullyQualifiedName::method(superclass.namespace_parts(), method.clone());
            if let Some(return_type) = self
                .local_method_returns
                .get(&super_method)
                .filter(|ty| **ty != RubyType::Unknown)
            {
                return return_type.clone();
            }
        }

        let Some(analysis_engine) = self.analysis_engine.as_ref() else {
            return RubyType::Unknown;
        };
        let engine = analysis_engine.read();
        let query = AnalysisQuery::new(&engine);
        let Some(callee) = query.resolve_super_method_callee(&namespace, method) else {
            return RubyType::Unknown;
        };
        let super_method =
            FullyQualifiedName::method(callee.owner.namespace_parts(), method.clone());
        if let Some(return_type) = self
            .local_method_returns
            .get(&super_method)
            .filter(|ty| **ty != RubyType::Unknown)
        {
            return return_type.clone();
        }
        query
            .method_return_type_for_callee(&callee)
            .unwrap_or(RubyType::Unknown)
    }

    /// Infer the type of a return statement
    fn infer_return(&mut self, ret: &ReturnNode) -> RubyType {
        let return_type = if let Some(args) = ret.arguments() {
            let args_list: Vec<_> = args.arguments().iter().collect();
            if args_list.is_empty() {
                RubyType::nil_class()
            } else if args_list.len() == 1 {
                self.infer_expression(&args_list[0])
            } else {
                // Multiple return values become an array
                let types: Vec<RubyType> =
                    args_list.iter().map(|a| self.infer_expression(a)).collect();
                RubyType::Array(RubyType::canonical_union_members(types))
            }
        } else {
            RubyType::nil_class()
        };

        let returned_dependency = ret.arguments().and_then(|arguments| {
            let mut args = arguments.arguments().iter();
            let first = args.next();
            let has_exactly_one = first.is_some() && args.next().is_none();
            has_exactly_one
                .then(|| {
                    first.and_then(|value| {
                        let is_direct_recursive_value = value.as_call_node().is_some_and(|call| {
                            call_is_direct_recursive(&call, self.current_method.as_ref())
                        });
                        let dependency = self.return_term_dependency_for_node(&value)?;
                        self.should_track_return_dependency(
                            &dependency.0,
                            &return_type,
                            is_direct_recursive_value,
                        )
                        .then_some(dependency)
                    })
                })
                .flatten()
        });
        let approximation = match returned_dependency {
            Some((dependency, approximation)) => {
                self.observed_return_dependencies.insert(dependency);
                approximation
            }
            None => RecursiveReturnApproximation::from_ruby_type(return_type.clone()),
        };
        self.explicit_return_types.push(approximation);
        return_type
    }

    fn return_term_dependency_for_call(
        &self,
        call: &CallNode<'_>,
    ) -> Option<(FullyQualifiedName, RecursiveReturnApproximation)> {
        if self
            .direct_call_return_proofs
            .contains(&call.location().start_offset())
        {
            return None;
        }
        let dependency = self.implicit_self_call_fqn(call)?;

        if call_is_direct_recursive(call, self.current_method.as_ref()) {
            if let Some(approximation) = self.recursive_return_approximation.as_ref() {
                return Some((dependency, approximation.clone()));
            }
        }
        Some((dependency, RecursiveReturnApproximation::Bottom))
    }

    fn should_track_return_dependency(
        &self,
        dependency: &FullyQualifiedName,
        inferred_type: &RubyType,
        direct_recursive: bool,
    ) -> bool {
        direct_recursive
            || *inferred_type == RubyType::Unknown
            || self.local_method_candidates.contains(dependency)
            || self
                .analysis_engine
                .as_ref()
                .is_some_and(|engine| engine.read().has_method_return_equation(dependency))
    }

    fn return_term_dependency_for_node(
        &self,
        node: &Node<'_>,
    ) -> Option<(FullyQualifiedName, RecursiveReturnApproximation)> {
        if let Some(statements) = node.as_statements_node() {
            return statements
                .body()
                .iter()
                .last()
                .and_then(|last| self.return_term_dependency_for_node(&last));
        }
        if let Some(parentheses) = node.as_parentheses_node() {
            return parentheses
                .body()
                .and_then(|body| self.return_term_dependency_for_node(&body));
        }
        if let Some(call) = node.as_call_node() {
            return self.return_term_dependency_for_call(&call);
        }
        if let Some(read) = node.as_local_variable_read_node() {
            let name = String::from_utf8_lossy(read.name().as_slice());
            return self.local_return_terms.get(name.as_ref()).cloned();
        }
        if let Some(write) = node.as_local_variable_write_node() {
            let name = String::from_utf8_lossy(write.name().as_slice());
            return self.local_return_terms.get(name.as_ref()).cloned();
        }
        None
    }

    fn implicit_self_call_fqn(&self, call: &CallNode<'_>) -> Option<FullyQualifiedName> {
        if call
            .receiver()
            .is_some_and(|receiver| receiver.as_self_node().is_none())
        {
            return None;
        }
        let method_name = String::from_utf8_lossy(call.name().as_slice());
        let method = RubyMethod::new(method_name.as_ref()).ok()?;
        Some(FullyQualifiedName::method(
            self.current_class
                .as_ref()
                .map(FullyQualifiedName::namespace_parts)
                .unwrap_or_default(),
            method,
        ))
    }

    /// Resolve a constant path to an FQN (e.g., Foo::Bar::Baz)
    fn resolve_constant_path(const_path: &ConstantPathNode) -> Option<FullyQualifiedName> {
        let mut parts = Vec::new();

        // Get the child constant name
        if let Some(name_node) = const_path.name() {
            let name = String::from_utf8_lossy(name_node.as_slice()).to_string();
            parts.push(RubyConstant::new(&name).ok()?);
        }

        // Get parent parts recursively
        if let Some(parent) = const_path.parent() {
            if let Some(parent_path) = parent.as_constant_path_node() {
                if let Some(FullyQualifiedName::Constant(parent_parts)) =
                    Self::resolve_constant_path(&parent_path)
                {
                    let mut full_parts = parent_parts;
                    full_parts.extend(parts);
                    return Some(FullyQualifiedName::constant(full_parts));
                }
            } else if let Some(const_read) = parent.as_constant_read_node() {
                let parent_name = String::from_utf8_lossy(const_read.name().as_slice()).to_string();
                let mut full_parts = vec![RubyConstant::new(&parent_name).ok()?];
                full_parts.extend(parts);
                return Some(FullyQualifiedName::constant(full_parts));
            }
        } else {
            // No parent means this is a top-level constant
            return Some(FullyQualifiedName::constant(parts));
        }

        None
    }

    /// Merge another environment into this one
    ///
    /// Used at control flow join points (after if/case/while).
    /// Variables with different types are merged into unions.
    ///
    /// If `no_else_branch` is true, variables that only exist in one branch
    /// are assumed to be nil in the other branch.
    fn merge_env(&mut self, other_env: &HashMap<String, RubyType>, no_else_branch: bool) {
        // For each variable in other environment
        for (var, other_ty) in other_env {
            if let Some(this_ty) = self.vars.get(var) {
                // Variable exists in both - create union if types differ
                if this_ty != other_ty {
                    let union = RubyType::union(vec![this_ty.clone(), other_ty.clone()]);
                    self.vars.insert(var.clone(), union);
                }
            } else {
                // Variable only in other environment
                if no_else_branch {
                    // No else branch: variable might not be defined
                    let union = RubyType::union(vec![other_ty.clone(), RubyType::nil_class()]);
                    self.vars.insert(var.clone(), union);
                } else {
                    // Has else branch: variable was defined in else but not then
                    // Add with nil union
                    let union = RubyType::union(vec![other_ty.clone(), RubyType::nil_class()]);
                    self.vars.insert(var.clone(), union);
                }
            }
        }

        // Handle variables only in this environment (they might be nil in other)
        if no_else_branch {
            // If there's no else branch, variables in then branch might be undefined
            // when the condition is false
            for (var, this_ty) in self.vars.clone() {
                if !other_env.contains_key(&var) {
                    let union = RubyType::union(vec![this_ty, RubyType::nil_class()]);
                    self.vars.insert(var, union);
                }
            }
        } else {
            // Has else branch: variables in then but not else get nil union
            for (var, this_ty) in self.vars.clone() {
                if !other_env.contains_key(&var) {
                    let union = RubyType::union(vec![this_ty, RubyType::nil_class()]);
                    self.vars.insert(var, union);
                }
            }
        }
    }
}

fn normalized_method_name(method: &DefNode<'_>) -> RubyMethod {
    let source_name = String::from_utf8_lossy(method.name().as_slice());
    let semantic_name = if source_name.as_ref() == "initialize" {
        "new"
    } else {
        source_name.as_ref()
    };
    RubyMethod::new(semantic_name).unwrap_or_else(|error| {
        panic!(
            "INVARIANT VIOLATED: Prism produced invalid method name `{semantic_name}` while tracking a definition: {error}. \
             This is a bug because FactCollector validates method names before type inference. \
             Fix: keep method-name validation and TypeTracker invocation on the same definition."
        )
    })
}

fn ruby_truthiness(ruby_type: &RubyType) -> Truthiness {
    match ruby_type {
        RubyType::Unknown => Truthiness::Conditional,
        RubyType::Union(members) => {
            assert!(
                members.len() >= 2,
                "INVARIANT VIOLATED: RubyType::Union contains fewer than two members. This is a bug because RubyType::union must collapse empty and singleton inputs. Fix: construct unions only through the canonical RubyType helpers."
            );
            let has_falsy = members.iter().any(is_falsy_type);
            let has_truthy = members.iter().any(|member| !is_falsy_type(member));
            match (has_truthy, has_falsy) {
                (true, false) => Truthiness::AlwaysTruthy,
                (false, true) => Truthiness::AlwaysFalsy,
                (true, true) => Truthiness::Conditional,
                (false, false) => panic!(
                    "INVARIANT VIOLATED: a nonempty RubyType::Union has no truthy or falsy members. This is a bug because every concrete Ruby value has one truthiness class. Fix: update truthiness classification when adding a RubyType variant."
                ),
            }
        }
        ruby_type if is_falsy_type(ruby_type) => Truthiness::AlwaysFalsy,
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Array(_)
        | RubyType::Hash(_, _) => Truthiness::AlwaysTruthy,
    }
}

fn is_falsy_type(ruby_type: &RubyType) -> bool {
    let RubyType::Class(fqn) = ruby_type else {
        return false;
    };
    let parts = fqn.namespace_parts_slice();
    parts.len() == 1
        && matches!(
            parts
                .first()
                .expect("INVARIANT VIOLATED: a one-part FQN lost its first part while classifying Ruby truthiness. This is a bug because the immutable slice was checked immediately before access. Fix: keep the length check and access in one expression.")
                .as_str(),
            "FalseClass" | "NilClass"
        )
}

fn short_circuit_result_type(
    left_type: RubyType,
    right_type: RubyType,
    operator: ShortCircuitOperator,
) -> RubyType {
    if left_type == RubyType::Unknown {
        return RubyType::Unknown;
    }

    let RubyType::Union(left_members) = left_type else {
        panic!(
            "INVARIANT VIOLATED: conditional short-circuit evaluation received a non-union concrete left type. This is a bug because one concrete Ruby class is always truthy or always falsy. Fix: keep ruby_truthiness and short-circuit projection exhaustive over the same RubyType variants."
        );
    };
    let mut result_members = left_members
        .into_iter()
        .filter(|member| match operator {
            ShortCircuitOperator::And => is_falsy_type(member),
            ShortCircuitOperator::Or => !is_falsy_type(member),
        })
        .collect::<Vec<_>>();
    assert!(
        !result_members.is_empty(),
        "INVARIANT VIOLATED: conditional short-circuit evaluation has no left-side result member. This is a bug because Conditional requires both an executing and a short-circuiting path. Fix: keep truthiness classification and member projection symmetric."
    );
    result_members.push(right_type);
    RubyType::union(result_members)
}

fn call_is_direct_recursive(call: &CallNode<'_>, method: Option<&RubyMethod>) -> bool {
    let Some(method) = method else {
        return false;
    };
    call.name().as_slice() == method.as_str().as_bytes()
        && call
            .receiver()
            .is_none_or(|receiver| receiver.as_self_node().is_some())
}

fn join_recursive_return_approximations(
    alternatives: impl IntoIterator<Item = RecursiveReturnApproximation>,
) -> RecursiveReturnApproximation {
    let mut proven = Vec::new();
    for alternative in alternatives {
        match alternative {
            RecursiveReturnApproximation::Bottom => {}
            RecursiveReturnApproximation::Proven(ruby_type) => proven.push(ruby_type),
            RecursiveReturnApproximation::Unknown => {
                return RecursiveReturnApproximation::Unknown;
            }
        }
    }
    if proven.is_empty() {
        RecursiveReturnApproximation::Bottom
    } else {
        RecursiveReturnApproximation::Proven(RubyType::union(proven))
    }
}

/// Join branch result types into the surrounding expression's type, excluding
/// branches that always diverge (return/raise/break/...). The join point is
/// never reached via a diverging branch, so its result type is irrelevant.
///
/// All branches diverge → `Unknown` (Bottom-equivalent; downstream consumers
/// already treat this as "no information").
fn join_branch_types(branches: &[(RubyType, bool)]) -> RubyType {
    let surviving: Vec<RubyType> = branches
        .iter()
        .filter(|(_, diverges)| !*diverges)
        .map(|(ty, _)| ty.clone())
        .collect();
    if surviving.is_empty() {
        RubyType::Unknown
    } else {
        RubyType::union(surviving)
    }
}

/// Add the path taken when no `when` clause in an ordinary Ruby `case`
/// matches and no `else` is present. That path preserves the incoming local
/// environment and the expression evaluates to `nil`.
///
/// Do not use this for `case ... in`: an unmatched pattern case without an
/// `else` raises `NoMatchingPatternError`, so its unmatched path diverges.
fn push_unmatched_ordinary_case_path(
    branches: &mut Vec<(HashMap<String, RubyType>, RubyType, bool)>,
    env_before: &HashMap<String, RubyType>,
) {
    branches.push((env_before.clone(), RubyType::nil_class(), false));
}

fn block_parameter_names(block: &BlockNode<'_>) -> Vec<String> {
    let Some(parameters_node) = block.parameters() else {
        return Vec::new();
    };
    if let Some(numbered) = parameters_node.as_numbered_parameters_node() {
        return numbered_parameter_names(numbered);
    }
    let Some(parameters) = parameters_node
        .as_block_parameters_node()
        .and_then(|node| node.parameters())
    else {
        return Vec::new();
    };

    let mut names = Vec::new();
    for required in parameters.requireds().iter() {
        if let Some(param) = required.as_required_parameter_node() {
            names.push(String::from_utf8_lossy(param.name().as_slice()).to_string());
        }
    }
    for optional in parameters.optionals().iter() {
        if let Some(param) = optional.as_optional_parameter_node() {
            names.push(String::from_utf8_lossy(param.name().as_slice()).to_string());
        }
    }
    if let Some(rest) = parameters.rest() {
        if let Some(param) = rest.as_rest_parameter_node() {
            if let Some(name) = param.name() {
                names.push(String::from_utf8_lossy(name.as_slice()).to_string());
            }
        }
    }
    for post in parameters.posts().iter() {
        if let Some(param) = post.as_required_parameter_node() {
            names.push(String::from_utf8_lossy(param.name().as_slice()).to_string());
        }
    }
    names
}

fn numbered_parameter_names(params: NumberedParametersNode<'_>) -> Vec<String> {
    (1..=usize::from(params.maximum()))
        .map(|index| format!("_{index}"))
        .collect()
}

fn pattern_symbol_key(node: &Node<'_>) -> Option<String> {
    node.as_symbol_node()
        .map(|symbol| String::from_utf8_lossy(symbol.unescaped()).to_string())
}

/// Helper to get type at offset from var_types BTreeMap
pub fn get_var_type_at(
    var_types: &BTreeMap<usize, HashMap<String, RubyType>>,
    offset: usize,
    var_name: &str,
) -> Option<RubyType> {
    var_types
        .range(..=offset)
        .next_back()
        .and_then(|(_, vars)| vars.get(var_name).cloned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_tracker<'a>(source: &'a str) -> TypeTracker<'a> {
        TypeTracker::new(source.as_bytes())
    }

    fn instance_type(name: &str) -> RubyType {
        RubyType::Class(FullyQualifiedName::constant(vec![
            RubyConstant::new(name).expect("test class name must be a valid Ruby constant")
        ]))
    }

    fn exact_local_read_type(
        tracker: &mut TypeTracker<'_>,
        source: &str,
        needle: &str,
    ) -> RubyType {
        let start_offset = source.rfind(needle).expect(
            "INVARIANT VIOLATED: the test local-read needle is absent. This is a bug because the fixture and assertion must identify the same source token. Fix: keep the needle synchronized with the fixture.",
        );
        tracker
            .take_local_read_types()
            .into_iter()
            .find(|read| read.start_offset == start_offset)
            .map(|read| read.ruby_type)
            .expect(
                "INVARIANT VIOLATED: TypeTracker did not retain the expected exact local read. This is a bug because the fixture places it inside rescue control flow. Fix: keep local-read evidence enabled and traverse the rescue expression through track_node.",
            )
    }

    #[test]
    fn test_simple_method_tracking() {
        let source = "def foo\n  5\nend";
        let mut tracker = create_test_tracker(source);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        let return_type = tracker.track_method(&def_node);

        assert_eq!(return_type, RubyType::integer());
    }

    #[test]
    fn direct_recursive_return_uses_the_least_proven_fixed_point() {
        let source = r#"def count_down(n)
  return 0 if n == 0
  count_down(n - 1)
end"#;
        let mut tracker = create_test_tracker(source);
        let parse_result = ruby_prism::parse(source.as_bytes());
        let def_node = parse_result
            .node()
            .as_program_node()
            .unwrap()
            .statements()
            .body()
            .iter()
            .next()
            .unwrap()
            .as_def_node()
            .unwrap();

        let outcome = tracker.track_method_outcome(&def_node);

        assert_eq!(outcome.proven_type(), Some(&RubyType::integer()));
        assert_eq!(outcome.unknown_reason(), None);
    }

    #[test]
    fn recursive_cycle_without_a_base_stays_explainable_unknown() {
        let source = "def forever\n  forever\nend";
        let mut tracker = create_test_tracker(source);
        let parse_result = ruby_prism::parse(source.as_bytes());
        let def_node = parse_result
            .node()
            .as_program_node()
            .unwrap()
            .statements()
            .body()
            .iter()
            .next()
            .unwrap()
            .as_def_node()
            .unwrap();

        let outcome = tracker.track_method_outcome(&def_node);

        assert_eq!(outcome.proven_type(), None);
        assert_eq!(
            outcome.unknown_reason(),
            Some(UnknownReason::UnprovenRecursiveCycle)
        );
    }

    #[test]
    fn mutual_return_equations_do_not_require_preinserted_method_facts() {
        let source = r#"def left
  right
end

def right
  left
end"#;
        let parse_result = ruby_prism::parse(source.as_bytes());
        let statements = parse_result.node().as_program_node().unwrap().statements();
        let left = FullyQualifiedName::method(
            Vec::new(),
            RubyMethod::new("left").expect("test method name must be valid"),
        );
        let right = FullyQualifiedName::method(
            Vec::new(),
            RubyMethod::new("right").expect("test method name must be valid"),
        );
        let mut equations = Vec::new();
        for (node, method) in statements.body().iter().zip([left.clone(), right.clone()]) {
            let definition = node
                .as_def_node()
                .expect("test statement must be a method definition");
            equations.push(TypeTracker::new(source.as_bytes()).track_method_equation(
                &definition,
                method,
                Arc::new(HashSet::new()),
            ));
        }

        let solved = crate::inference::method::recursive::solve_method_return_equations(&equations);

        assert_eq!(
            solved
                .get(&left)
                .and_then(TypeInferenceOutcome::unknown_reason),
            Some(UnknownReason::UnprovenRecursiveCycle)
        );
        assert_eq!(
            solved
                .get(&right)
                .and_then(TypeInferenceOutcome::unknown_reason),
            Some(UnknownReason::UnprovenRecursiveCycle)
        );
    }

    #[test]
    fn modeled_block_result_is_not_replaced_by_callee_return_dependency() {
        let source = r#"def label
  with_value { 1 }
end"#;
        let parse_result = ruby_prism::parse(source.as_bytes());
        let definition = parse_result
            .node()
            .as_program_node()
            .unwrap()
            .statements()
            .body()
            .iter()
            .next()
            .unwrap()
            .as_def_node()
            .unwrap();
        let with_value = FullyQualifiedName::method(
            Vec::new(),
            RubyMethod::new("with_value").expect("test method name must be valid"),
        );
        let label = FullyQualifiedName::method(
            Vec::new(),
            RubyMethod::new("label").expect("test method name must be valid"),
        );
        let mut yield_types = HashMap::new();
        yield_types.insert(with_value.clone(), vec![RubyType::integer()]);
        let equation = TypeTracker::new(source.as_bytes())
            .with_yield_param_types(yield_types)
            .track_method_equation(&definition, label, Arc::new(HashSet::from([with_value])));

        assert_eq!(
            equation.immediate_outcome().proven_type(),
            Some(&RubyType::integer())
        );
    }

    #[test]
    fn unresolved_recursive_base_stays_explainable_unknown() {
        let source = r#"def resolve(flag)
  return missing_value if flag
  resolve(flag)
end"#;
        let mut tracker = create_test_tracker(source);
        let parse_result = ruby_prism::parse(source.as_bytes());
        let def_node = parse_result
            .node()
            .as_program_node()
            .unwrap()
            .statements()
            .body()
            .iter()
            .next()
            .unwrap()
            .as_def_node()
            .unwrap();

        let outcome = tracker.track_method_outcome(&def_node);

        assert_eq!(outcome.proven_type(), None);
        assert_eq!(
            outcome.unknown_reason(),
            Some(UnknownReason::UnprovenRecursiveCycle)
        );
    }

    #[test]
    fn explicit_and_fallthrough_returns_are_both_inferred() {
        let source = r#"def value(flag)
  return 1 if flag
  "text"
end"#;
        let mut tracker = create_test_tracker(source);
        let parse_result = ruby_prism::parse(source.as_bytes());
        let def_node = parse_result
            .node()
            .as_program_node()
            .unwrap()
            .statements()
            .body()
            .iter()
            .next()
            .unwrap()
            .as_def_node()
            .unwrap();

        assert_eq!(
            tracker.track_method(&def_node),
            RubyType::union([RubyType::integer(), RubyType::string()])
        );
    }

    #[test]
    fn test_local_variable_assignment() {
        let source = "def foo\n  x = 5\n  x\nend";
        let mut tracker = create_test_tracker(source);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);
        let var_types = tracker.into_var_types();

        // Check that var_types were recorded
        assert!(!var_types.is_empty());

        // Find the assignment offset (after "x = 5")
        let assignment_end_offset = source.find("x = 5").unwrap() + "x = 5".len();

        // Query type after assignment
        let x_type = get_var_type_at(&var_types, assignment_end_offset, "x");
        assert_eq!(x_type, Some(RubyType::integer()));
    }

    #[test]
    fn test_multiple_assignments() {
        let source = "def foo\n  x = 5\n  y = \"hello\"\n  x\nend";
        let mut tracker = create_test_tracker(source);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);
        let var_types = tracker.into_var_types();

        // Find offset after both assignments
        let second_assignment_end = source.find("y = \"hello\"").unwrap() + "y = \"hello\"".len();

        // Both variables should be in the environment
        let x_type = get_var_type_at(&var_types, second_assignment_end, "x");
        let y_type = get_var_type_at(&var_types, second_assignment_end, "y");

        assert_eq!(x_type, Some(RubyType::integer()));
        assert_eq!(y_type, Some(RubyType::string()));
    }

    #[test]
    fn test_reassignment_changes_type() {
        let source = "def foo\n  x = 5\n  x = \"hello\"\n  x\nend";
        let mut tracker = create_test_tracker(source);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);
        let var_types = tracker.into_var_types();

        // After first assignment, should be Integer
        let first_assignment_end = source.find("x = 5").unwrap() + "x = 5".len();
        let x_type_1 = get_var_type_at(&var_types, first_assignment_end, "x");
        assert_eq!(x_type_1, Some(RubyType::integer()));

        // After second assignment, should be String
        let second_assignment_end = source.find("x = \"hello\"").unwrap() + "x = \"hello\"".len();
        let x_type_2 = get_var_type_at(&var_types, second_assignment_end, "x");
        assert_eq!(x_type_2, Some(RubyType::string()));
    }

    #[test]
    fn short_circuit_and_joins_executed_and_skipped_assignment_paths() {
        let source = r#"def foo(flag)
  value = 1
  flag && (value = "fallback")
  value
end"#;
        let parse_result = ruby_prism::parse(source.as_bytes());
        let def_node = parse_result
            .node()
            .as_program_node()
            .unwrap()
            .statements()
            .body()
            .iter()
            .next()
            .unwrap()
            .as_def_node()
            .unwrap();
        let mut tracker = create_test_tracker(source);

        assert_eq!(
            tracker.track_method(&def_node),
            RubyType::union([RubyType::integer(), RubyType::string()]),
            "an unknown left operand makes both the skipped and executed right-operand states reachable"
        );
    }

    #[test]
    fn short_circuit_or_joins_executed_and_skipped_assignment_paths() {
        let source = r#"def foo(flag)
  value = 1
  flag or (value = "fallback")
  value
end"#;
        let parse_result = ruby_prism::parse(source.as_bytes());
        let def_node = parse_result
            .node()
            .as_program_node()
            .unwrap()
            .statements()
            .body()
            .iter()
            .next()
            .unwrap()
            .as_def_node()
            .unwrap();
        let mut tracker = create_test_tracker(source);

        assert_eq!(
            tracker.track_method(&def_node),
            RubyType::union([RubyType::integer(), RubyType::string()]),
            "an unknown left operand makes both the skipped and executed right-operand states reachable"
        );
    }

    #[test]
    fn short_circuit_result_keeps_only_the_left_members_that_return() {
        let and_source = "def foo(flag)\n  flag && \"fallback\"\nend";
        let and_parse = ruby_prism::parse(and_source.as_bytes());
        let and_method = and_parse
            .node()
            .as_program_node()
            .unwrap()
            .statements()
            .body()
            .iter()
            .next()
            .unwrap()
            .as_def_node()
            .unwrap();
        let mut and_tracker = create_test_tracker(and_source)
            .with_parameter_types(HashMap::from([("flag".to_string(), RubyType::boolean())]));
        assert_eq!(
            and_tracker.track_method(&and_method),
            RubyType::union([RubyType::false_class(), RubyType::string()])
        );

        let or_source = "def foo(flag)\n  flag || \"fallback\"\nend";
        let or_parse = ruby_prism::parse(or_source.as_bytes());
        let or_method = or_parse
            .node()
            .as_program_node()
            .unwrap()
            .statements()
            .body()
            .iter()
            .next()
            .unwrap()
            .as_def_node()
            .unwrap();
        let mut or_tracker = create_test_tracker(or_source)
            .with_parameter_types(HashMap::from([("flag".to_string(), RubyType::boolean())]));
        assert_eq!(
            or_tracker.track_method(&or_method),
            RubyType::union([RubyType::string(), RubyType::true_class()])
        );
    }

    #[test]
    fn rescue_entry_joins_values_before_and_after_each_protected_assignment() {
        let source = r#"def foo
  value = Product.new
  begin
    value = Text.new
    dangerous
  rescue
    value
  end
end"#;
        let parse_result = ruby_prism::parse(source.as_bytes());
        let def_node = parse_result
            .node()
            .as_program_node()
            .unwrap()
            .statements()
            .body()
            .iter()
            .next()
            .unwrap()
            .as_def_node()
            .unwrap();
        let mut tracker = create_test_tracker(source).with_local_read_types();

        tracker.track_method(&def_node);

        assert_eq!(
            exact_local_read_type(&mut tracker, source, "value\n  end"),
            RubyType::union([instance_type("Product"), instance_type("Text")])
        );
    }

    #[test]
    fn rescue_modifier_uses_the_same_assignment_prefix_join() {
        let source = r#"def foo
  value = Product.new
  ((value = Text.new; dangerous) rescue value)
end"#;
        let parse_result = ruby_prism::parse(source.as_bytes());
        let def_node = parse_result
            .node()
            .as_program_node()
            .unwrap()
            .statements()
            .body()
            .iter()
            .next()
            .unwrap()
            .as_def_node()
            .unwrap();
        let mut tracker = create_test_tracker(source).with_local_read_types();

        tracker.track_method(&def_node);

        assert_eq!(
            exact_local_read_type(&mut tracker, source, "value)"),
            RubyType::union([instance_type("Product"), instance_type("Text")])
        );
    }

    #[test]
    fn unresolved_protected_assignment_absorbs_the_rescue_receiver_proof() {
        let source = r#"def foo
  value = Product.new
  begin
    value = dynamic_value
    dangerous
  rescue
    value
  end
end"#;
        let parse_result = ruby_prism::parse(source.as_bytes());
        let def_node = parse_result
            .node()
            .as_program_node()
            .unwrap()
            .statements()
            .body()
            .iter()
            .next()
            .unwrap()
            .as_def_node()
            .unwrap();
        let mut tracker = create_test_tracker(source).with_local_read_types();

        tracker.track_method(&def_node);

        assert_eq!(
            exact_local_read_type(&mut tracker, source, "value\n  end"),
            RubyType::Unknown,
            "one unproven assignment value must absorb every concrete rescue-entry alternative"
        );
    }

    #[test]
    fn test_if_with_else() {
        let source = r#"def foo
  if true
    x = 5
  else
    x = "hello"
  end
  x
end"#;
        let mut tracker = create_test_tracker(source);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);
        let var_types = tracker.into_var_types();

        // After the if statement, x should be Integer | String
        let after_if = source.find("end\n  x").unwrap() + "end".len();
        let x_type = get_var_type_at(&var_types, after_if, "x");

        // Should be a union type containing both Integer and String
        assert!(x_type.is_some());
        let x_type = x_type.unwrap();
        assert!(matches!(x_type, RubyType::Union(_)));
    }

    #[test]
    fn test_if_without_else() {
        let source = r#"def foo
  if true
    x = 5
  end
  x
end"#;
        let mut tracker = create_test_tracker(source);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);
        let var_types = tracker.into_var_types();

        // After the if statement, x should be Integer | NilClass (might not be defined)
        let after_if = source.find("end\n  x").unwrap() + "end".len();
        let x_type = get_var_type_at(&var_types, after_if, "x");

        // Should be a union type containing Integer and NilClass
        assert!(x_type.is_some());
        let x_type = x_type.unwrap();
        assert!(matches!(x_type, RubyType::Union(_)));
    }

    #[test]
    fn test_unless_statement() {
        let source = r#"def foo
  unless false
    x = 5
  else
    x = "hello"
  end
  x
end"#;
        let mut tracker = create_test_tracker(source);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);
        let var_types = tracker.into_var_types();

        // After the unless statement, x should be Integer | String
        let after_unless = source.find("end\n  x").unwrap() + "end".len();
        let x_type = get_var_type_at(&var_types, after_unless, "x");

        assert!(x_type.is_some());
        let x_type = x_type.unwrap();
        assert!(matches!(x_type, RubyType::Union(_)));
    }

    #[test]
    fn test_elsif_chain() {
        let source = r#"def foo
  if true
    x = 5
  elsif false
    x = "hello"
  else
    x = 3.14
  end
  x
end"#;
        let mut tracker = create_test_tracker(source);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);
        let var_types = tracker.into_var_types();

        // After the if/elsif/else, x should be a union of all three types
        let after_if = source.find("end\n  x").unwrap() + "end".len();
        let x_type = get_var_type_at(&var_types, after_if, "x");

        assert!(x_type.is_some());
        let x_type = x_type.unwrap();
        assert!(matches!(x_type, RubyType::Union(_)));
    }

    #[test]
    fn test_case_with_else() {
        let source = r#"def foo
  case value
  when 1
    x = 5
  when 2
    x = "hello"
  else
    x = 3.14
  end
  x
end"#;
        let mut tracker = create_test_tracker(source);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);
        let var_types = tracker.into_var_types();

        // After the case statement, x should be Integer | String | Float
        let after_case = source.find("end\n  x").unwrap() + "end".len();
        let x_type = get_var_type_at(&var_types, after_case, "x");

        assert!(x_type.is_some());
        let x_type = x_type.unwrap();
        assert!(matches!(x_type, RubyType::Union(_)));
    }

    #[test]
    fn test_case_without_else() {
        let source = r#"def foo
  case value
  when 1
    x = 5
  when 2
    x = "hello"
  end
  x
end"#;
        let mut tracker = create_test_tracker(source);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);
        let var_types = tracker.into_var_types();

        // After the case statement, x should be Integer | String | NilClass
        let after_case = source.find("end\n  x").unwrap() + "end".len();
        let x_type = get_var_type_at(&var_types, after_case, "x");

        assert!(x_type.is_some());
        let x_type = x_type.unwrap();
        assert!(matches!(x_type, RubyType::Union(_)));
    }

    #[test]
    fn case_without_else_preserves_the_pre_case_binding_on_the_unmatched_path() {
        let source = r#"def choose(value)
  result = 1
  case value
  when :ready
    result = "ready"
  end
  result
end"#;
        let mut tracker = create_test_tracker(source);
        let parse_result = ruby_prism::parse(source.as_bytes());
        let definition = parse_result
            .node()
            .as_program_node()
            .unwrap()
            .statements()
            .body()
            .iter()
            .next()
            .unwrap()
            .as_def_node()
            .unwrap();

        let return_type = tracker.track_method(&definition);
        let var_types = tracker.into_var_types();
        let after_case = source.find("end\n  result").unwrap() + "end".len();
        let expected = RubyType::union([RubyType::integer(), RubyType::string()]);

        assert_eq!(return_type, expected);
        assert_eq!(
            get_var_type_at(&var_types, after_case, "result"),
            Some(expected)
        );
    }

    #[test]
    fn case_without_else_includes_the_unmatched_nil_result() {
        let source = r#"def choose(value)
  case value
  when :ready
    "ready"
  end
end"#;
        let mut tracker = create_test_tracker(source);
        let parse_result = ruby_prism::parse(source.as_bytes());
        let definition = parse_result
            .node()
            .as_program_node()
            .unwrap()
            .statements()
            .body()
            .iter()
            .next()
            .unwrap()
            .as_def_node()
            .unwrap();

        assert_eq!(
            tracker.track_method(&definition),
            RubyType::union([RubyType::nil_class(), RubyType::string()]),
            "an unmatched ordinary case path must contribute Ruby's nil result"
        );
    }

    #[test]
    fn pattern_case_without_else_keeps_only_reaching_branch_types() {
        let source = r#"def choose
  case { name: "Ada" }
  in { name: value }
    value
  end
  value
end"#;
        let mut tracker = create_test_tracker(source);
        let parse_result = ruby_prism::parse(source.as_bytes());
        let definition = parse_result
            .node()
            .as_program_node()
            .unwrap()
            .statements()
            .body()
            .iter()
            .next()
            .unwrap()
            .as_def_node()
            .unwrap();

        let return_type = tracker.track_method(&definition);
        let var_types = tracker.into_var_types();
        let after_case = source.find("end\n  value").unwrap() + "end".len();

        assert_eq!(return_type, RubyType::string());
        assert_eq!(
            get_var_type_at(&var_types, after_case, "value"),
            Some(RubyType::string()),
            "the unmatched pattern path raises, so it cannot contribute NilClass at the join"
        );
    }

    #[test]
    fn pattern_case_explicit_else_keeps_nil_for_an_unbound_capture() {
        let source = r#"def choose
  case { name: "Ada" }
  in { name: value }
    value
  else
    nil
  end
  value
end"#;
        let mut tracker = create_test_tracker(source);
        let parse_result = ruby_prism::parse(source.as_bytes());
        let definition = parse_result
            .node()
            .as_program_node()
            .unwrap()
            .statements()
            .body()
            .iter()
            .next()
            .unwrap()
            .as_def_node()
            .unwrap();
        let expected = RubyType::union([RubyType::nil_class(), RubyType::string()]);

        let return_type = tracker.track_method(&definition);
        let var_types = tracker.into_var_types();
        let after_case = source.find("end\n  value").unwrap() + "end".len();

        assert_eq!(return_type, expected);
        assert_eq!(
            get_var_type_at(&var_types, after_case, "value"),
            Some(expected),
            "an explicit else reaches the join without binding the pattern capture"
        );
    }

    #[test]
    fn pattern_case_branch_specific_capture_remains_nilable() {
        let source = r#"def choose
  case { name: "Ada" }
  in { name: value }
    value
  in { age: age }
    age
  end
  value
end"#;
        let mut tracker = create_test_tracker(source);
        let parse_result = ruby_prism::parse(source.as_bytes());
        let definition = parse_result
            .node()
            .as_program_node()
            .unwrap()
            .statements()
            .body()
            .iter()
            .next()
            .unwrap()
            .as_def_node()
            .unwrap();
        let expected = RubyType::union([RubyType::nil_class(), RubyType::string()]);

        let return_type = tracker.track_method(&definition);
        let var_types = tracker.into_var_types();
        let after_case = source.find("end\n  value").unwrap() + "end".len();

        assert_eq!(return_type, expected);
        assert_eq!(
            get_var_type_at(&var_types, after_case, "value"),
            Some(expected),
            "a different reachable in-branch may leave this capture unbound"
        );
    }

    #[test]
    fn test_case_single_branch() {
        let source = r#"def foo
  case value
  when 1
    x = 5
  end
  x
end"#;
        let mut tracker = create_test_tracker(source);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);
        let var_types = tracker.into_var_types();

        // After the case statement, x should be Integer | NilClass
        let after_case = source.find("end\n  x").unwrap() + "end".len();
        let x_type = get_var_type_at(&var_types, after_case, "x");

        assert!(x_type.is_some());
        let x_type = x_type.unwrap();
        assert!(matches!(x_type, RubyType::Union(_)));
    }

    #[test]
    fn test_while_loop() {
        let source = r#"def foo
  x = 0
  while true
    x = 5
  end
  x
end"#;
        let mut tracker = create_test_tracker(source);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);
        let var_types = tracker.into_var_types();

        // After the while loop, x should be Integer (0 or 5)
        let after_while = source.find("end\n  x").unwrap() + "end".len();
        let x_type = get_var_type_at(&var_types, after_while, "x");

        assert!(x_type.is_some());
        // Type should still be Integer (union of Integer | Integer = Integer)
    }

    #[test]
    fn test_until_loop() {
        let source = r#"def foo
  x = 0
  until false
    x = "hello"
  end
  x
end"#;
        let mut tracker = create_test_tracker(source);

        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let program = root.as_program_node().unwrap();
        let stmts = program.statements();
        let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

        tracker.track_method(&def_node);
        let var_types = tracker.into_var_types();

        // After the until loop, x should be Integer | String
        let after_until = source.find("end\n  x").unwrap() + "end".len();
        let x_type = get_var_type_at(&var_types, after_until, "x");

        assert!(x_type.is_some());
        let x_type = x_type.unwrap();
        assert!(matches!(x_type, RubyType::Union(_)));
    }

    #[test]
    fn nested_loop_stabilization_is_linear_not_exponential() {
        let source = r#"def parse
  while outer
    until middle
      while inner
        value = 1
      end
    end
  end
end"#;
        let parse_result = ruby_prism::parse(source.as_bytes());
        let root = parse_result.node();
        let def_node = root
            .as_program_node()
            .unwrap()
            .statements()
            .body()
            .iter()
            .next()
            .unwrap()
            .as_def_node()
            .unwrap();
        let mut tracker = create_test_tracker(source);
        tracker.max_loop_iterations = 3;

        tracker.track_method(&def_node);

        assert_eq!(tracker.loop_body_passes, 9);
    }
}
