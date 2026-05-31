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
use crate::core::{FullyQualifiedName, NamespaceKind, RubyConstant, RubyMethod};
use crate::engine::{AnalysisEngine, AnalysisQuery};
use crate::r#type::literal::LiteralAnalyzer;
use crate::r#type::ruby::RubyType;
use parking_lot::RwLock;
use ruby_prism::*;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// Simple forward type tracker with control flow merging.
///
/// Performs a single forward pass through a method's AST, tracking variable
/// types and creating snapshots at each statement. Handles control flow by
/// cloning the environment for branches and merging at join points.
pub struct TypeTracker<'a> {
    /// Current type environment (variable name → type)
    vars: HashMap<String, RubyType>,

    /// Variable types at each offset (for queries)
    /// Key = offset where state was recorded, Value = all variables and their types
    var_types: BTreeMap<usize, HashMap<String, RubyType>>,

    /// Source code (for offset calculations)
    #[allow(dead_code)]
    source: &'a [u8],

    /// Literal analyzer (for static type inference)
    literal_analyzer: LiteralAnalyzer,

    /// Engine for method return type lookups on analysis path
    analysis_engine: Option<Arc<RwLock<AnalysisEngine>>>,

    /// Max loop iterations (to prevent infinite loops)
    max_loop_iterations: usize,

    /// Current class/module context for resolving implicit self
    current_class: Option<FullyQualifiedName>,

    /// Current method context for resolving `super`.
    current_method: Option<RubyMethod>,

    /// Same-file method return facts already collected before this method.
    local_method_returns: HashMap<FullyQualifiedName, RubyType>,

    /// Same-file superclass edges already collected before this method.
    local_superclasses: HashMap<FullyQualifiedName, FullyQualifiedName>,

    /// Same-file methods that contain `yield`, keyed by method FQN.
    yield_param_types_by_method: HashMap<FullyQualifiedName, Vec<RubyType>>,

    /// Local variables assigned lambda/proc literals, keyed by local name.
    proc_return_types_by_local: HashMap<String, RubyType>,
}

impl<'a> TypeTracker<'a> {
    /// Create a new type tracker for the given source.
    pub fn new(source: &'a [u8]) -> Self {
        Self {
            vars: HashMap::new(),
            var_types: BTreeMap::new(),
            source,
            literal_analyzer: LiteralAnalyzer::new(),
            analysis_engine: None,
            max_loop_iterations: 10,
            current_class: None,
            current_method: None,
            local_method_returns: HashMap::new(),
            local_superclasses: HashMap::new(),
            yield_param_types_by_method: HashMap::new(),
            proc_return_types_by_local: HashMap::new(),
        }
    }

    pub fn with_analysis_engine(mut self, analysis_engine: Arc<RwLock<AnalysisEngine>>) -> Self {
        self.analysis_engine = Some(analysis_engine);
        self
    }

    pub fn with_local_method_returns(
        mut self,
        local_method_returns: HashMap<FullyQualifiedName, RubyType>,
    ) -> Self {
        self.local_method_returns = local_method_returns;
        self
    }

    pub fn with_local_superclasses(
        mut self,
        local_superclasses: HashMap<FullyQualifiedName, FullyQualifiedName>,
    ) -> Self {
        self.local_superclasses = local_superclasses;
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
        let previous_method = self.current_method.clone();
        let method_name = String::from_utf8_lossy(method.name().as_slice());
        let method_name = if method_name.as_ref() == "initialize" {
            "new"
        } else {
            method_name.as_ref()
        };
        self.current_method = RubyMethod::new(method_name).ok();

        // Add parameters to environment
        if let Some(params) = method.parameters() {
            self.add_parameters(&params);
        }

        // Track method body
        let return_type = if let Some(body) = method.body() {
            self.track_node(&body)
        } else {
            RubyType::nil_class()
        };

        // Record final state at method end
        if let Some(body) = method.body() {
            let end_offset = body.location().end_offset();
            self.record_state(end_offset);
        }

        self.current_method = previous_method;
        return_type
    }

    /// Add method parameters to the type environment
    fn add_parameters(&mut self, _params: &ParametersNode) {
        // TODO: Extract parameter types from YARD/RBS or infer from usage
        // For now, parameters default to Unknown
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
                self.track_if(&if_node)
            }

            _ if node.as_unless_node().is_some() => {
                let unless_node = node.as_unless_node().unwrap();
                self.track_unless(&unless_node)
            }

            // Case statement
            _ if node.as_case_node().is_some() => {
                let case_node = node.as_case_node().unwrap();
                self.track_case(&case_node)
            }

            _ if node.as_case_match_node().is_some() => {
                let case_match_node = node.as_case_match_node().unwrap();
                self.track_case_match(&case_match_node)
            }

            _ if node.as_super_node().is_some() || node.as_forwarding_super_node().is_some() => {
                self.infer_super_return_type()
            }

            // Begin/rescue/ensure expressions
            _ if node.as_begin_node().is_some() => {
                let begin_node = node.as_begin_node().unwrap();
                self.track_begin(&begin_node)
            }

            _ if node.as_rescue_modifier_node().is_some() => {
                let rescue_modifier = node.as_rescue_modifier_node().unwrap();
                self.track_rescue_modifier(&rescue_modifier)
            }

            // Loops
            _ if node.as_while_node().is_some() => {
                let while_node = node.as_while_node().unwrap();
                self.track_while(&while_node)
            }

            _ if node.as_until_node().is_some() => {
                let until_node = node.as_until_node().unwrap();
                self.track_until(&until_node)
            }

            // Default: try to infer expression type
            _ => self.infer_expression(node),
        }
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

        // Infer type from value
        let value = write.value();
        let var_type = self.track_node(&value);
        if let Some(return_type) = self.infer_proc_literal_return_type(&value) {
            self.proc_return_types_by_local
                .insert(var_name.clone(), return_type);
        } else {
            self.proc_return_types_by_local.remove(&var_name);
        }

        // Update environment
        self.vars.insert(var_name, var_type.clone());

        // Return the assigned type (assignments return their value in Ruby)
        var_type
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
            if !has_else {
                for (var, ty) in self.vars.clone() {
                    let union = RubyType::union(vec![ty, RubyType::nil_class()]);
                    self.vars.insert(var, union);
                }
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
            if !has_else {
                for (var, ty) in self.vars.clone() {
                    let union = RubyType::union(vec![ty, RubyType::nil_class()]);
                    self.vars.insert(var, union);
                }
            }
        }

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
            self.vars = env_before.clone();
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
        let expression_type = self.track_node(&expression);
        let expression_env = self.vars.clone();

        let rescue_expression = rescue_modifier.rescue_expression();
        self.vars = env_before;
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

        // Iterate loop body a limited number of times
        let mut last_type = RubyType::nil_class();
        for _iteration in 0..self.max_loop_iterations {
            if let Some(statements) = while_node.statements() {
                last_type = self.track_node(&statements.as_node());
            }
        }

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

        // Iterate loop body a limited number of times
        let mut last_type = RubyType::nil_class();
        for _iteration in 0..self.max_loop_iterations {
            if let Some(statements) = until_node.statements() {
                last_type = self.track_node(&statements.as_node());
            }
        }

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
            if let Some(ty) = self.vars.get(&var_name) {
                return ty.clone();
            }
            return RubyType::Unknown;
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
                return self.infer_expression(&body);
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

        if let Some(block_return_type) = self.infer_yielding_block_return_type(call, &method_name) {
            return block_return_type;
        }
        if let Some(proc_return_type) = self.infer_proc_call_return_type(call, &method_name) {
            return proc_return_type;
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
        let return_type = body
            .map(|body| self.track_node(&body))
            .unwrap_or_else(RubyType::nil_class);
        self.vars = previous_vars;
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
        if allow_private {
            if let Some(return_type) =
                self.local_method_return_type_for_receiver(receiver_type, &method)
            {
                return Some(return_type);
            }
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
            query.method_return_type_for_receiver(&namespace, &method)
        } else if let Some(current_class) = self.current_class.as_ref() {
            let caller_namespace = FullyQualifiedName::namespace_with_kind(
                current_class.namespace_parts(),
                crate::core::NamespaceKind::Instance,
            );
            query.method_return_type_for_protected_receiver(&namespace, &method, &caller_namespace)
        } else {
            query.method_return_type_for_public_receiver(&namespace, &method)
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
        self.local_method_returns
            .get(&FullyQualifiedName::method(parts, method.clone()))
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
            .method_return_type_for_receiver(&callee.owner, method)
            .unwrap_or(RubyType::Unknown)
    }

    /// Infer the type of a return statement
    fn infer_return(&mut self, ret: &ReturnNode) -> RubyType {
        if let Some(args) = ret.arguments() {
            let args_list: Vec<_> = args.arguments().iter().collect();
            if args_list.is_empty() {
                return RubyType::nil_class();
            } else if args_list.len() == 1 {
                return self.infer_expression(&args_list[0]);
            } else {
                // Multiple return values become an array
                let types: Vec<RubyType> =
                    args_list.iter().map(|a| self.infer_expression(a)).collect();
                return RubyType::Array(types);
            }
        }
        RubyType::nil_class()
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
}
