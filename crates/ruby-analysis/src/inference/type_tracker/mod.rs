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
    ConstantTypeDependency, FullyQualifiedName, LiteralKey, LiteralValue, MethodReturnEquation,
    NamespaceKind, RubyConstant, RubyMethod, ShapeConstructionError, ShapeExactness, ShapeField,
    ShapeStability, ShapeType, TypeInferenceOutcome, UnknownReason, MAX_SHAPE_ALIASES,
};
use crate::engine::{AnalysisEngine, AnalysisQuery, AnalysisQueryCache};
use crate::inference::method::recursive::MAX_RECURSIVE_RETURN_ITERATIONS;
use crate::r#type::literal::{
    infer_array_literal_type_fallible, infer_hash_literal_type_fallible, literal_key,
    literal_shape_construction_unknown_reason, project_immediate_hash_receiver_type,
    LiteralAnalyzer,
};
use crate::r#type::ruby::RubyType;
use crate::r#type::shape as shape_reads;
use parking_lot::RwLock;
use ruby_prism::*;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::ops::{Deref, DerefMut};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShapePredicateMatch {
    Matches,
    DoesNotMatch,
    Inconclusive,
}

#[derive(Debug, Default)]
struct RescueEntryTypes {
    locals: HashMap<String, RubyType>,
}

/// One flow-local abstract identity for a mutable Hash value.
///
/// The identity never leaves one TypeTracker pass. Engine facts retain only
/// the resulting canonical RubyType or an explicit UnknownReason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct ShapeIdentity(u32);

/// One proven containment edge between two flow-local Hash identities.
///
/// The edge is deliberately inference-private. A parent Shape stores the
/// canonical projected child type, while this edge retains the Ruby object
/// identity needed to update that projection after a known child mutation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ShapeContainment {
    parent: ShapeIdentity,
    key: LiteralKey,
    child: ShapeIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ShapeAlternativeTransition {
    before: RubyType,
    after: RubyType,
}

/// Bounded positional identity evidence for one exact local Array value.
///
/// Only positions containing tracked Hash identities are retained. `length`
/// makes negative literal indices deterministic without allocating storage for
/// scalar-only positions, while `contained` supports whole-Array escape
/// invalidation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct ArrayShapeAliases {
    length: usize,
    positions: BTreeMap<usize, BTreeSet<ShapeIdentity>>,
    contained: BTreeSet<ShapeIdentity>,
    unknown_reason: Option<UnknownReason>,
}

impl ArrayShapeAliases {
    fn identities_at(&self, index: i32) -> BTreeSet<ShapeIdentity> {
        let position = if index >= 0 {
            usize::try_from(index).expect(
                "INVARIANT VIOLATED: a nonnegative i32 Array index did not fit usize. This is a bug because every supported Rust target can represent u32-sized collection positions. Fix: keep positional indices bounded to Prism's i32 conversion.",
            )
        } else {
            let from_end = usize::try_from(index.unsigned_abs()).expect(
                "INVARIANT VIOLATED: an i32 Array index magnitude did not fit usize. This is a bug because every supported Rust target can represent u32-sized collection positions. Fix: keep positional indices bounded to Prism's i32 conversion.",
            );
            let Some(position) = self.length.checked_sub(from_end) else {
                return BTreeSet::new();
            };
            position
        };
        if position >= self.length {
            return BTreeSet::new();
        }
        self.positions.get(&position).cloned().unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ShapeIdentityState {
    Proven(RubyType),
    Invalidated(UnknownReason),
}

#[derive(Debug, Clone, Default)]
struct FlowEnvironment {
    types: HashMap<String, RubyType>,
    constant_dependencies: HashMap<String, BTreeSet<ConstantTypeDependency>>,
    shape_bindings: HashMap<String, BTreeSet<ShapeIdentity>>,
    shape_states: HashMap<ShapeIdentity, ShapeIdentityState>,
    shape_containments: BTreeSet<ShapeContainment>,
    array_shape_aliases: HashMap<String, ArrayShapeAliases>,
    unknown_reasons: HashMap<String, UnknownReason>,
    callables: HashMap<String, crate::inference::higher_order::KnownProcType>,
    max_live_shape_aliases: usize,
}

impl FlowEnvironment {
    fn insert(&mut self, name: String, ruby_type: RubyType) -> Option<RubyType> {
        self.constant_dependencies.remove(&name);
        self.shape_bindings.remove(&name);
        self.array_shape_aliases.remove(&name);
        self.unknown_reasons.remove(&name);
        self.types.insert(name, ruby_type)
    }

    fn insert_unknown(&mut self, name: String, reason: UnknownReason) -> Option<RubyType> {
        self.constant_dependencies.remove(&name);
        self.shape_bindings.remove(&name);
        self.array_shape_aliases.remove(&name);
        self.unknown_reasons.insert(name.clone(), reason);
        self.types.insert(name, RubyType::Unknown)
    }

    fn bind_shape_identities(
        &mut self,
        name: String,
        ruby_type: RubyType,
        identities: BTreeSet<ShapeIdentity>,
    ) {
        assert!(
            !identities.is_empty(),
            "INVARIANT VIOLATED: a shape binding was installed without an abstract identity. This is a bug because aliases can only synchronize through a concrete flow-local identity. Fix: allocate or copy at least one identity before binding a shape local."
        );
        self.constant_dependencies.remove(&name);
        self.unknown_reasons.remove(&name);
        self.array_shape_aliases.remove(&name);
        self.types.insert(name.clone(), ruby_type);
        self.shape_bindings.insert(name, identities);
        self.enforce_alias_bound();
    }

    fn shape_identities(&self, name: &str) -> BTreeSet<ShapeIdentity> {
        self.shape_bindings.get(name).cloned().unwrap_or_default()
    }

    fn unknown_reason(&self, name: &str) -> Option<UnknownReason> {
        self.unknown_reasons.get(name).copied()
    }

    fn invalidate_identities(
        &mut self,
        identities: &BTreeSet<ShapeIdentity>,
        reason: UnknownReason,
    ) {
        let mut affected = identities.clone();
        loop {
            let before = affected.len();
            for link in &self.shape_containments {
                if affected.contains(&link.parent) || affected.contains(&link.child) {
                    affected.insert(link.parent);
                    affected.insert(link.child);
                }
            }
            if affected.len() == before {
                break;
            }
        }
        for identity in &affected {
            assert!(
                self.shape_states.contains_key(identity),
                "INVARIANT VIOLATED: shape invalidation targeted an unknown abstract identity {:?}. This is a bug because a local cannot reference an identity absent from its environment. Fix: merge identity bindings and states atomically.",
                identity
            );
            self.shape_states
                .insert(*identity, ShapeIdentityState::Invalidated(reason));
        }
        self.synchronize_shape_aliases();
    }

    fn clear(&mut self) {
        self.types.clear();
        self.constant_dependencies.clear();
        self.shape_bindings.clear();
        self.shape_states.clear();
        self.shape_containments.clear();
        self.array_shape_aliases.clear();
        self.unknown_reasons.clear();
        self.callables.clear();
        self.max_live_shape_aliases = 0;
    }

    fn bind_array_shape_aliases(&mut self, name: String, aliases: ArrayShapeAliases) {
        if aliases.contained.is_empty() && aliases.unknown_reason.is_none() {
            self.array_shape_aliases.remove(&name);
        } else {
            self.array_shape_aliases.insert(name, aliases);
        }
        self.synchronize_shape_aliases();
    }

    fn contained_child(&self, parent: ShapeIdentity, key: &LiteralKey) -> Option<ShapeIdentity> {
        let mut matches = self
            .shape_containments
            .iter()
            .filter(|link| link.parent == parent && &link.key == key)
            .map(|link| link.child);
        let child = matches.next()?;
        assert!(
            matches.next().is_none(),
            "INVARIANT VIOLATED: one parent Hash field points at multiple abstract child identities. This is a bug because a precise required field contains one Ruby object on one flow path. Fix: invalidate ambiguous branch containment before installing the merged environment."
        );
        Some(child)
    }

    fn link_contained_shape(
        &mut self,
        parent: ShapeIdentity,
        key: LiteralKey,
        child: ShapeIdentity,
    ) {
        assert!(
            parent != child,
            "INVARIANT VIOLATED: a Hash identity was linked as its own statically proven child. This is a bug because bounded literal construction cannot create a recursive Ruby Hash. Fix: treat runtime-created cycles as an unsupported mutation boundary."
        );
        assert!(
            self.shape_states.contains_key(&parent) && self.shape_states.contains_key(&child),
            "INVARIANT VIOLATED: a containment edge references an absent Hash identity. This is a bug because parent and child states must exist before their relationship is installed. Fix: allocate both identities before calling link_contained_shape."
        );
        if let Some(existing) = self.contained_child(parent, &key) {
            assert_eq!(
                existing, child,
                "INVARIANT VIOLATED: one parent Hash field was rebound without detaching its prior child identity. This is a bug because Ruby assignment replaces the contained object. Fix: detach the exact field before installing its replacement containment edge."
            );
            return;
        }
        self.shape_containments
            .insert(ShapeContainment { parent, key, child });
    }

    fn detach_contained_shapes(
        &mut self,
        parents: &BTreeSet<ShapeIdentity>,
        key: Option<&LiteralKey>,
    ) {
        self.shape_containments.retain(|link| {
            !parents.contains(&link.parent) || key.is_some_and(|key| key != &link.key)
        });
    }

    fn synchronize_shape_aliases(&mut self) {
        let names = self.shape_bindings.keys().cloned().collect::<Vec<_>>();
        for name in names {
            let identities = self.shape_identities(&name);
            let current_type = self.types.get(&name).cloned().unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: shape alias `{name}` has no local type. This is a bug because bindings and type entries must be installed atomically. Fix: use FlowEnvironment::bind_shape_identities for shape locals."
                )
            });
            match self.type_for_shape_identities(&current_type, &identities) {
                Ok(ruby_type) => {
                    self.types.insert(name.clone(), ruby_type);
                    self.unknown_reasons.remove(&name);
                }
                Err(reason) => {
                    self.types.insert(name.clone(), RubyType::Unknown);
                    self.unknown_reasons.insert(name, reason);
                }
            }
        }

        let array_names = self.array_shape_aliases.keys().cloned().collect::<Vec<_>>();
        for name in array_names {
            let aliases = self
                .array_shape_aliases
                .get(&name)
                .cloned()
                .unwrap_or_else(|| {
                    panic!(
                        "INVARIANT VIOLATED: Array alias `{name}` disappeared during synchronization. This is a bug because the alias-name snapshot and map are not mutated by type projection. Fix: keep Array alias removal outside synchronize_shape_aliases."
                    )
                });
            let current_type = self.types.get(&name).cloned().unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: Array alias `{name}` has no local type. This is a bug because positional evidence and the Array type must be installed atomically. Fix: bind Array aliases only after inserting the assignment type."
                )
            });
            match self.type_for_array_shape_aliases(&current_type, &aliases) {
                Ok(ruby_type) => {
                    self.types.insert(name.clone(), ruby_type);
                    self.unknown_reasons.remove(&name);
                }
                Err(reason) => {
                    self.types
                        .insert(name.clone(), RubyType::Array(vec![RubyType::Unknown]));
                    self.unknown_reasons.insert(name, reason);
                }
            }
        }
    }

    fn type_for_array_shape_aliases(
        &self,
        current_type: &RubyType,
        aliases: &ArrayShapeAliases,
    ) -> Result<RubyType, UnknownReason> {
        if let Some(reason) = aliases.unknown_reason {
            return Err(reason);
        }
        let mut elements = array_element_alternatives(current_type)
            .iter()
            .flat_map(non_shape_alternatives)
            .collect::<Vec<_>>();
        for identity in &aliases.contained {
            match self.shape_states.get(identity).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: Array alias references absent contained identity {:?}. This is a bug because Array summaries and shape states must merge atomically. Fix: preserve every contained identity state while retaining positional evidence.",
                    identity
                )
            }) {
                ShapeIdentityState::Proven(ruby_type) => {
                    elements.extend(shape_alternatives(ruby_type)?);
                }
                ShapeIdentityState::Invalidated(reason) => return Err(*reason),
            }
        }
        assert!(
            !elements.is_empty(),
            "INVARIANT VIOLATED: a proven Array shape summary produced no element alternatives. This is a bug because a retained summary must contain at least one shape identity or an explicit unknown reason. Fix: remove empty Array summaries in bind_array_shape_aliases."
        );
        Ok(RubyType::Array(RubyType::canonical_union_members(elements)))
    }

    fn type_for_shape_identities(
        &self,
        current_type: &RubyType,
        identities: &BTreeSet<ShapeIdentity>,
    ) -> Result<RubyType, UnknownReason> {
        let mut alternatives = non_shape_alternatives(current_type);
        for identity in identities {
            match self.shape_states.get(identity).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: shape binding references absent identity {:?}. This is a bug because branch joins must merge identity states before synchronizing aliases. Fix: keep shape_bindings and shape_states in one FlowEnvironment.",
                    identity
                )
            }) {
                ShapeIdentityState::Proven(ruby_type) => {
                    alternatives.extend(shape_alternatives(ruby_type)?);
                }
                ShapeIdentityState::Invalidated(reason) => return Err(*reason),
            }
        }
        let joined = RubyType::union(alternatives);
        if joined == RubyType::Unknown {
            return Err(UnknownReason::ShapeBoundExceeded);
        }
        Ok(joined)
    }

    fn enforce_alias_bound(&mut self) {
        let identities = self.shape_states.keys().copied().collect::<Vec<_>>();
        let mut exceeded = BTreeSet::new();
        for identity in identities {
            let alias_count = self
                .shape_bindings
                .values()
                .filter(|binding| binding.contains(&identity))
                .count()
                + self
                    .shape_containments
                    .iter()
                    .filter(|link| link.child == identity)
                    .count();
            self.max_live_shape_aliases = self.max_live_shape_aliases.max(alias_count);
            if alias_count > MAX_SHAPE_ALIASES {
                exceeded.insert(identity);
            }
        }
        if !exceeded.is_empty() {
            self.invalidate_identities(&exceeded, UnknownReason::ShapeBoundExceeded);
        }
    }

    fn set_constant_dependencies(
        &mut self,
        name: String,
        dependencies: BTreeSet<ConstantTypeDependency>,
    ) {
        if dependencies.is_empty() {
            self.constant_dependencies.remove(&name);
        } else {
            self.constant_dependencies.insert(name, dependencies);
        }
    }

    fn dependencies(&self, name: &str) -> BTreeSet<ConstantTypeDependency> {
        self.constant_dependencies
            .get(name)
            .cloned()
            .unwrap_or_default()
    }
}

fn array_element_alternatives(ruby_type: &RubyType) -> Vec<RubyType> {
    match ruby_type {
        RubyType::Array(elements) => elements.clone(),
        RubyType::Union(members) => members
            .iter()
            .flat_map(array_element_alternatives)
            .collect(),
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Literal(_)
        | RubyType::Hash(_, _)
        | RubyType::Shape(_)
        | RubyType::Unknown => panic!(
            "INVARIANT VIOLATED: positional shape aliases are attached to non-Array type `{ruby_type}`. This is a bug because every reachable type for an Array identity must remain an Array. Fix: clear Array aliases whenever any branch rebinds the local to a non-Array value."
        ),
    }
}

impl Deref for FlowEnvironment {
    type Target = HashMap<String, RubyType>;

    fn deref(&self) -> &Self::Target {
        &self.types
    }
}

impl DerefMut for FlowEnvironment {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.types
    }
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

    fn environment_from(&self, environment_before: &FlowEnvironment) -> FlowEnvironment {
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
    pub unknown_reason: Option<UnknownReason>,
    pub constant_dependencies: BTreeSet<ConstantTypeDependency>,
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
    vars: FlowEnvironment,

    /// Monotonic allocator for flow-local abstract Hash identities. Keeping it
    /// outside cloned branch environments prevents identities created in
    /// separate branches from colliding when those branches join.
    next_shape_identity: u32,

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

    /// Value-constant terms returned by the current method. These stay
    /// private until attached to the file-owned method equation.
    observed_return_constant_dependencies: BTreeSet<ConstantTypeDependency>,

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
    fn invalidate_escaped_callables_in_value(&mut self, value: &Node<'_>) {
        if self.vars.callables.is_empty() {
            return;
        }
        if crate::indexer::is_static_callable_literal(value)
            || value.as_local_variable_read_node().is_some()
        {
            return;
        }
        let mut escaped = EscapedCallableReadCollector::default();
        escaped.visit(value);
        self.invalidate_callable_names(escaped.names);
    }

    fn invalidate_callable_names(&mut self, names: HashSet<String>) {
        for name in names {
            let Some(identity) = self
                .vars
                .callables
                .get(&name)
                .map(|callable| callable.identity)
            else {
                continue;
            };
            for callable in self.vars.callables.values_mut() {
                if callable.identity == identity {
                    callable.summary = Err(UnknownReason::EscapedCallableValue);
                }
            }
        }
    }

    fn invalidate_escaped_callables_in_call(&mut self, call: &CallNode<'_>) {
        if self.vars.callables.is_empty() {
            return;
        }
        let mut escaped = EscapedCallableReadCollector::default();
        if let Some(receiver) = call.receiver() {
            let direct_invoke = call.name().as_slice() == b"call"
                && receiver.as_local_variable_read_node().is_some();
            if !direct_invoke {
                escaped.visit(&receiver);
            }
        }
        if let Some(arguments) = call.arguments() {
            escaped.visit_arguments_node(&arguments);
        }
        if call
            .block()
            .is_some_and(|block| block.as_block_node().is_some())
        {
            escaped.visit(call.block().as_ref().expect("checked block presence"));
        }
        self.invalidate_callable_names(escaped.names);
    }

    /// Create a new type tracker for the given source.
    pub fn new(source: &'a [u8]) -> Self {
        Self {
            vars: FlowEnvironment::default(),
            next_shape_identity: 0,
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
            local_return_terms: HashMap::new(),
            inside_control_flow: false,
            recursive_return_approximation: None,
            local_method_candidates: Arc::new(HashSet::new()),
            observed_return_dependencies: BTreeSet::new(),
            observed_return_constant_dependencies: BTreeSet::new(),
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

    pub(crate) fn max_live_shape_aliases(&self) -> usize {
        self.vars.max_live_shape_aliases
    }

    fn bind_local_callable(
        &mut self,
        name: String,
        mut callable: crate::inference::higher_order::KnownProcType,
    ) {
        if callable
            .summary
            .as_ref()
            .is_ok_and(|summary| summary.captures.binary_search(&name).is_ok())
        {
            callable.summary = Err(UnknownReason::CallableRecursionUnsupported);
        }
        let alias_count = self
            .vars
            .callables
            .iter()
            .filter(|(existing_name, existing)| {
                existing_name.as_str() != name && existing.identity == callable.identity
            })
            .count();
        if alias_count >= crate::core::callable_body::MAX_CALLABLE_BODY_ALIASES {
            callable.summary = Err(UnknownReason::CallableBodyBoundExceeded);
            for existing in self.vars.callables.values_mut() {
                if existing.identity == callable.identity {
                    existing.summary = Err(UnknownReason::CallableBodyBoundExceeded);
                }
            }
        }
        self.vars.callables.insert(name, callable);
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

    /// Infer one block body from an explicit environment and return the
    /// post-body value of each tracked parameter only when the same bounded
    /// mutable identity remains proven. This is the shared bridge used by the
    /// indexer and ordinary flow tracker; local rebinding is not mistaken for
    /// mutation of the yielded object.
    pub(crate) fn track_isolated_block_body(
        &mut self,
        body: Option<Node<'_>>,
        bindings: &HashMap<String, RubyType>,
        tracked_parameters: &[(String, RubyType)],
    ) -> (RubyType, Vec<RubyType>) {
        self.vars.clear();
        self.next_shape_identity = 0;
        for (name, ruby_type) in bindings {
            if type_is_shape_only(ruby_type) {
                let identity = self.allocate_shape_identity(ruby_type.clone());
                self.vars.bind_shape_identities(
                    name.clone(),
                    ruby_type.clone(),
                    BTreeSet::from([identity]),
                );
            } else {
                self.vars.insert(name.clone(), ruby_type.clone());
            }
        }
        let result = body
            .map(|body| self.track_node(&body))
            .unwrap_or_else(RubyType::nil_class);
        let post_parameters = tracked_parameters
            .iter()
            .map(|(name, original)| {
                let identities = self.vars.shape_identities(name);
                if identities.is_empty() {
                    original.clone()
                } else {
                    self.shape_identity_type(&identities)
                        .unwrap_or(RubyType::Unknown)
                }
            })
            .collect();
        (result, post_parameters)
    }

    /// Record current variable state at an offset
    fn record_state(&mut self, offset: usize) {
        // Only record if there are variables to track
        if !self.vars.is_empty() {
            self.var_types.insert(offset, self.vars.types.clone());
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

        let constant_dependencies = std::mem::take(&mut self.observed_return_constant_dependencies);
        MethodReturnEquation::new(method_fqn, base, dependencies)
            .with_constant_dependencies(constant_dependencies)
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
        self.next_shape_identity = 0;
        self.var_types.clear();
        self.local_read_types.clear();
        self.has_seen_control_flow = false;
        self.local_return_terms.clear();
        self.explicit_return_types.clear();
        self.observed_return_dependencies.clear();
        self.observed_return_constant_dependencies.clear();
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
        let fallthrough_constant_dependencies = method
            .body()
            .map(|body| self.constant_dependencies_for_node(&body))
            .unwrap_or_default();
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
            Some(_) if !fallthrough_constant_dependencies.is_empty() => {
                self.observed_return_constant_dependencies
                    .extend(fallthrough_constant_dependencies);
                if fallthrough_type == RubyType::Unknown {
                    RecursiveReturnApproximation::Bottom
                } else {
                    RecursiveReturnApproximation::from_ruby_type(fallthrough_type)
                }
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

            // Storing a tracked mutable shape outside the local flow graph is
            // an escape. The assignment expression itself cannot retain a
            // concrete shape after that boundary because arbitrary later code
            // may mutate the stored object.
            _ if node.as_instance_variable_write_node().is_some() => {
                let write = node.as_instance_variable_write_node().unwrap();
                self.track_escaping_write(&write.value())
            }
            _ if node.as_class_variable_write_node().is_some() => {
                let write = node.as_class_variable_write_node().unwrap();
                self.track_escaping_write(&write.value())
            }
            _ if node.as_global_variable_write_node().is_some() => {
                let write = node.as_global_variable_write_node().unwrap();
                self.track_escaping_write(&write.value())
            }
            _ if node.as_constant_write_node().is_some() => {
                let write = node.as_constant_write_node().unwrap();
                self.track_escaping_write(&write.value())
            }
            _ if node.as_constant_path_write_node().is_some() => {
                let write = node.as_constant_path_write_node().unwrap();
                self.track_escaping_write(&write.value())
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
        self.invalidate_escaped_callables_in_value(&write.value());

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

        // Capture alias provenance before evaluating the RHS. Evaluation may
        // freeze, mutate, or invalidate the referenced identity, but an
        // assignment such as `copy = payload` or `copy = payload.freeze`
        // still binds the exact same object after that effect.
        let value = write.value();
        let aliased_shape_identities = self.shape_identities_for_alias_expression(&value);
        let is_direct_recursive_value = value
            .as_call_node()
            .is_some_and(|call| call_is_direct_recursive(&call, self.current_method.as_ref()));
        let (var_type, assignment_unknown_reason) =
            if let Some(result) = self.infer_collection_literal_type(&value) {
                match result {
                    Ok(ruby_type) => (ruby_type, None),
                    Err(error) => (
                        RubyType::Unknown,
                        Some(literal_shape_construction_unknown_reason(error)),
                    ),
                }
            } else {
                (self.track_node(&value), None)
            };
        let array_shape_aliases = self.array_shape_aliases_for_assignment(&value);
        let constant_dependencies = self.constant_dependencies_for_node(&value);
        let dependency = value
            .as_call_node()
            .and_then(|call| self.return_term_dependency_for_call(&call));
        if let Some(return_type) = self.infer_proc_literal_return_type(&value) {
            self.bind_local_callable(var_name.clone(), return_type);
        } else if let Some(alias) = value.as_local_variable_read_node() {
            let alias_name = String::from_utf8_lossy(alias.name().as_slice()).to_string();
            if let Some(callable) = self.vars.callables.get(&alias_name).cloned() {
                self.bind_local_callable(var_name.clone(), callable);
            } else {
                self.vars.callables.remove(&var_name);
            }
        } else {
            self.vars.callables.remove(&var_name);
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

        // Update environment. A direct local/freeze alias shares identity;
        // a precise keyed read shares the nested object's contained identity;
        // every other complete shape-producing expression allocates a fresh
        // abstract Hash identity.
        if !aliased_shape_identities.is_empty() {
            self.vars.bind_shape_identities(
                var_name.clone(),
                var_type.clone(),
                aliased_shape_identities,
            );
            self.vars.synchronize_shape_aliases();
        } else if type_is_shape_only(&var_type) {
            let contained_identities =
                self.shape_identities_for_contained_alias_expression(&value, &var_type);
            let identities = if contained_identities.is_empty() {
                let identity = self.allocate_shape_identity(var_type.clone());
                self.link_shape_literal_children(&value, identity);
                if matches!(var_type, RubyType::Shape(_))
                    && self.materialize_direct_shape_children(identity).is_err()
                {
                    self.vars.invalidate_identities(
                        &BTreeSet::from([identity]),
                        UnknownReason::MutableShapeInvalidated,
                    );
                }
                BTreeSet::from([identity])
            } else {
                contained_identities
            };
            self.vars
                .bind_shape_identities(var_name.clone(), var_type.clone(), identities);
            self.vars.synchronize_shape_aliases();
        } else if let Some(reason) = assignment_unknown_reason {
            self.vars.insert_unknown(var_name.clone(), reason);
        } else {
            self.vars.insert(var_name.clone(), var_type.clone());
        }
        if let Some(array_shape_aliases) = array_shape_aliases {
            assert!(
                matches!(var_type, RubyType::Array(_)),
                "INVARIANT VIOLATED: positional Array shape aliases were attached to non-Array type `{var_type}`. This is a bug because only exact Array literals or aliases produce this evidence. Fix: keep array_shape_aliases_for_assignment aligned with collection inference."
            );
            self.vars
                .bind_array_shape_aliases(var_name.clone(), array_shape_aliases);
        }
        self.vars
            .set_constant_dependencies(var_name.clone(), constant_dependencies);
        if !self.rescue_entry_types.is_empty() {
            self.observe_rescue_entry_type(&var_name, &var_type);
        }

        // Return the assigned type (assignments return their value in Ruby).
        // Synchronization may have replaced it with an explained Unknown when
        // the alias limit or an earlier escape invalidated the identity.
        self.vars.get(&var_name).cloned().unwrap_or(var_type)
    }

    fn allocate_shape_identity(&mut self, ruby_type: RubyType) -> ShapeIdentity {
        assert!(
            type_is_shape_only(&ruby_type),
            "INVARIANT VIOLATED: attempted to allocate a Hash identity for non-shape type `{ruby_type}`. This is a bug because only complete shape-producing expressions participate in alias tracking. Fix: guard allocation with type_is_shape_only."
        );
        let identity = ShapeIdentity(self.next_shape_identity);
        self.next_shape_identity = self.next_shape_identity.checked_add(1).expect(
            "INVARIANT VIOLATED: one flow traversal allocated more than u32::MAX abstract Hash identities. This is a bug because source size and fixed shape bounds make that impossible in a valid analysis pass. Fix: investigate repeated allocation or widen ShapeIdentity.",
        );
        let previous = self
            .vars
            .shape_states
            .insert(identity, ShapeIdentityState::Proven(ruby_type));
        assert!(
            previous.is_none(),
            "INVARIANT VIOLATED: flow-local Hash identity {:?} was allocated twice. This is a bug because the allocator must be monotonic across cloned branch environments. Fix: keep next_shape_identity on TypeTracker rather than FlowEnvironment.",
            identity
        );
        identity
    }

    fn shape_identities_for_alias_expression(&self, node: &Node<'_>) -> BTreeSet<ShapeIdentity> {
        if let Some(read) = node.as_local_variable_read_node() {
            let name = String::from_utf8_lossy(read.name().as_slice());
            return self.vars.shape_identities(name.as_ref());
        }
        if let Some(parentheses) = node.as_parentheses_node() {
            return parentheses
                .body()
                .map(|body| self.shape_identities_for_alias_expression(&body))
                .unwrap_or_default();
        }
        if let Some(call) = node.as_call_node() {
            let method_name = call.name().as_slice();
            let identity_returning_hash_call =
                method_name == b"freeze" || (method_name == b"to_h" && call.block().is_none());
            if identity_returning_hash_call && call.arguments().is_none() {
                return call
                    .receiver()
                    .map(|receiver| self.shape_identities_for_alias_expression(&receiver))
                    .unwrap_or_default();
            }
            if let Some(receiver) = call.receiver() {
                if let Some(local) = receiver.as_local_variable_read_node() {
                    let name = String::from_utf8_lossy(local.name().as_slice());
                    if let Some(aliases) = self.vars.array_shape_aliases.get(name.as_ref()) {
                        if call.arguments().is_none() {
                            return match method_name {
                                b"first" => aliases.identities_at(0),
                                b"last" => aliases.identities_at(-1),
                                _ => BTreeSet::new(),
                            };
                        }
                        if matches!(method_name, b"[]" | b"at" | b"fetch") {
                            let arguments = call.arguments().expect(
                                "INVARIANT VIOLATED: checked Array read arguments disappeared before use. This is a bug because Prism nodes are immutable. Fix: destructure call.arguments once.",
                            );
                            let mut arguments = arguments.arguments().iter();
                            let Some(argument) = arguments.next() else {
                                return BTreeSet::new();
                            };
                            if arguments.next().is_none() {
                                if let Some(index) = literal_array_index(&argument) {
                                    return aliases.identities_at(index);
                                }
                            }
                        }
                    }
                }
            }
        }
        BTreeSet::new()
    }

    fn array_shape_aliases_for_assignment(&mut self, node: &Node<'_>) -> Option<ArrayShapeAliases> {
        if let Some(local) = node.as_local_variable_read_node() {
            let name = String::from_utf8_lossy(local.name().as_slice());
            return self.vars.array_shape_aliases.get(name.as_ref()).cloned();
        }
        if let Some(parentheses) = node.as_parentheses_node() {
            return parentheses
                .body()
                .and_then(|body| self.array_shape_aliases_for_assignment(&body));
        }
        if let Some(call) = node.as_call_node() {
            if call.arguments().is_none()
                && call.block().is_none()
                && matches!(call.name().as_slice(), b"to_a" | b"freeze")
            {
                let receiver = call.receiver()?;
                let local = receiver.as_local_variable_read_node()?;
                let name = String::from_utf8_lossy(local.name().as_slice());
                return self.vars.array_shape_aliases.get(name.as_ref()).cloned();
            }
        }
        let array = node.as_array_node()?;
        let elements = array.elements().iter().collect::<Vec<_>>();
        let mut aliases = ArrayShapeAliases {
            length: elements.len(),
            ..ArrayShapeAliases::default()
        };
        for (index, element) in elements.iter().enumerate() {
            let mut identities = self.shape_identities_for_alias_expression(element);
            if identities.is_empty() {
                if let Some(local) = element.as_local_variable_read_node() {
                    let name = String::from_utf8_lossy(local.name().as_slice());
                    if let Some(nested) = self.vars.array_shape_aliases.get(name.as_ref()) {
                        identities.extend(&nested.contained);
                    }
                } else if element.as_hash_node().is_some() {
                    if let Some(Ok(ruby_type)) = self.infer_collection_literal_type(element) {
                        if type_is_shape_only(&ruby_type) {
                            let identity = self.allocate_shape_identity(ruby_type);
                            self.link_shape_literal_children(element, identity);
                            if let Err(reason) = self.materialize_direct_shape_children(identity) {
                                self.vars
                                    .invalidate_identities(&BTreeSet::from([identity]), reason);
                                aliases.positions.clear();
                                aliases.contained.clear();
                                aliases.unknown_reason = Some(reason);
                                return Some(aliases);
                            }
                            identities.insert(identity);
                        }
                    }
                }
            }
            if !identities.is_empty() {
                aliases.positions.insert(index, identities.clone());
            }
            aliases.contained.extend(identities);
            if aliases.positions.len() > MAX_SHAPE_ALIASES
                || aliases.contained.len() > MAX_SHAPE_ALIASES
            {
                self.vars
                    .invalidate_identities(&aliases.contained, UnknownReason::ShapeBoundExceeded);
                aliases.positions.clear();
                aliases.contained.clear();
                aliases.unknown_reason = Some(UnknownReason::ShapeBoundExceeded);
                return Some(aliases);
            }
        }
        Some(aliases)
    }

    fn shape_identities_for_contained_alias_expression(
        &mut self,
        node: &Node<'_>,
        inferred_type: &RubyType,
    ) -> BTreeSet<ShapeIdentity> {
        let Some((parent_name, path)) = shape_local_key_path(node) else {
            return BTreeSet::new();
        };
        let root_parents = self.vars.shape_identities(&parent_name);
        if root_parents.is_empty() {
            return BTreeSet::new();
        }

        let mut current_parents = root_parents.clone();
        for key in path {
            let mut children = BTreeSet::new();
            let mut pending = Vec::new();
            for parent in &current_parents {
                if let Some(child) = self.vars.contained_child(*parent, &key) {
                    children.insert(child);
                    continue;
                }
                let child_type = match self.vars.shape_states.get(parent).unwrap_or_else(|| {
                    panic!(
                        "INVARIANT VIOLATED: nested keyed read references absent parent identity {:?}. This is a bug because a local shape binding and its state must be installed atomically. Fix: preserve both through assignment and branch joins.",
                        parent
                    )
                }) {
                    ShapeIdentityState::Proven(parent_type) => {
                        precise_contained_shape_field(parent_type, &key)
                    }
                    ShapeIdentityState::Invalidated(_) => None,
                };
                let Some(child_type) = child_type else {
                    self.vars.invalidate_identities(
                        &root_parents,
                        UnknownReason::MutableShapeInvalidated,
                    );
                    return BTreeSet::new();
                };
                pending.push((*parent, child_type));
            }

            for (parent, child_type) in pending {
                let child = self.allocate_shape_identity(child_type);
                self.vars.link_contained_shape(parent, key.clone(), child);
                children.insert(child);
            }
            current_parents = children;
        }

        if current_parents.is_empty() {
            return current_parents;
        }
        let projected = self
            .shape_identity_type(&current_parents)
            .unwrap_or(RubyType::Unknown);
        if projected != *inferred_type {
            self.vars
                .invalidate_identities(&root_parents, UnknownReason::MutableShapeInvalidated);
            return BTreeSet::new();
        }
        current_parents
    }

    fn link_shape_literal_children(&mut self, node: &Node<'_>, parent: ShapeIdentity) {
        if self.link_non_mutating_merge_children(node, parent) {
            return;
        }
        let hash = if let Some(hash) = node.as_hash_node() {
            Some(hash)
        } else if let Some(parentheses) = node.as_parentheses_node() {
            parentheses.body().and_then(|body| body.as_hash_node())
        } else {
            None
        };
        let Some(hash) = hash else {
            return;
        };

        let mut links = BTreeMap::<LiteralKey, ShapeIdentity>::new();
        for element in hash.elements().iter() {
            if let Some(splat) = element.as_assoc_splat_node() {
                let Some(value) = splat.value() else {
                    self.vars.invalidate_identities(
                        &BTreeSet::from([parent]),
                        UnknownReason::MutableShapeInvalidated,
                    );
                    return;
                };
                let source_parents = self.shape_identities_for_alias_expression(&value);
                for source_parent in &source_parents {
                    if self
                        .materialize_direct_shape_children(*source_parent)
                        .is_err()
                    {
                        self.vars.invalidate_identities(
                            &BTreeSet::from([parent]),
                            UnknownReason::MutableShapeInvalidated,
                        );
                        return;
                    }
                }
                for link in self.vars.shape_containments.clone() {
                    if !source_parents.contains(&link.parent) {
                        continue;
                    }
                    if links
                        .insert(link.key.clone(), link.child)
                        .is_some_and(|previous| previous != link.child)
                    {
                        self.vars.invalidate_identities(
                            &BTreeSet::from([parent]),
                            UnknownReason::MutableShapeInvalidated,
                        );
                        return;
                    }
                }
                continue;
            }
            let Some(assoc) = element.as_assoc_node() else {
                continue;
            };
            let Some(key) = literal_key(&assoc.key()) else {
                continue;
            };
            links.remove(&key);
            let children = self.shape_identities_for_alias_expression(&assoc.value());
            if children.is_empty() {
                continue;
            }
            let Some(child) = children
                .iter()
                .copied()
                .next()
                .filter(|_| children.len() == 1)
            else {
                self.vars.invalidate_identities(
                    &BTreeSet::from([parent]),
                    UnknownReason::MutableShapeInvalidated,
                );
                return;
            };
            links.insert(key, child);
        }

        for (key, child) in links {
            let parent_field = self
                .vars
                .shape_states
                .get(&parent)
                .and_then(|state| match state {
                    ShapeIdentityState::Proven(parent_type) => {
                        precise_contained_shape_field(parent_type, &key)
                    }
                    ShapeIdentityState::Invalidated(_) => None,
                });
            let child_type = self.shape_identity_type(&BTreeSet::from([child])).ok();
            if parent_field.is_none() || parent_field != child_type {
                self.vars.invalidate_identities(
                    &BTreeSet::from([parent]),
                    UnknownReason::MutableShapeInvalidated,
                );
                return;
            }
            self.vars.link_contained_shape(parent, key, child);
        }
        self.vars.enforce_alias_bound();
    }

    fn link_non_mutating_merge_children(
        &mut self,
        node: &Node<'_>,
        new_parent: ShapeIdentity,
    ) -> bool {
        let Some(call) = node.as_call_node() else {
            return false;
        };
        if call.name().as_slice() != b"merge" {
            return false;
        }
        let Some(receiver) = call.receiver() else {
            return false;
        };
        let Some(receiver_local) = receiver.as_local_variable_read_node() else {
            return false;
        };
        let Some(arguments) = call.arguments() else {
            return false;
        };
        let argument_nodes = arguments.arguments().iter().collect::<Vec<_>>();
        if argument_nodes.len() != 1 {
            return false;
        }
        let right = &argument_nodes[0];
        let Some(right_hash) = right.as_hash_node() else {
            self.vars.invalidate_identities(
                &BTreeSet::from([new_parent]),
                UnknownReason::MutableShapeInvalidated,
            );
            return true;
        };
        let Some(overwritten_keys) = literal_hash_keys(&right_hash) else {
            self.vars.invalidate_identities(
                &BTreeSet::from([new_parent]),
                UnknownReason::MutableShapeInvalidated,
            );
            return true;
        };
        let receiver_name = String::from_utf8_lossy(receiver_local.name().as_slice());
        let source_parents = self.vars.shape_identities(receiver_name.as_ref());
        for source_parent in &source_parents {
            if self
                .materialize_direct_shape_children(*source_parent)
                .is_err()
            {
                self.vars.invalidate_identities(
                    &BTreeSet::from([new_parent]),
                    UnknownReason::MutableShapeInvalidated,
                );
                return true;
            }
        }

        let mut copied = BTreeMap::<LiteralKey, ShapeIdentity>::new();
        for link in self.vars.shape_containments.clone() {
            if !source_parents.contains(&link.parent) || overwritten_keys.contains(&link.key) {
                continue;
            }
            if copied
                .insert(link.key.clone(), link.child)
                .is_some_and(|previous| previous != link.child)
            {
                self.vars.invalidate_identities(
                    &BTreeSet::from([new_parent]),
                    UnknownReason::MutableShapeInvalidated,
                );
                return true;
            }
        }
        for (key, child) in copied {
            let child_type = self.shape_identity_type(&BTreeSet::from([child])).ok();
            let parent_field =
                self.vars
                    .shape_states
                    .get(&new_parent)
                    .and_then(|state| match state {
                        ShapeIdentityState::Proven(parent_type) => {
                            precise_contained_shape_field(parent_type, &key)
                        }
                        ShapeIdentityState::Invalidated(_) => None,
                    });
            if parent_field.is_none() || parent_field != child_type {
                self.vars.invalidate_identities(
                    &BTreeSet::from([new_parent]),
                    UnknownReason::MutableShapeInvalidated,
                );
                return true;
            }
            self.vars.link_contained_shape(new_parent, key, child);
        }

        self.link_shape_literal_children(right, new_parent);
        true
    }

    fn link_mutating_merge_children(
        &mut self,
        receivers: &BTreeSet<ShapeIdentity>,
        right: &Node<'_>,
        overwritten_keys: &BTreeSet<LiteralKey>,
    ) -> Result<(), UnknownReason> {
        if right.as_hash_node().is_some() {
            for receiver in receivers {
                self.link_shape_literal_children(right, *receiver);
            }
            self.shape_identity_type(receivers).map(|_| ())?;
            return Ok(());
        }

        let source_parents = self.shape_identities_for_alias_expression(right);
        if source_parents.is_empty() {
            // There is no local alias that must remain synchronized. Create
            // child identities from the now-complete receiver shape so later
            // keyed reads within this flow still share one object identity.
            for receiver in receivers {
                self.materialize_direct_shape_children(*receiver)?;
            }
            return Ok(());
        }

        for source_parent in &source_parents {
            self.materialize_direct_shape_children(*source_parent)?;
        }
        let mut copied = BTreeMap::<LiteralKey, ShapeIdentity>::new();
        for link in self.vars.shape_containments.clone() {
            if !source_parents.contains(&link.parent) || !overwritten_keys.contains(&link.key) {
                continue;
            }
            if copied
                .insert(link.key.clone(), link.child)
                .is_some_and(|previous| previous != link.child)
            {
                return Err(UnknownReason::MutableShapeInvalidated);
            }
        }

        for receiver in receivers {
            for (key, child) in &copied {
                let parent_field =
                    self.vars
                        .shape_states
                        .get(receiver)
                        .and_then(|state| match state {
                            ShapeIdentityState::Proven(parent_type) => {
                                precise_contained_shape_field(parent_type, key)
                            }
                            ShapeIdentityState::Invalidated(_) => None,
                        });
                let child_type = self.shape_identity_type(&BTreeSet::from([*child]))?;
                if parent_field.as_ref() != Some(&child_type) {
                    return Err(UnknownReason::MutableShapeInvalidated);
                }
                self.vars
                    .link_contained_shape(*receiver, key.clone(), *child);
            }
        }
        self.vars.enforce_alias_bound();
        self.shape_identity_type(receivers).map(|_| ())
    }

    fn materialize_direct_shape_children(
        &mut self,
        parent: ShapeIdentity,
    ) -> Result<(), UnknownReason> {
        let parent_type = match self.vars.shape_states.get(&parent).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: child materialization references absent parent identity {:?}. This is a bug because only allocated shape identities can own child fields. Fix: allocate the parent before materializing containment.",
                parent
            )
        }) {
            ShapeIdentityState::Proven(ruby_type) => ruby_type.clone(),
            ShapeIdentityState::Invalidated(reason) => return Err(*reason),
        };
        let RubyType::Shape(shape) = parent_type else {
            return Err(UnknownReason::MutableShapeInvalidated);
        };
        let fields = shape
            .fields()
            .iter()
            .filter(|field| field.is_required() && type_is_shape_only(field.value()))
            .map(|field| (field.key().clone(), field.value().clone()))
            .collect::<Vec<_>>();
        for (key, child_type) in fields {
            if self.vars.contained_child(parent, &key).is_some() {
                continue;
            }
            let child = self.allocate_shape_identity(child_type);
            self.vars.link_contained_shape(parent, key, child);
        }
        self.vars.enforce_alias_bound();
        Ok(())
    }

    fn observe_rescue_entry_type(&mut self, name: &str, ruby_type: &RubyType) {
        for entry_types in &mut self.rescue_entry_types {
            entry_types.observe(name, ruby_type);
        }
    }

    fn track_escaping_write(&mut self, value: &Node<'_>) -> RubyType {
        let identities = self.shape_identities_in_escape_expression(value);
        let value_type = self.track_node(value);
        if identities.is_empty() {
            return value_type;
        }
        self.vars
            .invalidate_identities(&identities, UnknownReason::MutableShapeInvalidated);
        RubyType::Unknown
    }

    fn narrow_shape_key_presence(&mut self, predicate: &Node<'_>, truth: bool) -> bool {
        let Some(call) = predicate.as_call_node() else {
            return true;
        };
        let method_name = call.name().as_slice();
        if method_name == b"!" && call.arguments().is_none() {
            return call
                .receiver()
                .map(|receiver| self.narrow_shape_key_presence(&receiver, !truth))
                .unwrap_or(true);
        }
        if !matches!(
            method_name,
            b"key?" | b"has_key?" | b"include?" | b"member?"
        ) {
            return true;
        }
        let Some(receiver) = call.receiver() else {
            return true;
        };
        let Some(read) = receiver.as_local_variable_read_node() else {
            return true;
        };
        let Some(arguments) = call.arguments() else {
            return true;
        };
        let argument_nodes = arguments.arguments().iter().collect::<Vec<_>>();
        if argument_nodes.len() != 1 {
            return true;
        }
        let Some(key) = literal_key(&argument_nodes[0]) else {
            return true;
        };
        let name = String::from_utf8_lossy(read.name().as_slice());
        let identities = self.vars.shape_identities(name.as_ref());
        if identities.is_empty() {
            return true;
        }

        self.apply_shape_identity_narrowing(&identities, |ruby_type| {
            narrow_shape_presence_type(ruby_type, &key, truth)
        })
    }

    fn narrow_shape_predicate(&mut self, predicate: &Node<'_>, truth: bool) -> bool {
        let Some(call) = predicate.as_call_node() else {
            return true;
        };
        if call.name().as_slice() == b"!" && call.arguments().is_none() {
            return call
                .receiver()
                .map(|receiver| self.narrow_shape_predicate(&receiver, !truth))
                .unwrap_or(true);
        }
        if let Some(reaches) = self.narrow_shape_literal_comparison(&call, truth) {
            return reaches;
        }
        self.narrow_shape_key_presence(predicate, truth)
    }

    fn narrow_shape_literal_comparison(
        &mut self,
        call: &CallNode<'_>,
        truth: bool,
    ) -> Option<bool> {
        let equality_when_true = match call.name().as_slice() {
            b"==" | b"eql?" => true,
            b"!=" => false,
            _ => return None,
        };
        let arguments = call.arguments()?;
        let argument_nodes = arguments.arguments().iter().collect::<Vec<_>>();
        if argument_nodes.len() != 1 {
            return None;
        }
        let receiver = call.receiver()?;
        let left_read = shape_local_key_read(&receiver);
        let right_read = shape_local_key_read(&argument_nodes[0]);
        let left_literal = literal_value(&receiver);
        let right_literal = literal_value(&argument_nodes[0]);
        let (name, key, literal) =
            if let (Some((name, key)), Some(literal)) = (left_read, right_literal) {
                (name, key, literal)
            } else if let (Some(literal), Some((name, key))) = (left_literal, right_read) {
                (name, key, literal)
            } else {
                return None;
            };
        let identities = self.vars.shape_identities(&name);
        if identities.is_empty() {
            return None;
        }
        let require_match = truth == equality_when_true;
        Some(
            self.apply_shape_identity_narrowing(&identities, |ruby_type| {
                narrow_shape_literal_type(ruby_type, &key, &literal, require_match)
            }),
        )
    }

    fn narrow_shape_literal_set(
        &mut self,
        name: &str,
        key: &LiteralKey,
        literals: &[LiteralValue],
        require_match: bool,
    ) -> bool {
        assert!(
            !literals.is_empty(),
            "INVARIANT VIOLATED: shape discriminator narrowing received an empty literal set. This is a bug because an empty Ruby when clause cannot reach this helper. Fix: require at least one supported literal condition before narrowing."
        );
        let identities = self.vars.shape_identities(name);
        if identities.is_empty() {
            return true;
        }
        self.apply_shape_identity_narrowing(&identities, |ruby_type| {
            narrow_shape_literal_set_type(ruby_type, key, literals, require_match)
        })
    }

    fn narrow_shape_hash_pattern(
        &mut self,
        predicate: &Node<'_>,
        pattern: &Node<'_>,
        require_match: bool,
    ) -> Option<bool> {
        let read = predicate.as_local_variable_read_node()?;
        let name = String::from_utf8_lossy(read.name().as_slice()).to_string();
        let requirements = hash_pattern_requirements(pattern)?;
        if requirements.is_empty() {
            return None;
        }
        let identities = self.vars.shape_identities(&name);
        if identities.is_empty() {
            return None;
        }
        let reaches = self.apply_shape_identity_narrowing(&identities, |ruby_type| {
            narrow_shape_pattern_type(ruby_type, &requirements, require_match)
        });
        if !reaches || !require_match {
            return Some(reaches);
        }

        // A successful Hash pattern proves every listed key is present even
        // when its source contract marked the field optional. Apply that
        // presence proof only after filtering complete variants; open/rest
        // shapes still remain conservative about the field value.
        for (key, _) in &requirements {
            let active_identities = self.vars.shape_identities(&name);
            let reaches = self.apply_shape_identity_narrowing(&active_identities, |ruby_type| {
                narrow_shape_presence_type(ruby_type, key, true)
            });
            if !reaches {
                return Some(false);
            }
        }
        Some(true)
    }

    fn apply_shape_identity_narrowing(
        &mut self,
        identities: &BTreeSet<ShapeIdentity>,
        mut narrow: impl FnMut(&RubyType) -> Result<Option<RubyType>, UnknownReason>,
    ) -> bool {
        let mut retained = BTreeSet::new();
        let mut updates = Vec::new();
        for identity in identities {
            let state = self.vars.shape_states.get(identity).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: shape guard references absent identity {:?}. This is a bug because a local binding and its identity states must be joined atomically. Fix: preserve the complete FlowEnvironment across branches.",
                    identity
                )
            });
            match state {
                ShapeIdentityState::Invalidated(_) => {
                    retained.insert(*identity);
                }
                ShapeIdentityState::Proven(ruby_type) => match narrow(ruby_type) {
                    Ok(Some(narrowed)) => {
                        retained.insert(*identity);
                        updates.push((*identity, narrowed));
                    }
                    Ok(None) => {}
                    Err(reason) => {
                        self.vars.invalidate_identities(identities, reason);
                        return true;
                    }
                },
            }
        }
        if retained.is_empty() {
            return false;
        }
        for (identity, ruby_type) in updates {
            self.vars
                .shape_states
                .insert(identity, ShapeIdentityState::Proven(ruby_type));
        }
        if &retained != identities {
            for binding in self.vars.shape_bindings.values_mut() {
                binding.retain(|identity| {
                    !identities.contains(identity) || retained.contains(identity)
                });
            }
        }
        self.vars.synchronize_shape_aliases();
        true
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
        self.vars = env_before.clone();
        let then_reaches = self.narrow_shape_predicate(&predicate, true);
        let then_diverges = !then_reaches
            || if_node
                .statements()
                .map(|s| control_flow::diverges(&s.as_node()))
                .unwrap_or(false);
        let then_type = if !then_reaches {
            RubyType::nil_class()
        } else if let Some(statements) = if_node.statements() {
            self.track_node(&statements.as_node())
        } else {
            RubyType::nil_class()
        };
        let then_env = self.vars.clone();

        self.vars = env_before.clone();

        // Else branch
        let else_reaches = self.narrow_shape_predicate(&predicate, false);
        let else_diverges = !else_reaches
            || if_node
                .subsequent()
                .map(|n| control_flow::diverges(&n))
                .unwrap_or(false);
        let else_type = if !else_reaches {
            RubyType::nil_class()
        } else if let Some(subsequent) = if_node.subsequent() {
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
        let predicate = case_node.predicate();
        if let Some(predicate) = &predicate {
            self.track_node(&predicate);
        }

        let env_before = self.vars.clone();

        let when_nodes = case_node
            .conditions()
            .iter()
            .filter_map(|condition| condition.as_when_node())
            .collect::<Vec<_>>();
        let when_literal_sets = when_nodes
            .iter()
            .map(|when_node| {
                let literals = when_node
                    .conditions()
                    .iter()
                    .map(|condition| literal_value(&condition))
                    .collect::<Option<Vec<_>>>()?;
                (!literals.is_empty()).then_some(literals)
            })
            .collect::<Vec<_>>();
        let discriminator = predicate.as_ref().and_then(shape_local_key_read);
        let discriminator_is_complete =
            discriminator.is_some() && when_literal_sets.iter().all(Option::is_some);
        let all_literals = discriminator_is_complete.then(|| {
            when_literal_sets
                .iter()
                .flat_map(|literals| {
                    literals
                        .as_ref()
                        .expect(
                            "INVARIANT VIOLATED: complete case discriminator lost a literal condition set. This is a bug because completeness was checked immediately before flattening. Fix: retain the checked sets unchanged.",
                        )
                        .iter()
                        .cloned()
                })
                .collect::<Vec<_>>()
        });

        // (env, type, diverges) per branch.
        let mut branches: Vec<(FlowEnvironment, RubyType, bool)> = Vec::new();

        for (when_node, literals) in when_nodes.iter().zip(&when_literal_sets) {
            self.vars = env_before.clone();
            let reaches = match (&discriminator, literals) {
                (Some((name, key)), Some(literals)) if discriminator_is_complete => {
                    self.narrow_shape_literal_set(name, key, literals, true)
                }
                (Some(_) | None, Some(_) | None) => true,
            };
            let diverges = !reaches
                || when_node
                    .statements()
                    .map(|s| control_flow::diverges(&s.as_node()))
                    .unwrap_or(false);
            let branch_type = if !reaches {
                RubyType::nil_class()
            } else if let Some(statements) = when_node.statements() {
                self.track_node(&statements.as_node())
            } else {
                RubyType::nil_class()
            };
            branches.push((self.vars.clone(), branch_type, diverges));
        }

        self.vars = env_before.clone();
        if let (Some((name, key)), Some(literals)) = (&discriminator, &all_literals) {
            self.narrow_shape_literal_set(name, key, literals, false);
        }
        let unmatched_env = self.vars.clone();
        if let Some(else_clause) = case_node.else_clause() {
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
            push_unmatched_ordinary_case_path(&mut branches, &unmatched_env);
        }

        if branches.is_empty() {
            return RubyType::nil_class();
        }

        // Pick post-state from non-diverging branches only.
        let surviving_envs: Vec<&FlowEnvironment> = branches
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
        let mut branches: Vec<(FlowEnvironment, RubyType, bool)> = Vec::new();

        let in_nodes = case_node
            .conditions()
            .iter()
            .filter_map(|condition| condition.as_in_node())
            .collect::<Vec<_>>();
        let patterns_supported = predicate.as_ref().is_some_and(|predicate| {
            predicate.as_local_variable_read_node().is_some()
                && in_nodes
                    .iter()
                    .all(|in_node| hash_pattern_requirements(&in_node.pattern()).is_some())
        });

        for (index, in_node) in in_nodes.iter().enumerate() {
            self.vars = env_before.clone();
            let reaches = if patterns_supported {
                let predicate = predicate.as_ref().expect(
                    "INVARIANT VIOLATED: supported Hash pattern case lost its predicate. This is a bug because patterns_supported requires one. Fix: retain the checked predicate for branch narrowing.",
                );
                let mut reaches = true;
                for prior in &in_nodes[..index] {
                    if !reaches {
                        break;
                    }
                    reaches = self
                        .narrow_shape_hash_pattern(predicate, &prior.pattern(), false)
                        .expect(
                            "INVARIANT VIOLATED: previously supported Hash pattern became unsupported. This is a bug because the immutable pattern was validated before branch traversal. Fix: use one shared pattern recognizer for validation and narrowing.",
                        );
                }
                if reaches {
                    reaches = self
                        .narrow_shape_hash_pattern(predicate, &in_node.pattern(), true)
                        .expect(
                            "INVARIANT VIOLATED: supported Hash pattern became unsupported before its branch. This is a bug because the immutable pattern was validated before traversal. Fix: use one shared pattern recognizer for validation and narrowing.",
                        );
                }
                reaches
            } else {
                true
            };
            if let Some(predicate) = &predicate {
                let captures = self.pattern_capture_types_for_value(&in_node.pattern(), predicate);
                for (name, ty) in captures {
                    if ty != RubyType::Unknown {
                        self.vars.insert(name, ty);
                    }
                }
            }

            let diverges = !reaches
                || in_node
                    .statements()
                    .map(|s| control_flow::diverges(&s.as_node()))
                    .unwrap_or(false);
            let branch_type = if !reaches {
                RubyType::nil_class()
            } else if let Some(statements) = in_node.statements() {
                self.track_node(&statements.as_node())
            } else {
                RubyType::nil_class()
            };
            branches.push((self.vars.clone(), branch_type, diverges));
        }

        let has_else = case_node.else_clause().is_some();
        if has_else {
            self.vars = env_before.clone();
            let mut else_reaches = true;
            if patterns_supported {
                let predicate = predicate.as_ref().expect(
                    "INVARIANT VIOLATED: supported Hash pattern else path lost its predicate. This is a bug because patterns_supported requires one. Fix: retain the checked predicate for unmatched narrowing.",
                );
                for in_node in &in_nodes {
                    if !else_reaches {
                        break;
                    }
                    else_reaches = self
                        .narrow_shape_hash_pattern(predicate, &in_node.pattern(), false)
                        .expect(
                            "INVARIANT VIOLATED: supported Hash pattern became unsupported on the else path. This is a bug because the immutable pattern was validated before traversal. Fix: use one shared pattern recognizer for validation and narrowing.",
                        );
                }
            }
            let else_clause = case_node.else_clause().unwrap();
            let diverges = !else_reaches
                || else_clause
                    .statements()
                    .map(|s| control_flow::diverges(&s.as_node()))
                    .unwrap_or(false);
            let else_type = if !else_reaches {
                RubyType::nil_class()
            } else if let Some(statements) = else_clause.statements() {
                self.track_node(&statements.as_node())
            } else {
                RubyType::nil_class()
            };
            branches.push((self.vars.clone(), else_type, diverges));
        }

        if branches.is_empty() {
            return RubyType::nil_class();
        }

        let surviving_envs: Vec<&FlowEnvironment> = branches
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
        if let Some(read) = value.as_local_variable_read_node() {
            let name = String::from_utf8_lossy(read.name().as_slice());
            if let Some(ruby_type) = self.vars.get(name.as_ref()).cloned() {
                self.collect_pattern_capture_types_from_type(pattern, &ruby_type, &mut captures);
                return captures;
            }
        }
        self.collect_pattern_capture_types(pattern, value, &mut captures);
        captures
    }

    fn collect_pattern_capture_types_from_type(
        &self,
        pattern: &Node<'_>,
        value_type: &RubyType,
        captures: &mut HashMap<String, RubyType>,
    ) {
        if let Some(implicit) = pattern.as_implicit_node() {
            self.collect_pattern_capture_types_from_type(&implicit.value(), value_type, captures);
            return;
        }
        if let Some(target) = pattern.as_local_variable_target_node() {
            let name = String::from_utf8_lossy(target.name().as_slice()).to_string();
            captures.insert(name, value_type.clone());
            return;
        }
        let Some(pattern_hash) = pattern.as_hash_pattern_node() else {
            return;
        };
        for element in pattern_hash.elements().iter() {
            let Some(assoc) = element.as_assoc_node() else {
                continue;
            };
            let Some(key) = literal_key(&assoc.key()) else {
                continue;
            };
            let Ok(field_type) = shape_reads::indexed_read(value_type, Some(&key)) else {
                continue;
            };
            self.collect_pattern_capture_types_from_type(&assoc.value(), &field_type, captures);
        }
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
        self.vars = env_before.clone();
        let then_reaches = self.narrow_shape_predicate(&predicate, false);
        let then_diverges = !then_reaches
            || unless_node
                .statements()
                .map(|s| control_flow::diverges(&s.as_node()))
                .unwrap_or(false);
        let then_type = if !then_reaches {
            RubyType::nil_class()
        } else if let Some(statements) = unless_node.statements() {
            self.track_node(&statements.as_node())
        } else {
            RubyType::nil_class()
        };
        let then_env = self.vars.clone();

        self.vars = env_before.clone();

        // Else branch
        let else_reaches = self.narrow_shape_predicate(&predicate, true);
        let else_diverges = !else_reaches
            || unless_node
                .else_clause()
                .and_then(|e| e.statements())
                .map(|s| control_flow::diverges(&s.as_node()))
                .unwrap_or(false);
        let else_type = if !else_reaches {
            RubyType::nil_class()
        } else if let Some(else_clause) = unless_node.else_clause() {
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
        // Hashes and arrays must use the flow-aware resolver recursively. The
        // syntax-only LiteralAnalyzer cannot prove local reads inside a nested
        // collection and would otherwise erase an already-proven shape.
        if let Some(result) = self.infer_collection_literal_type(node) {
            return result.unwrap_or(RubyType::Unknown);
        }

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
            let unknown_reason = self.vars.unknown_reason(&var_name);
            if self.record_local_read_types
                && (self.has_seen_control_flow
                    || type_contains_shape(&ruby_type)
                    || unknown_reason.is_some())
            {
                let location = read.location();
                let constant_dependencies = self.vars.dependencies(&var_name);
                self.local_read_types.push(LocalReadType {
                    start_offset: location.start_offset(),
                    end_offset: location.end_offset(),
                    name: var_name,
                    ruby_type: ruby_type.clone(),
                    unknown_reason,
                    constant_dependencies,
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

        // A Ruby constant can hold any value. Require an engine-owned value or
        // namespace fact instead of turning unresolved syntax into a class.
        // FactCollector's direct pass installs those facts before local-flow
        // inference, so this also covers constants declared in the same file.
        if let Some((parts, absolute)) = Self::constant_reference(node) {
            if let Some(ruby_type) = self.constant_value_type(&parts, absolute) {
                return ruby_type;
            }
            return RubyType::Unknown;
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

    /// Infer nested collection literals while preserving a shape-construction
    /// failure from any depth. Non-collection expressions retain the existing
    /// resolver and therefore represent ordinary incompleteness as
    /// `RubyType::Unknown`, not as a shape-bound error.
    fn infer_collection_literal_type(
        &mut self,
        node: &Node<'_>,
    ) -> Option<Result<RubyType, ShapeConstructionError>> {
        if let Some(hash) = node.as_hash_node() {
            return Some(infer_hash_literal_type_fallible(&hash, |value| {
                self.infer_collection_literal_type(value)
                    .unwrap_or_else(|| Ok(self.infer_expression(value)))
            }));
        }
        node.as_array_node().map(|array| {
            infer_array_literal_type_fallible(&array, |value| {
                self.infer_collection_literal_type(value)
                    .unwrap_or_else(|| Ok(self.infer_expression(value)))
            })
        })
    }

    /// Infer the return type of a method call
    fn infer_call(&mut self, call: &CallNode) -> RubyType {
        self.invalidate_escaped_callables_in_call(call);
        let method_name = String::from_utf8_lossy(call.name().as_slice()).to_string();

        if let Some(higher_order_type) = self.infer_rbs_higher_order_call(call, &method_name) {
            self.direct_call_return_proofs
                .insert(call.location().start_offset());
            return higher_order_type;
        }

        self.apply_array_shape_call_boundary(call, &method_name);

        // Shape effects are flow state, not ordinary Hash method-return
        // lookup. Apply them before resolving the call result so every later
        // alias read observes the same abstract identity state.
        if let Some(result) = self.infer_shape_call_effect(call, &method_name) {
            return result;
        }

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
        let mut receiver_type = if let Some(receiver) = call.receiver() {
            let inferred = self.infer_expression(&receiver);
            project_immediate_hash_receiver_type(&receiver, inferred)
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

        if type_is_shape_only(&receiver_type) {
            if let Some(return_type) =
                self.infer_shape_read_from_type(call, &method_name, &receiver_type)
            {
                return return_type;
            }
            receiver_type =
                shape_reads::generic_hash_projection(&receiver_type).unwrap_or(RubyType::Unknown);
            if receiver_type == RubyType::Unknown {
                return RubyType::Unknown;
            }
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

    fn infer_rbs_higher_order_call(
        &mut self,
        call: &CallNode<'_>,
        method_name: &str,
    ) -> Option<RubyType> {
        let block_expression = call.block()?;
        if method_name.ends_with('!') {
            return None;
        }
        let receiver_type = call.receiver().map(|receiver| {
            project_immediate_hash_receiver_type(&receiver, self.infer_expression(&receiver))
        });
        if receiver_type.as_ref().is_some_and(|receiver| {
            receiver == &RubyType::Unknown || RubyType::contains_unknown(receiver)
        }) {
            return None;
        }
        let argument_types = call
            .arguments()
            .map(|arguments| {
                arguments
                    .arguments()
                    .iter()
                    .map(|argument| self.track_node(&argument))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let prepared_result = if let Some(analysis_engine) = &self.analysis_engine {
            let engine = analysis_engine.read();
            let query = AnalysisQuery::new(&engine);
            let direct = receiver_type.as_ref().map_or_else(
                || Err(UnknownReason::UnsupportedCallable),
                |receiver_type| {
                    crate::inference::rbs::prepare_higher_order_call(
                        Some(&query),
                        receiver_type,
                        method_name,
                        &argument_types,
                    )
                },
            );
            direct.or_else(|_| {
                let namespace = FullyQualifiedName::namespace(
                    self.current_class
                        .as_ref()
                        .map(FullyQualifiedName::namespace_parts)
                        .unwrap_or_default(),
                );
                crate::inference::rbs::prepare_forwarded_higher_order_call(
                    &query,
                    receiver_type.as_ref(),
                    Some(&namespace),
                    method_name,
                    &argument_types,
                )
                .or_else(|_| {
                    crate::inference::rbs::prepare_direct_yield_higher_order_call(
                        &query,
                        receiver_type.as_ref(),
                        Some(&namespace),
                        method_name,
                        &argument_types,
                    )
                })
            })
        } else {
            receiver_type.as_ref().map_or_else(
                || Err(UnknownReason::UnsupportedCallable),
                |receiver_type| {
                    crate::inference::rbs::prepare_higher_order_call(
                        None,
                        receiver_type,
                        method_name,
                        &argument_types,
                    )
                },
            )
        };
        let prepared = prepared_result.ok()?;
        if let Some(block) = block_expression.as_block_node() {
            if block
                .body()
                .as_ref()
                .is_some_and(|body| control_flow::has_unsupported_higher_order_exit(body))
            {
                return None;
            }
            let parameter_types = prepared.block_parameter_types().to_vec();
            let parameter_names = block_parameter_names(&block);
            let environment_before = self.vars.clone();
            let explicit_return_count = self.explicit_return_types.len();
            for (index, name) in parameter_names.iter().enumerate() {
                let parameter_type = parameter_types
                    .get(index)
                    .cloned()
                    .unwrap_or_else(RubyType::nil_class);
                if type_is_shape_only(&parameter_type) {
                    let identity = self.allocate_shape_identity(parameter_type.clone());
                    self.vars.bind_shape_identities(
                        name.clone(),
                        parameter_type,
                        BTreeSet::from([identity]),
                    );
                } else {
                    self.vars.insert(name.clone(), parameter_type);
                }
            }
            let block_return_type = block
                .body()
                .map(|body| self.track_node(&body))
                .unwrap_or_else(RubyType::nil_class);
            let post_block_parameter_types = parameter_names
                .iter()
                .zip(&parameter_types)
                .map(|(name, original)| {
                    let identities = self.vars.shape_identities(name);
                    if identities.is_empty() {
                        original.clone()
                    } else {
                        self.shape_identity_type(&identities)
                            .unwrap_or(RubyType::Unknown)
                    }
                })
                .collect::<Vec<_>>();
            self.vars = environment_before;
            self.explicit_return_types.truncate(explicit_return_count);
            return prepared
                .finish_with_proven_block_state(&block_return_type, &post_block_parameter_types)
                .into_proven_type();
        }

        let block_argument = block_expression.as_block_argument_node()?;
        let expression = block_argument.expression()?;
        if let Some(symbol) = expression.as_symbol_node() {
            let target = std::str::from_utf8(symbol.unescaped()).ok()?;
            return prepared
                .finish_static_method(target, |receiver_type, target| {
                    self.resolve_static_method_return_outcome(receiver_type, target)
                })
                .into_proven_type();
        }
        let callable = if let Some(local) = expression.as_local_variable_read_node() {
            let name = String::from_utf8_lossy(local.name().as_slice()).to_string();
            self.vars.callables.get(&name)?.clone()
        } else {
            match self.constant_callable_body_for_node(&expression)? {
                Ok(summary) => crate::inference::higher_order::KnownProcType {
                    identity: u32::MAX,
                    summary: Ok(summary),
                },
                Err(_) => return None,
            }
        };
        let mut stack = vec![callable.identity];
        prepared
            .finish_known_proc(
                &callable,
                |capture| self.vars.types.get(capture).cloned(),
                |capture, arguments| {
                    let nested = self.vars.callables.get(capture)?.clone();
                    Some(self.instantiate_known_proc_with_stack(&nested, arguments, &mut stack))
                },
                |receiver, method, _arguments| {
                    self.resolve_static_method_return_outcome(receiver, method.as_str())
                },
            )
            .into_proven_type()
    }

    fn resolve_static_method_return_outcome(
        &self,
        receiver_type: &RubyType,
        method_name: &str,
    ) -> TypeInferenceOutcome {
        if let RubyType::Union(members) = receiver_type {
            let mut return_types = Vec::with_capacity(members.len());
            for member in members {
                let outcome = self.resolve_static_method_return_outcome(member, method_name);
                let Some(return_type) = outcome.into_proven_type() else {
                    return TypeInferenceOutcome::unknown(UnknownReason::IncompleteUnionMember);
                };
                return_types.push(return_type);
            }
            return TypeInferenceOutcome::from_optional(
                (!return_types.is_empty()).then(|| RubyType::union(return_types)),
                UnknownReason::IncompleteUnionMember,
            );
        }
        TypeInferenceOutcome::from_optional(
            self.resolve_method_return_type_from_analysis(receiver_type, method_name, false)
                .or_else(|| self.resolve_rbs_method_return_type(receiver_type, method_name)),
            UnknownReason::UnresolvedMethodReturn,
        )
    }

    fn apply_array_shape_call_boundary(&mut self, call: &CallNode<'_>, method_name: &str) {
        let Some(receiver) = call.receiver() else {
            return;
        };
        let Some(local) = receiver.as_local_variable_read_node() else {
            return;
        };
        let name = String::from_utf8_lossy(local.name().as_slice()).to_string();
        let Some(aliases) = self.vars.array_shape_aliases.get(&name).cloned() else {
            return;
        };
        let argument_nodes = call
            .arguments()
            .map(|arguments| arguments.arguments().iter().collect::<Vec<_>>())
            .unwrap_or_default();
        let preserves_positional_proof = match method_name {
            "first" | "last" | "to_a" | "freeze" => argument_nodes.is_empty(),
            "[]" | "at" | "fetch" => {
                argument_nodes.len() == 1 && literal_array_index(&argument_nodes[0]).is_some()
            }
            "length" | "size" | "empty?" | "include?" | "member?" | "index" | "rindex" | "join"
            | "inspect" | "hash" | "eql?" | "==" => true,
            _ => false,
        };
        if preserves_positional_proof {
            return;
        }

        self.vars
            .invalidate_identities(&aliases.contained, UnknownReason::MutableShapeInvalidated);
    }

    fn infer_shape_read_from_type(
        &mut self,
        call: &CallNode<'_>,
        method_name: &str,
        receiver_type: &RubyType,
    ) -> Option<RubyType> {
        let argument_nodes = call
            .arguments()
            .map(|arguments| arguments.arguments().iter().collect::<Vec<_>>())
            .unwrap_or_default();
        let argument_types = argument_nodes
            .iter()
            .map(|argument| self.track_node(argument))
            .collect::<Vec<_>>();
        match method_name {
            "[]" if argument_nodes.len() == 1 => Some(
                shape_reads::indexed_read(receiver_type, literal_key(&argument_nodes[0]).as_ref())
                    .unwrap_or(RubyType::Unknown),
            ),
            "fetch" if matches!(argument_nodes.len(), 1 | 2) => Some(
                shape_reads::fetch(
                    receiver_type,
                    literal_key(&argument_nodes[0]).as_ref(),
                    argument_types.get(1),
                )
                .unwrap_or(RubyType::Unknown),
            ),
            "dig" if !argument_nodes.is_empty() => {
                let keys = argument_nodes.iter().map(literal_key).collect::<Vec<_>>();
                Some(shape_reads::dig(receiver_type, &keys).unwrap_or(RubyType::Unknown))
            }
            "key?" | "has_key?" | "include?" | "member?" if argument_nodes.len() == 1 => Some(
                shape_reads::key_presence(receiver_type, literal_key(&argument_nodes[0]).as_ref())
                    .unwrap_or(RubyType::Unknown),
            ),
            "keys" if argument_nodes.is_empty() => {
                Some(shape_reads::keys(receiver_type).unwrap_or(RubyType::Unknown))
            }
            "values" if argument_nodes.is_empty() => {
                Some(shape_reads::values(receiver_type).unwrap_or(RubyType::Unknown))
            }
            "each" | "each_pair" | "each_key" | "each_value" if argument_nodes.is_empty() => Some(
                shape_reads::each_return(receiver_type, call.block().is_some())
                    .unwrap_or(RubyType::Unknown),
            ),
            "[]" | "fetch" | "dig" | "key?" | "has_key?" | "include?" | "member?" | "keys"
            | "values" | "each" | "each_pair" | "each_key" | "each_value" => {
                Some(RubyType::Unknown)
            }
            _ => None,
        }
    }

    fn infer_shape_call_effect(
        &mut self,
        call: &CallNode<'_>,
        method_name: &str,
    ) -> Option<RubyType> {
        let receiver_identities = call
            .receiver()
            .map(|receiver| self.shape_identities_for_alias_expression(&receiver))
            .unwrap_or_default();
        let argument_nodes = call
            .arguments()
            .map(|arguments| arguments.arguments().iter().collect::<Vec<_>>())
            .unwrap_or_default();
        let mut escaped_argument_identities = BTreeSet::new();
        for argument in &argument_nodes {
            escaped_argument_identities
                .extend(self.shape_identities_in_escape_expression(argument));
        }

        if receiver_identities.is_empty() && escaped_argument_identities.is_empty() {
            return None;
        }

        // Ruby evaluates the receiver before explicit arguments. Identity
        // lookup alone is not a semantic read: track the receiver expression
        // so its exact read-site type (or invalidation reason) is installed
        // for hover, chained dispatch, and every other engine consumer.
        if let Some(receiver) = call.receiver() {
            self.track_node(&receiver);
        }

        // Evaluate explicit arguments before entering the method. This also
        // lets a nested shape-producing expression provide the exact value
        // used by a known mutation.
        let argument_types = argument_nodes
            .iter()
            .map(|argument| self.track_node(argument))
            .collect::<Vec<_>>();

        if receiver_identities.is_empty() {
            self.vars.invalidate_identities(
                &escaped_argument_identities,
                UnknownReason::MutableShapeInvalidated,
            );
            return None;
        }

        match method_name {
            "[]" if argument_nodes.len() == 1 => Some(
                shape_reads::indexed_read(
                    &self
                        .shape_identity_type(&receiver_identities)
                        .unwrap_or(RubyType::Unknown),
                    literal_key(&argument_nodes[0]).as_ref(),
                )
                .unwrap_or(RubyType::Unknown),
            ),
            "fetch" if matches!(argument_nodes.len(), 1 | 2) => Some(
                shape_reads::fetch(
                    &self
                        .shape_identity_type(&receiver_identities)
                        .unwrap_or(RubyType::Unknown),
                    literal_key(&argument_nodes[0]).as_ref(),
                    argument_types.get(1),
                )
                .unwrap_or(RubyType::Unknown),
            ),
            "dig" if !argument_nodes.is_empty() => {
                let keys = argument_nodes.iter().map(literal_key).collect::<Vec<_>>();
                Some(
                    shape_reads::dig(
                        &self
                            .shape_identity_type(&receiver_identities)
                            .unwrap_or(RubyType::Unknown),
                        &keys,
                    )
                    .unwrap_or(RubyType::Unknown),
                )
            }
            "key?" | "has_key?" | "include?" | "member?" if argument_nodes.len() == 1 => Some(
                shape_reads::key_presence(
                    &self
                        .shape_identity_type(&receiver_identities)
                        .unwrap_or(RubyType::Unknown),
                    literal_key(&argument_nodes[0]).as_ref(),
                )
                .unwrap_or(RubyType::Unknown),
            ),
            "keys" if argument_nodes.is_empty() => Some(
                shape_reads::keys(
                    &self
                        .shape_identity_type(&receiver_identities)
                        .unwrap_or(RubyType::Unknown),
                )
                .unwrap_or(RubyType::Unknown),
            ),
            "values" if argument_nodes.is_empty() => Some(
                shape_reads::values(
                    &self
                        .shape_identity_type(&receiver_identities)
                        .unwrap_or(RubyType::Unknown),
                )
                .unwrap_or(RubyType::Unknown),
            ),
            "each" | "each_pair" | "each_key" | "each_value" if argument_nodes.is_empty() => Some(
                self.shape_identity_type(&receiver_identities)
                    .and_then(|receiver_type| {
                        shape_reads::each_return(&receiver_type, call.block().is_some())
                    })
                    .unwrap_or(RubyType::Unknown),
            ),
            "[]=" if argument_nodes.len() == 2 => {
                let Some(key) = literal_key(&argument_nodes[0]) else {
                    self.vars.invalidate_identities(
                        &receiver_identities,
                        UnknownReason::MutableShapeInvalidated,
                    );
                    return Some(RubyType::Unknown);
                };
                let value_type = argument_types[1].clone();
                if value_type == RubyType::Unknown {
                    let affected = receiver_identities
                        .union(&escaped_argument_identities)
                        .copied()
                        .collect();
                    self.vars
                        .invalidate_identities(&affected, UnknownReason::MutableShapeInvalidated);
                    return Some(RubyType::Unknown);
                }
                self.vars
                    .detach_contained_shapes(&receiver_identities, Some(&key));
                if self
                    .transform_shape_identities(&receiver_identities, |shape| {
                        shape_with_required_field(shape, key.clone(), value_type.clone())
                    })
                    .is_err()
                {
                    return Some(RubyType::Unknown);
                }
                if !escaped_argument_identities.is_empty()
                    && self
                        .link_assigned_shape_child(
                            &receiver_identities,
                            &key,
                            &escaped_argument_identities,
                            &value_type,
                        )
                        .is_err()
                {
                    let affected = receiver_identities
                        .union(&escaped_argument_identities)
                        .copied()
                        .collect();
                    self.vars
                        .invalidate_identities(&affected, UnknownReason::MutableShapeInvalidated);
                    return Some(RubyType::Unknown);
                }
                Some(value_type)
            }
            "delete" if argument_nodes.len() == 1 => {
                let Some(key) = literal_key(&argument_nodes[0]) else {
                    self.vars.invalidate_identities(
                        &receiver_identities,
                        UnknownReason::MutableShapeInvalidated,
                    );
                    return Some(RubyType::Unknown);
                };
                let deleted_types = self
                    .shape_identity_type(&receiver_identities)
                    .ok()
                    .map(|receiver_type| shape_literal_read_type(&receiver_type, &key));
                self.vars
                    .detach_contained_shapes(&receiver_identities, Some(&key));
                if self
                    .transform_shape_identities(&receiver_identities, |shape| {
                        shape_without_field(shape, &key)
                    })
                    .is_err()
                {
                    return Some(RubyType::Unknown);
                }
                Some(deleted_types.unwrap_or(RubyType::Unknown))
            }
            "clear" if argument_nodes.is_empty() => {
                self.vars
                    .detach_contained_shapes(&receiver_identities, None);
                let result = self
                    .transform_shape_identities(&receiver_identities, shape_cleared)
                    .unwrap_or(RubyType::Unknown);
                Some(result)
            }
            "merge!" | "update" if argument_types.len() == 1 => {
                let right = argument_types[0].clone();
                let overwritten_keys = match shape_field_keys(&right) {
                    Ok(keys) => keys,
                    Err(reason) => {
                        self.vars
                            .invalidate_identities(&receiver_identities, reason);
                        return Some(RubyType::Unknown);
                    }
                };
                for key in &overwritten_keys {
                    self.vars
                        .detach_contained_shapes(&receiver_identities, Some(key));
                }
                if self
                    .transform_shape_identities(&receiver_identities, |left| {
                        merge_shape_types(left, &right, true)
                    })
                    .is_err()
                {
                    return Some(RubyType::Unknown);
                }
                if let Err(reason) = self.link_mutating_merge_children(
                    &receiver_identities,
                    &argument_nodes[0],
                    &overwritten_keys,
                ) {
                    let affected = receiver_identities
                        .union(&escaped_argument_identities)
                        .copied()
                        .collect();
                    self.vars.invalidate_identities(&affected, reason);
                    return Some(RubyType::Unknown);
                }
                Some(self.shape_identity_type(&receiver_identities).expect(
                    "INVARIANT VIOLATED: mutating merge child linking succeeded but its receiver shape is not proven. This is a bug because the helper validates the receiver after installing every containment edge. Fix: return an error from link_mutating_merge_children whenever alias-bound enforcement invalidates the receiver.",
                ))
            }
            "merge" if argument_types.len() == 1 => {
                let left = self
                    .shape_identity_type(&receiver_identities)
                    .unwrap_or(RubyType::Unknown);
                let merged = merge_shape_types(&left, &argument_types[0], false)
                    .unwrap_or(RubyType::Unknown);
                Some(merged)
            }
            "freeze" if argument_nodes.is_empty() => {
                let result = self
                    .transform_shape_identities(&receiver_identities, shape_frozen)
                    .unwrap_or(RubyType::Unknown);
                Some(result)
            }
            // These calls do not change the receiver's field set. Phase 4
            // owns their precise results; retaining identity here merely
            // prevents a read from being mistaken for an unsupported mutator.
            "[]" | "fetch" | "dig" | "key?" | "has_key?" | "include?" | "member?" | "keys"
            | "values" | "each" | "each_pair" | "each_key" | "each_value" | "empty?" | "length"
            | "size" | "to_h" | "inspect" | "hash" | "eql?" | "==" | "merge" => None,
            _ => {
                let affected = receiver_identities
                    .union(&escaped_argument_identities)
                    .copied()
                    .collect();
                self.vars
                    .invalidate_identities(&affected, UnknownReason::MutableShapeInvalidated);
                Some(RubyType::Unknown)
            }
        }
    }

    fn shape_identities_in_escape_expression(&self, node: &Node<'_>) -> BTreeSet<ShapeIdentity> {
        let direct = self.shape_identities_for_alias_expression(node);
        if !direct.is_empty() {
            return direct;
        }
        if let Some(local) = node.as_local_variable_read_node() {
            let name = String::from_utf8_lossy(local.name().as_slice());
            if let Some(aliases) = self.vars.array_shape_aliases.get(name.as_ref()) {
                return aliases.contained.clone();
            }
        }
        if let Some(array) = node.as_array_node() {
            return array
                .elements()
                .iter()
                .flat_map(|element| self.shape_identities_in_escape_expression(&element))
                .collect();
        }
        if let Some(hash) = node.as_hash_node() {
            let mut identities = BTreeSet::new();
            for element in hash.elements().iter() {
                if let Some(assoc) = element.as_assoc_node() {
                    identities.extend(self.shape_identities_in_escape_expression(&assoc.key()));
                    identities.extend(self.shape_identities_in_escape_expression(&assoc.value()));
                } else if let Some(splat) = element.as_assoc_splat_node() {
                    if let Some(value) = splat.value() {
                        identities.extend(self.shape_identities_in_escape_expression(&value));
                    }
                }
            }
            return identities;
        }
        BTreeSet::new()
    }

    fn shape_identity_type(
        &self,
        identities: &BTreeSet<ShapeIdentity>,
    ) -> Result<RubyType, UnknownReason> {
        let mut alternatives = Vec::new();
        for identity in identities {
            match self.vars.shape_states.get(identity).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: call receiver references absent shape identity {:?}. This is a bug because receiver bindings and identity states must be cloned and joined together. Fix: merge the complete FlowEnvironment.",
                    identity
                )
            }) {
                ShapeIdentityState::Proven(ruby_type) => {
                    alternatives.extend(shape_alternatives(ruby_type)?);
                }
                ShapeIdentityState::Invalidated(reason) => return Err(*reason),
            }
        }
        let joined = RubyType::union(alternatives);
        (joined != RubyType::Unknown)
            .then_some(joined)
            .ok_or(UnknownReason::ShapeBoundExceeded)
    }

    fn transform_shape_identities(
        &mut self,
        identities: &BTreeSet<ShapeIdentity>,
        mut transform: impl FnMut(&RubyType) -> Result<RubyType, UnknownReason>,
    ) -> Result<RubyType, UnknownReason> {
        let mut transitions = BTreeMap::new();
        for identity in identities {
            let current = match self.vars.shape_states.get(identity).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: shape transform references absent identity {:?}. This is a bug because mutations may target only live aliases. Fix: preserve identity states through branch cloning and joins.",
                    identity
                )
            }) {
                ShapeIdentityState::Proven(ruby_type) => ruby_type.clone(),
                ShapeIdentityState::Invalidated(reason) => return Err(*reason),
            };
            let mut identity_transitions = Vec::new();
            for before in shape_alternatives(&current)? {
                match transform(&before) {
                    Ok(after) if after != RubyType::Unknown => {
                        identity_transitions.push(ShapeAlternativeTransition { before, after });
                    }
                    Ok(_) => {
                        self.vars
                            .invalidate_identities(identities, UnknownReason::ShapeBoundExceeded);
                        return Err(UnknownReason::ShapeBoundExceeded);
                    }
                    Err(reason) => {
                        self.vars.invalidate_identities(identities, reason);
                        return Err(reason);
                    }
                }
            }
            let ruby_type = RubyType::union(
                identity_transitions
                    .iter()
                    .map(|transition| transition.after.clone()),
            );
            if ruby_type == RubyType::Unknown {
                self.vars
                    .invalidate_identities(identities, UnknownReason::ShapeBoundExceeded);
                return Err(UnknownReason::ShapeBoundExceeded);
            }
            self.vars
                .shape_states
                .insert(*identity, ShapeIdentityState::Proven(ruby_type));
            transitions.insert(*identity, identity_transitions);
        }
        self.propagate_contained_shape_updates(transitions)?;
        self.vars.synchronize_shape_aliases();
        self.shape_identity_type(identities)
    }

    fn propagate_contained_shape_updates(
        &mut self,
        mut changed: BTreeMap<ShapeIdentity, Vec<ShapeAlternativeTransition>>,
    ) -> Result<(), UnknownReason> {
        for _ in 0..crate::core::MAX_SHAPE_SOLVE_ITERATIONS {
            if changed.is_empty() {
                return Ok(());
            }
            let mut links_by_parent = BTreeMap::<ShapeIdentity, Vec<ShapeContainment>>::new();
            for link in &self.vars.shape_containments {
                if changed.contains_key(&link.child) {
                    links_by_parent
                        .entry(link.parent)
                        .or_default()
                        .push(link.clone());
                }
            }
            let mut changed_parents = BTreeMap::new();
            for (parent, links) in links_by_parent {
                let parent_type = match self.vars.shape_states.get(&parent).unwrap_or_else(|| {
                    panic!(
                        "INVARIANT VIOLATED: containment propagation references absent parent identity {:?}. This is a bug because an edge and both endpoint states must be cloned and joined atomically. Fix: preserve shape_containments with shape_states.",
                        parent
                    )
                }) {
                    ShapeIdentityState::Proven(ruby_type) => ruby_type.clone(),
                    ShapeIdentityState::Invalidated(reason) => return Err(*reason),
                };
                let mut parent_transitions = Vec::new();
                for before in shape_alternatives(&parent_type)? {
                    let mut after = before.clone();
                    for link in &links {
                        let child_transitions = changed.get(&link.child).unwrap_or_else(|| {
                            panic!(
                                "INVARIANT VIOLATED: containment worklist lost child transition {:?}. This is a bug because links_by_parent was derived from the same changed map. Fix: keep one immutable worklist generation.",
                                link.child
                            )
                        });
                        after =
                            shape_with_contained_transitions(&after, &link.key, child_transitions)?;
                    }
                    parent_transitions.push(ShapeAlternativeTransition { before, after });
                }
                let updated = RubyType::union(
                    parent_transitions
                        .iter()
                        .map(|transition| transition.after.clone()),
                );
                if updated == RubyType::Unknown {
                    self.vars.invalidate_identities(
                        &BTreeSet::from([parent]),
                        UnknownReason::ShapeBoundExceeded,
                    );
                    return Err(UnknownReason::ShapeBoundExceeded);
                }
                self.vars
                    .shape_states
                    .insert(parent, ShapeIdentityState::Proven(updated));
                changed_parents.insert(parent, parent_transitions);
            }
            changed = changed_parents;
        }

        let remaining = changed.keys().copied().collect::<BTreeSet<_>>();
        self.vars
            .invalidate_identities(&remaining, UnknownReason::ShapeBoundExceeded);
        Err(UnknownReason::ShapeBoundExceeded)
    }

    fn link_assigned_shape_child(
        &mut self,
        parents: &BTreeSet<ShapeIdentity>,
        key: &LiteralKey,
        children: &BTreeSet<ShapeIdentity>,
        value_type: &RubyType,
    ) -> Result<(), UnknownReason> {
        let child = children
            .iter()
            .copied()
            .next()
            .filter(|_| children.len() == 1)
            .ok_or(UnknownReason::MutableShapeInvalidated)?;
        if parents.contains(&child) || !type_is_shape_only(value_type) {
            return Err(UnknownReason::MutableShapeInvalidated);
        }
        let child_type = self.shape_identity_type(children)?;
        if child_type != *value_type {
            return Err(UnknownReason::MutableShapeInvalidated);
        }
        for parent in parents {
            let parent_field = match self.vars.shape_states.get(parent).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: shape field assignment references absent parent identity {:?}. This is a bug because receiver bindings and states must be installed atomically. Fix: preserve both through mutation.",
                    parent
                )
            }) {
                ShapeIdentityState::Proven(parent_type) => {
                    precise_contained_shape_field(parent_type, key)
                }
                ShapeIdentityState::Invalidated(_) => None,
            };
            if parent_field.as_ref() != Some(value_type) {
                return Err(UnknownReason::MutableShapeInvalidated);
            }
        }
        for parent in parents {
            self.vars.link_contained_shape(*parent, key.clone(), child);
        }
        self.vars.enforce_alias_bound();
        self.vars.synchronize_shape_aliases();
        Ok(())
    }

    fn infer_proc_call_return_type(
        &mut self,
        call: &CallNode,
        method_name: &str,
    ) -> Option<RubyType> {
        if method_name != "call" {
            return None;
        }
        let receiver = call.receiver()?;
        let callable = if let Some(local) = receiver.as_local_variable_read_node() {
            let name = String::from_utf8_lossy(local.name().as_slice()).to_string();
            self.vars.callables.get(&name)?.clone()
        } else {
            match self.constant_callable_body_for_node(&receiver)? {
                Ok(summary) => crate::inference::higher_order::KnownProcType {
                    identity: u32::MAX,
                    summary: Ok(summary),
                },
                Err(_) => return None,
            }
        };
        let argument_types = call
            .arguments()
            .map(|arguments| {
                arguments
                    .arguments()
                    .iter()
                    .map(|argument| self.track_node(&argument))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        self.instantiate_known_proc_with_stack(&callable, &argument_types, &mut Vec::new())
            .into_proven_type()
    }

    fn instantiate_known_proc_with_stack(
        &self,
        callable: &crate::inference::higher_order::KnownProcType,
        arguments: &[RubyType],
        stack: &mut Vec<u32>,
    ) -> TypeInferenceOutcome {
        if stack.contains(&callable.identity) {
            return TypeInferenceOutcome::unknown(UnknownReason::CallableRecursionUnsupported);
        }
        if stack.len() >= crate::core::callable_body::MAX_CALLABLE_BODY_INSTANTIATIONS {
            return TypeInferenceOutcome::unknown(UnknownReason::CallableBodyBoundExceeded);
        }
        let summary = match &callable.summary {
            Ok(summary) => summary,
            Err(reason) => return TypeInferenceOutcome::unknown(*reason),
        };
        stack.push(callable.identity);
        let result = crate::inference::callable_body::instantiate_callable_body(
            summary,
            arguments,
            |capture| self.vars.types.get(capture).cloned(),
            |capture, nested_arguments| {
                let nested = self.vars.callables.get(capture)?.clone();
                Some(self.instantiate_known_proc_with_stack(&nested, nested_arguments, stack))
            },
            |receiver, method, _arguments| {
                self.resolve_static_method_return_outcome(receiver, method.as_str())
            },
        );
        let popped = stack.pop().expect(
            "INVARIANT VIOLATED: callable instantiation stack underflowed. This is a bug because every accepted callable pushes exactly one identity. Fix: keep push/evaluate/pop in one function.",
        );
        assert_eq!(
            popped, callable.identity,
            "INVARIANT VIOLATED: callable instantiation stack order changed during evaluation. This is a bug because nested evaluation must be strictly LIFO. Fix: do not retain or reorder stack entries."
        );
        result
    }

    fn infer_proc_literal_return_type(
        &mut self,
        value: &Node,
    ) -> Option<crate::inference::higher_order::KnownProcType> {
        crate::indexer::is_static_callable_literal(value).then(|| {
            let outer_locals = self.vars.types.keys().cloned();
            crate::inference::higher_order::KnownProcType {
                identity: u32::try_from(value.location().start_offset()).expect(
                    "INVARIANT VIOLATED: callable literal offset exceeded u32. This is a bug because analysis ranges already require u32 offsets. Fix: reject oversized source before callable lowering.",
                ),
                summary: crate::indexer::lower_callable_literal_with_outer_locals(
                    value,
                    outer_locals,
                ),
            }
        })
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
            RubyType::Shape(shape) => {
                return self.resolve_rbs_method_return_type(&shape.generic_hash_type(), method_name)
            }
            RubyType::Literal(value) => {
                return self.resolve_rbs_method_return_type(&value.widened_type(), method_name)
            }
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
            RubyType::Literal(value) => {
                return self.resolve_method_return_type_from_analysis(
                    &value.widened_type(),
                    method_name,
                    allow_private,
                );
            }
            RubyType::Array(_)
            | RubyType::Hash(_, _)
            | RubyType::Shape(_)
            | RubyType::Union(_)
            | RubyType::Unknown => {
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
                    RubyType::Literal(_) | RubyType::Shape(_) => {
                        return None;
                    }
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
            RubyType::Literal(_) | RubyType::Shape(_) => return None,
            RubyType::Array(_) | RubyType::Hash(_, _) | RubyType::Union(_) | RubyType::Unknown => {
                return None;
            }
        };
        let method_fqn = FullyQualifiedName::method(parts, method.clone());
        if require_public && !self.local_public_method_candidates.contains(&method_fqn) {
            return None;
        }
        self.local_method_returns.get(&method_fqn).cloned()
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
            if let Some(return_type) = self.local_method_returns.get(&super_method) {
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
        if let Some(return_type) = self.local_method_returns.get(&super_method) {
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
            None => {
                let constant_dependencies = ret
                    .arguments()
                    .and_then(|arguments| {
                        let mut args = arguments.arguments().iter();
                        let first = args.next()?;
                        args.next()
                            .is_none()
                            .then(|| self.constant_dependencies_for_node(&first))
                    })
                    .unwrap_or_default();
                if constant_dependencies.is_empty() {
                    RecursiveReturnApproximation::from_ruby_type(return_type.clone())
                } else {
                    self.observed_return_constant_dependencies
                        .extend(constant_dependencies);
                    if return_type == RubyType::Unknown {
                        RecursiveReturnApproximation::Bottom
                    } else {
                        RecursiveReturnApproximation::from_ruby_type(return_type.clone())
                    }
                }
            }
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

    fn constant_dependencies_for_node(&self, node: &Node<'_>) -> BTreeSet<ConstantTypeDependency> {
        if let Some(statements) = node.as_statements_node() {
            return statements
                .body()
                .iter()
                .last()
                .map(|last| self.constant_dependencies_for_node(&last))
                .unwrap_or_default();
        }
        if let Some(parentheses) = node.as_parentheses_node() {
            return parentheses
                .body()
                .map(|body| self.constant_dependencies_for_node(&body))
                .unwrap_or_default();
        }
        if let Some(read) = node.as_local_variable_read_node() {
            let name = String::from_utf8_lossy(read.name().as_slice());
            return self.vars.dependencies(name.as_ref());
        }
        if let Some(write) = node.as_local_variable_write_node() {
            let name = String::from_utf8_lossy(write.name().as_slice());
            return self.vars.dependencies(name.as_ref());
        }
        if let Some(call) = node.as_call_node() {
            if call.name().as_slice() == b"new" {
                let Some(receiver) = call.receiver() else {
                    return BTreeSet::new();
                };
                let Some((parts, absolute)) = Self::constant_reference(&receiver) else {
                    return BTreeSet::new();
                };
                let lexical_context = self
                    .current_class
                    .as_ref()
                    .map(FullyQualifiedName::namespace_parts)
                    .unwrap_or_default();
                return BTreeSet::from([ConstantTypeDependency::constructor(
                    parts,
                    absolute,
                    lexical_context,
                )]);
            }
        }
        let Some((parts, absolute)) = Self::constant_reference(node) else {
            return BTreeSet::new();
        };
        let lexical_context = self
            .current_class
            .as_ref()
            .map(FullyQualifiedName::namespace_parts)
            .unwrap_or_default();
        BTreeSet::from([ConstantTypeDependency::new(
            parts,
            absolute,
            lexical_context,
        )])
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

    fn constant_value_type(&self, parts: &[RubyConstant], absolute: bool) -> Option<RubyType> {
        let analysis_engine = self.analysis_engine.as_ref()?;
        let engine = analysis_engine.read();
        let query = AnalysisQuery::new(&engine);
        let constant = if absolute {
            FullyQualifiedName::constant(parts.to_vec())
        } else {
            let lexical_context = self
                .current_class
                .as_ref()
                .map(FullyQualifiedName::namespace_parts)
                .unwrap_or_default();
            let resolved = query.resolve_constant_in_context(parts, &lexical_context)?;
            FullyQualifiedName::constant(resolved.namespace_parts())
        };
        query
            .constant_value_type(&constant)
            .or_else(|| query.constant_reference_type(constant.namespace_parts_slice()))
    }

    fn constant_callable_body_for_node(
        &self,
        node: &Node<'_>,
    ) -> Option<Result<crate::core::CallableBodySummary, UnknownReason>> {
        let (parts, absolute) = Self::constant_reference(node)?;
        let analysis_engine = self.analysis_engine.as_ref()?;
        let engine = analysis_engine.read();
        let query = AnalysisQuery::new(&engine);
        let constant = if absolute {
            FullyQualifiedName::constant(parts)
        } else {
            let lexical_context = self
                .current_class
                .as_ref()
                .map(FullyQualifiedName::namespace_parts)
                .unwrap_or_default();
            let resolved = query.resolve_constant_in_context(&parts, &lexical_context)?;
            FullyQualifiedName::constant(resolved.namespace_parts())
        };
        query.constant_callable_body(&constant)
    }

    fn constant_reference(node: &Node<'_>) -> Option<(Vec<RubyConstant>, bool)> {
        if let Some(constant_read) = node.as_constant_read_node() {
            let name = String::from_utf8_lossy(constant_read.name().as_slice());
            return Some((vec![RubyConstant::new(name.as_ref()).ok()?], false));
        }

        let constant_path = node.as_constant_path_node()?;
        let fqn = Self::resolve_constant_path(&constant_path)?;
        let absolute = Self::constant_path_is_absolute(&constant_path);
        Some((fqn.namespace_parts(), absolute))
    }

    fn constant_path_is_absolute(constant_path: &ConstantPathNode<'_>) -> bool {
        match constant_path.parent() {
            None => true,
            Some(parent) => parent
                .as_constant_path_node()
                .is_some_and(|parent| Self::constant_path_is_absolute(&parent)),
        }
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
    fn merge_env(&mut self, other_env: &FlowEnvironment, no_else_branch: bool) {
        let this_env = self.vars.clone();
        // For each variable in other environment
        for (var, other_ty) in &other_env.types {
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
            for (var, this_ty) in self.vars.types.clone() {
                if !other_env.contains_key(&var) {
                    let union = RubyType::union(vec![this_ty, RubyType::nil_class()]);
                    self.vars.insert(var, union);
                }
            }
        } else {
            // Has else branch: variables in then but not else get nil union
            for (var, this_ty) in self.vars.types.clone() {
                if !other_env.contains_key(&var) {
                    let union = RubyType::union(vec![this_ty, RubyType::nil_class()]);
                    self.vars.insert(var, union);
                }
            }
        }

        let mut names = this_env
            .types
            .keys()
            .chain(other_env.types.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut merged_dependencies = HashMap::new();
        for name in std::mem::take(&mut names) {
            let this_dependencies = this_env.dependencies(&name);
            let other_dependencies = other_env.dependencies(&name);
            // A dependency-only equation can represent this join only when
            // every reachable branch contributes a constant term. A literal,
            // parameter, unsupported Unknown, or implicit nil branch would
            // require a concrete base term; drop the equation rather than
            // publishing the known constant members as a partial union.
            if this_dependencies.is_empty() || other_dependencies.is_empty() {
                continue;
            }
            let dependencies = this_dependencies
                .into_iter()
                .chain(other_dependencies)
                .collect::<BTreeSet<_>>();
            if !dependencies.is_empty() {
                merged_dependencies.insert(name, dependencies);
            }
        }
        self.vars.constant_dependencies = merged_dependencies;

        // Merge abstract Hash identity state independently from displayed
        // local types. The same identity can be mutated differently on two
        // reachable branches; its joined state must then update every alias,
        // preserving complete correlated variants rather than field-wise
        // compression.
        let identity_keys = this_env
            .shape_states
            .keys()
            .chain(other_env.shape_states.keys())
            .copied()
            .collect::<BTreeSet<_>>();
        let mut merged_states = HashMap::new();
        for identity in identity_keys {
            let state = match (
                this_env.shape_states.get(&identity),
                other_env.shape_states.get(&identity),
            ) {
                (
                    Some(ShapeIdentityState::Invalidated(reason)),
                    Some(ShapeIdentityState::Invalidated(other_reason)),
                ) => ShapeIdentityState::Invalidated(if reason == other_reason {
                    *reason
                } else if *reason == UnknownReason::ShapeBoundExceeded
                    || *other_reason == UnknownReason::ShapeBoundExceeded
                {
                    UnknownReason::ShapeBoundExceeded
                } else {
                    UnknownReason::MutableShapeInvalidated
                }),
                (Some(ShapeIdentityState::Invalidated(reason)), Some(_))
                | (Some(_), Some(ShapeIdentityState::Invalidated(reason))) => {
                    ShapeIdentityState::Invalidated(*reason)
                }
                (
                    Some(ShapeIdentityState::Proven(left)),
                    Some(ShapeIdentityState::Proven(right)),
                ) => {
                    let joined = RubyType::union([left.clone(), right.clone()]);
                    if joined == RubyType::Unknown {
                        ShapeIdentityState::Invalidated(UnknownReason::ShapeBoundExceeded)
                    } else {
                        ShapeIdentityState::Proven(joined)
                    }
                }
                (Some(state), None) | (None, Some(state)) => state.clone(),
                (None, None) => panic!(
                    "INVARIANT VIOLATED: merged shape identity {:?} is absent from both branch environments. This is a bug because identity_keys is derived from those exact maps. Fix: keep key collection and state lookup in one immutable merge.",
                    identity
                ),
            };
            merged_states.insert(identity, state);
        }

        let binding_names = this_env
            .shape_bindings
            .keys()
            .chain(other_env.shape_bindings.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut merged_bindings = HashMap::new();
        for name in binding_names {
            let identities = this_env
                .shape_identities(&name)
                .into_iter()
                .chain(other_env.shape_identities(&name))
                .collect::<BTreeSet<_>>();
            if !identities.is_empty() {
                merged_bindings.insert(name, identities);
            }
        }

        let mut merged_unknown_reasons = HashMap::new();
        for name in this_env
            .unknown_reasons
            .keys()
            .chain(other_env.unknown_reasons.keys())
            .cloned()
            .collect::<BTreeSet<_>>()
        {
            if self.vars.types.get(&name) != Some(&RubyType::Unknown) {
                continue;
            }
            let reason = match (
                this_env.unknown_reason(&name),
                other_env.unknown_reason(&name),
            ) {
                (Some(left), Some(right)) if left == right => left,
                (Some(UnknownReason::ShapeBoundExceeded), _)
                | (_, Some(UnknownReason::ShapeBoundExceeded)) => UnknownReason::ShapeBoundExceeded,
                (Some(UnknownReason::MutableShapeInvalidated), _)
                | (_, Some(UnknownReason::MutableShapeInvalidated)) => {
                    UnknownReason::MutableShapeInvalidated
                }
                (Some(reason), None) | (None, Some(reason)) => reason,
                (Some(_), Some(_)) | (None, None) => UnknownReason::UnresolvedAssignmentValue,
            };
            merged_unknown_reasons.insert(name, reason);
        }

        let merged_containments = this_env
            .shape_containments
            .intersection(&other_env.shape_containments)
            .cloned()
            .collect::<BTreeSet<_>>();
        for link in this_env
            .shape_containments
            .symmetric_difference(&other_env.shape_containments)
        {
            for identity in [link.parent, link.child] {
                if merged_states.contains_key(&identity) {
                    merged_states.insert(
                        identity,
                        ShapeIdentityState::Invalidated(UnknownReason::MutableShapeInvalidated),
                    );
                }
            }
        }
        let array_names = this_env
            .array_shape_aliases
            .keys()
            .chain(other_env.array_shape_aliases.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut merged_array_shape_aliases = HashMap::new();
        let mut inconsistent_array_identities = BTreeSet::new();
        for name in array_names {
            match (
                this_env.array_shape_aliases.get(&name),
                other_env.array_shape_aliases.get(&name),
            ) {
                (Some(left), Some(right)) if left == right => {
                    merged_array_shape_aliases.insert(name, left.clone());
                }
                (Some(left), Some(right)) => {
                    inconsistent_array_identities.extend(&left.contained);
                    inconsistent_array_identities.extend(&right.contained);
                }
                (Some(aliases), None) | (None, Some(aliases)) => {
                    inconsistent_array_identities.extend(&aliases.contained);
                }
                (None, None) => panic!(
                    "INVARIANT VIOLATED: merged Array alias name `{name}` is absent from both branch environments. This is a bug because array_names is derived from those exact maps. Fix: keep key collection and lookup in one immutable merge."
                ),
            }
        }
        self.vars.shape_states = merged_states;
        self.vars.shape_bindings = merged_bindings;
        self.vars.shape_containments = merged_containments;
        self.vars.array_shape_aliases = merged_array_shape_aliases;
        self.vars.unknown_reasons = merged_unknown_reasons;
        self.vars.max_live_shape_aliases = this_env
            .max_live_shape_aliases
            .max(other_env.max_live_shape_aliases);
        let callable_names = this_env
            .callables
            .keys()
            .chain(other_env.callables.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut merged_callables = HashMap::new();
        for name in callable_names {
            let merged = match (
                this_env.callables.get(&name),
                other_env.callables.get(&name),
            ) {
                (Some(left), Some(right)) if left == right => left.clone(),
                (Some(left), Some(right)) => crate::inference::higher_order::KnownProcType {
                    identity: left.identity.min(right.identity),
                    summary: Err(UnknownReason::AmbiguousCallableValue),
                },
                (Some(callable), None) | (None, Some(callable)) => {
                    crate::inference::higher_order::KnownProcType {
                        identity: callable.identity,
                        summary: Err(UnknownReason::AmbiguousCallableValue),
                    }
                }
                (None, None) => panic!(
                    "INVARIANT VIOLATED: callable merge key `{name}` is absent from both branch environments. This is a bug because keys are derived from those exact maps. Fix: keep callable key collection and lookup atomic."
                ),
            };
            merged_callables.insert(name, merged);
        }
        self.vars.callables = merged_callables;
        if !inconsistent_array_identities.is_empty() {
            self.vars.invalidate_identities(
                &inconsistent_array_identities,
                UnknownReason::MutableShapeInvalidated,
            );
        }
        self.vars.enforce_alias_bound();
        self.vars.synchronize_shape_aliases();
    }
}

fn type_contains_shape(ruby_type: &RubyType) -> bool {
    match ruby_type {
        RubyType::Shape(_) => true,
        RubyType::Union(members) => members.iter().any(type_contains_shape),
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Literal(_)
        | RubyType::Array(_)
        | RubyType::Hash(_, _)
        | RubyType::Unknown => false,
    }
}

fn type_is_shape_only(ruby_type: &RubyType) -> bool {
    match ruby_type {
        RubyType::Shape(_) => true,
        RubyType::Union(members) => !members.is_empty() && members.iter().all(type_is_shape_only),
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Literal(_)
        | RubyType::Array(_)
        | RubyType::Hash(_, _)
        | RubyType::Unknown => false,
    }
}

fn shape_alternatives(ruby_type: &RubyType) -> Result<Vec<RubyType>, UnknownReason> {
    match ruby_type {
        RubyType::Shape(_) => Ok(vec![ruby_type.clone()]),
        RubyType::Union(members) => {
            let mut shapes = Vec::new();
            for member in members {
                shapes.extend(shape_alternatives(member)?);
            }
            Ok(shapes)
        }
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Literal(_)
        | RubyType::Array(_)
        | RubyType::Hash(_, _)
        | RubyType::Unknown => Err(UnknownReason::MutableShapeInvalidated),
    }
}

fn non_shape_alternatives(ruby_type: &RubyType) -> Vec<RubyType> {
    match ruby_type {
        RubyType::Shape(_) => Vec::new(),
        RubyType::Union(members) => members.iter().flat_map(non_shape_alternatives).collect(),
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Literal(_)
        | RubyType::Array(_)
        | RubyType::Hash(_, _)
        | RubyType::Unknown => vec![ruby_type.clone()],
    }
}

fn map_shape_alternatives(
    ruby_type: &RubyType,
    mut transform: impl FnMut(&ShapeType) -> Result<ShapeType, UnknownReason>,
) -> Result<RubyType, UnknownReason> {
    let mut transformed = Vec::new();
    for alternative in shape_alternatives(ruby_type)? {
        let RubyType::Shape(shape) = alternative else {
            panic!(
                "INVARIANT VIOLATED: shape_alternatives returned non-shape type `{alternative}`. This is a bug because callers rely on exhaustive shape-only mapping. Fix: keep shape_alternatives filtering explicit."
            );
        };
        transformed.push(RubyType::Shape(Box::new(transform(&shape)?)));
    }
    let joined = RubyType::union(transformed);
    (joined != RubyType::Unknown)
        .then_some(joined)
        .ok_or(UnknownReason::ShapeBoundExceeded)
}

fn rebuild_shape(
    shape: &ShapeType,
    fields: impl IntoIterator<Item = ShapeField>,
    stability: ShapeStability,
) -> Result<ShapeType, UnknownReason> {
    ShapeType::try_new(fields, shape.rest().cloned(), shape.exactness(), stability)
        .map_err(|_| UnknownReason::ShapeBoundExceeded)
}

fn shape_with_required_field(
    ruby_type: &RubyType,
    key: LiteralKey,
    value: RubyType,
) -> Result<RubyType, UnknownReason> {
    map_shape_alternatives(ruby_type, |shape| {
        if shape.is_frozen() {
            return Ok(shape.clone());
        }
        let mut fields = shape
            .fields()
            .iter()
            .map(|field| (field.key().clone(), field.clone()))
            .collect::<BTreeMap<_, _>>();
        fields.insert(
            key.clone(),
            ShapeField::required(key.clone(), value.clone()),
        );
        rebuild_shape(shape, fields.into_values(), shape.stability())
    })
}

fn shape_with_contained_transitions(
    ruby_type: &RubyType,
    key: &LiteralKey,
    transitions: &[ShapeAlternativeTransition],
) -> Result<RubyType, UnknownReason> {
    let RubyType::Shape(shape) = ruby_type else {
        return Err(UnknownReason::MutableShapeInvalidated);
    };
    let Some(existing) = shape.field(key) else {
        return Err(UnknownReason::MutableShapeInvalidated);
    };
    if !existing.is_required() || !type_is_shape_only(existing.value()) {
        return Err(UnknownReason::MutableShapeInvalidated);
    }
    let mut transitioned_values = Vec::new();
    for before in shape_alternatives(existing.value())? {
        let mut matches = transitions
            .iter()
            .filter(|transition| transition.before == before);
        let transition = matches
            .next()
            .ok_or(UnknownReason::MutableShapeInvalidated)?;
        if matches.next().is_some() {
            return Err(UnknownReason::MutableShapeInvalidated);
        }
        transitioned_values.push(transition.after.clone());
    }
    let value = RubyType::union(transitioned_values);
    if value == RubyType::Unknown {
        return Err(UnknownReason::ShapeBoundExceeded);
    }
    let fields = shape.fields().iter().map(|field| {
        if field.key() == key {
            ShapeField::required(key.clone(), value.clone())
        } else {
            field.clone()
        }
    });
    let rebuilt = rebuild_shape(shape, fields, shape.stability())?;
    Ok(RubyType::Shape(Box::new(rebuilt)))
}

fn shape_without_field(ruby_type: &RubyType, key: &LiteralKey) -> Result<RubyType, UnknownReason> {
    map_shape_alternatives(ruby_type, |shape| {
        if shape.is_frozen() {
            return Ok(shape.clone());
        }
        let fields = shape
            .fields()
            .iter()
            .filter(|field| field.key() != key)
            .cloned();
        rebuild_shape(shape, fields, shape.stability())
    })
}

fn shape_cleared(ruby_type: &RubyType) -> Result<RubyType, UnknownReason> {
    map_shape_alternatives(ruby_type, |shape| {
        if shape.is_frozen() {
            return Ok(shape.clone());
        }
        ShapeType::try_new(
            std::iter::empty(),
            None,
            ShapeExactness::Exact,
            ShapeStability::TrackedMutable,
        )
        .map_err(|_| UnknownReason::ShapeBoundExceeded)
    })
}

fn shape_frozen(ruby_type: &RubyType) -> Result<RubyType, UnknownReason> {
    map_shape_alternatives(ruby_type, |shape| {
        rebuild_shape(
            shape,
            shape.fields().iter().cloned(),
            ShapeStability::Frozen,
        )
    })
}

fn merge_shape_types(
    left: &RubyType,
    right: &RubyType,
    preserve_left_stability: bool,
) -> Result<RubyType, UnknownReason> {
    let left_alternatives = shape_alternatives(left)?;
    let right_alternatives = shape_alternatives(right)?;
    let mut merged = Vec::new();
    for left in &left_alternatives {
        let RubyType::Shape(left_shape) = left else {
            panic!(
                "INVARIANT VIOLATED: left shape alternative is not a Shape. This is a bug because merge_shape_types consumes shape_alternatives. Fix: keep the helper return contract exhaustive."
            );
        };
        for right in &right_alternatives {
            let RubyType::Shape(right_shape) = right else {
                panic!(
                    "INVARIANT VIOLATED: right shape alternative is not a Shape. This is a bug because merge_shape_types consumes shape_alternatives. Fix: keep the helper return contract exhaustive."
                );
            };
            if !left_shape.is_exact()
                || left_shape.rest().is_some()
                || !right_shape.is_exact()
                || right_shape.rest().is_some()
            {
                return Err(UnknownReason::MutableShapeInvalidated);
            }
            if preserve_left_stability && left_shape.is_frozen() {
                merged.push(left.clone());
                continue;
            }
            let mut fields = left_shape
                .fields()
                .iter()
                .map(|field| (field.key().clone(), field.clone()))
                .collect::<BTreeMap<_, _>>();
            for field in right_shape.fields() {
                fields.insert(field.key().clone(), field.clone());
            }
            let stability = if preserve_left_stability {
                left_shape.stability()
            } else {
                ShapeStability::TrackedMutable
            };
            let shape =
                ShapeType::try_new(fields.into_values(), None, ShapeExactness::Exact, stability)
                    .map_err(|_| UnknownReason::ShapeBoundExceeded)?;
            merged.push(RubyType::Shape(Box::new(shape)));
        }
    }
    let joined = RubyType::union(merged);
    (joined != RubyType::Unknown)
        .then_some(joined)
        .ok_or(UnknownReason::ShapeBoundExceeded)
}

fn shape_literal_read_type(ruby_type: &RubyType, key: &LiteralKey) -> RubyType {
    let resolved = RubyType::union_from_proven(
        shape_alternatives(ruby_type).unwrap_or_default(),
        |alternative| {
            let RubyType::Shape(shape) = alternative else {
                return None;
            };
            match shape.field(key) {
                Some(field) if field.is_required() => Some(field.value().clone()),
                Some(field) => Some(RubyType::optional(field.value().clone())),
                None if shape.is_exact() => Some(RubyType::nil_class()),
                None => None,
            }
        },
    );
    resolved.unwrap_or(RubyType::Unknown)
}

fn shape_field_keys(ruby_type: &RubyType) -> Result<BTreeSet<LiteralKey>, UnknownReason> {
    let mut keys = BTreeSet::new();
    for alternative in shape_alternatives(ruby_type)? {
        let RubyType::Shape(shape) = alternative else {
            panic!(
                "INVARIANT VIOLATED: shape_alternatives returned non-shape type `{alternative}`. This is a bug because merge mutation key discovery accepts only shapes. Fix: keep shape_alternatives exhaustive."
            );
        };
        keys.extend(shape.fields().iter().map(|field| field.key().clone()));
    }
    Ok(keys)
}

fn literal_array_index(node: &Node<'_>) -> Option<i32> {
    let integer = node.as_integer_node()?;
    integer.value().try_into().ok()
}

fn literal_hash_keys(hash: &HashNode<'_>) -> Option<BTreeSet<LiteralKey>> {
    let mut keys = BTreeSet::new();
    for element in hash.elements().iter() {
        let assoc = element.as_assoc_node()?;
        keys.insert(literal_key(&assoc.key())?);
    }
    Some(keys)
}

fn precise_contained_shape_field(ruby_type: &RubyType, key: &LiteralKey) -> Option<RubyType> {
    let RubyType::Shape(shape) = ruby_type else {
        // A union would need variant-specific containment identities to avoid
        // destroying correlations when the child mutates. Keep that boundary
        // fail-closed until the flow graph can represent those identities.
        return None;
    };
    let field = shape.field(key)?;
    if !field.is_required() || !type_is_shape_only(field.value()) {
        return None;
    }
    Some(field.value().clone())
}

fn shape_local_key_path(node: &Node<'_>) -> Option<(String, Vec<LiteralKey>)> {
    if let Some(parentheses) = node.as_parentheses_node() {
        return parentheses
            .body()
            .and_then(|body| shape_local_key_path(&body));
    }
    let call = node.as_call_node()?;
    let receiver = call.receiver()?;
    let (name, mut path) = if let Some(local) = receiver.as_local_variable_read_node() {
        (
            String::from_utf8_lossy(local.name().as_slice()).to_string(),
            Vec::new(),
        )
    } else {
        shape_local_key_path(&receiver)?
    };
    let arguments = call.arguments()?;
    let argument_nodes = arguments.arguments().iter().collect::<Vec<_>>();
    match call.name().as_slice() {
        b"[]" | b"fetch" if argument_nodes.len() == 1 => {
            path.push(literal_key(&argument_nodes[0])?);
        }
        b"dig" if !argument_nodes.is_empty() => {
            for argument in argument_nodes {
                path.push(literal_key(&argument)?);
            }
        }
        _ => return None,
    }
    Some((name, path))
}

fn shape_local_key_read(node: &Node<'_>) -> Option<(String, LiteralKey)> {
    let call = node.as_call_node()?;
    if call.name().as_slice() != b"[]" {
        return None;
    }
    let receiver = call.receiver()?.as_local_variable_read_node()?;
    let arguments = call.arguments()?;
    let argument_nodes = arguments.arguments().iter().collect::<Vec<_>>();
    if argument_nodes.len() != 1 {
        return None;
    }
    Some((
        String::from_utf8_lossy(receiver.name().as_slice()).to_string(),
        literal_key(&argument_nodes[0])?,
    ))
}

fn literal_value(node: &Node<'_>) -> Option<LiteralValue> {
    if let Some(symbol) = node.as_symbol_node() {
        return Some(LiteralValue::symbol(
            String::from_utf8_lossy(symbol.unescaped()).to_string(),
        ));
    }
    node.as_string_node()
        .map(|string| LiteralValue::string(String::from_utf8_lossy(string.unescaped()).to_string()))
}

fn narrow_shape_literal_type(
    ruby_type: &RubyType,
    key: &LiteralKey,
    literal: &LiteralValue,
    require_match: bool,
) -> Result<Option<RubyType>, UnknownReason> {
    let mut retained = Vec::new();
    for alternative in shape_alternatives(ruby_type)? {
        let RubyType::Shape(shape) = alternative else {
            panic!(
                "INVARIANT VIOLATED: discriminator narrowing received a non-shape alternative. This is a bug because shape_alternatives guarantees Shape members. Fix: keep discriminator inputs restricted to complete shape identities."
            );
        };
        let relationship = shape_literal_relationship(&shape, key, literal);
        let keep = match (relationship, require_match) {
            (ShapePredicateMatch::Matches, true)
            | (ShapePredicateMatch::DoesNotMatch, false)
            | (ShapePredicateMatch::Inconclusive, true)
            | (ShapePredicateMatch::Inconclusive, false) => true,
            (ShapePredicateMatch::Matches, false) | (ShapePredicateMatch::DoesNotMatch, true) => {
                false
            }
        };
        if keep {
            retained.push(RubyType::Shape(shape));
        }
    }
    if retained.is_empty() {
        return Ok(None);
    }
    let narrowed = RubyType::union(retained);
    (narrowed != RubyType::Unknown)
        .then_some(Some(narrowed))
        .ok_or(UnknownReason::ShapeBoundExceeded)
}

fn narrow_shape_literal_set_type(
    ruby_type: &RubyType,
    key: &LiteralKey,
    literals: &[LiteralValue],
    require_match: bool,
) -> Result<Option<RubyType>, UnknownReason> {
    let mut retained = Vec::new();
    for alternative in shape_alternatives(ruby_type)? {
        let RubyType::Shape(shape) = alternative else {
            panic!(
                "INVARIANT VIOLATED: case discriminator narrowing received a non-shape alternative. This is a bug because shape_alternatives guarantees Shape members. Fix: keep case narrowing inputs restricted to complete shape identities."
            );
        };
        let relationships = literals
            .iter()
            .map(|literal| shape_literal_relationship(&shape, key, literal))
            .collect::<Vec<_>>();
        let relationship = if relationships
            .iter()
            .any(|candidate| *candidate == ShapePredicateMatch::Matches)
        {
            ShapePredicateMatch::Matches
        } else if relationships
            .iter()
            .all(|candidate| *candidate == ShapePredicateMatch::DoesNotMatch)
        {
            ShapePredicateMatch::DoesNotMatch
        } else {
            ShapePredicateMatch::Inconclusive
        };
        let keep = match (relationship, require_match) {
            (ShapePredicateMatch::Matches, true)
            | (ShapePredicateMatch::DoesNotMatch, false)
            | (ShapePredicateMatch::Inconclusive, true)
            | (ShapePredicateMatch::Inconclusive, false) => true,
            (ShapePredicateMatch::Matches, false) | (ShapePredicateMatch::DoesNotMatch, true) => {
                false
            }
        };
        if keep {
            retained.push(RubyType::Shape(shape));
        }
    }
    if retained.is_empty() {
        return Ok(None);
    }
    let narrowed = RubyType::union(retained);
    (narrowed != RubyType::Unknown)
        .then_some(Some(narrowed))
        .ok_or(UnknownReason::ShapeBoundExceeded)
}

fn shape_literal_relationship(
    shape: &ShapeType,
    key: &LiteralKey,
    literal: &LiteralValue,
) -> ShapePredicateMatch {
    let Some(field) = shape.field(key) else {
        return if shape.is_exact() {
            ShapePredicateMatch::DoesNotMatch
        } else {
            ShapePredicateMatch::Inconclusive
        };
    };
    if !field.is_required() {
        return ShapePredicateMatch::Inconclusive;
    }
    ruby_type_literal_relationship(field.value(), literal)
}

fn ruby_type_literal_relationship(
    ruby_type: &RubyType,
    literal: &LiteralValue,
) -> ShapePredicateMatch {
    match ruby_type {
        RubyType::Literal(value) => {
            if value.as_ref() == literal {
                ShapePredicateMatch::Matches
            } else {
                ShapePredicateMatch::DoesNotMatch
            }
        }
        RubyType::Union(members) => {
            let relationships = members
                .iter()
                .map(|member| ruby_type_literal_relationship(member, literal))
                .collect::<Vec<_>>();
            if relationships
                .iter()
                .all(|relationship| *relationship == ShapePredicateMatch::Matches)
            {
                ShapePredicateMatch::Matches
            } else if relationships
                .iter()
                .all(|relationship| *relationship == ShapePredicateMatch::DoesNotMatch)
            {
                ShapePredicateMatch::DoesNotMatch
            } else {
                ShapePredicateMatch::Inconclusive
            }
        }
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Array(_)
        | RubyType::Hash(_, _)
        | RubyType::Shape(_)
        | RubyType::Unknown => ShapePredicateMatch::Inconclusive,
    }
}

fn hash_pattern_requirements(
    pattern: &Node<'_>,
) -> Option<Vec<(LiteralKey, Option<LiteralValue>)>> {
    let hash = pattern.as_hash_pattern_node()?;
    let mut requirements = Vec::new();
    for element in hash.elements().iter() {
        let assoc = element.as_assoc_node()?;
        let key = literal_key(&assoc.key())?;
        let value = assoc.value();
        let literal = literal_value(&value);
        let supported_value = literal.is_some()
            || value.as_local_variable_target_node().is_some()
            || value
                .as_implicit_node()
                .is_some_and(|implicit| implicit.value().as_local_variable_target_node().is_some());
        if !supported_value {
            return None;
        }
        requirements.push((key, literal));
    }
    Some(requirements)
}

fn narrow_shape_pattern_type(
    ruby_type: &RubyType,
    requirements: &[(LiteralKey, Option<LiteralValue>)],
    require_match: bool,
) -> Result<Option<RubyType>, UnknownReason> {
    let mut retained = Vec::new();
    for alternative in shape_alternatives(ruby_type)? {
        let RubyType::Shape(shape) = alternative else {
            panic!(
                "INVARIANT VIOLATED: Hash pattern narrowing received a non-shape alternative. This is a bug because shape_alternatives guarantees Shape members. Fix: keep pattern inputs restricted to complete shape identities."
            );
        };
        let relationships = requirements
            .iter()
            .map(|(key, literal)| match (shape.field(key), literal) {
                (None, _) if shape.is_exact() => ShapePredicateMatch::DoesNotMatch,
                (None, _) => ShapePredicateMatch::Inconclusive,
                (Some(field), _) if !field.is_required() => ShapePredicateMatch::Inconclusive,
                (Some(_), None) => ShapePredicateMatch::Matches,
                (Some(field), Some(literal)) => {
                    ruby_type_literal_relationship(field.value(), literal)
                }
            })
            .collect::<Vec<_>>();
        let relationship = if relationships
            .iter()
            .any(|candidate| *candidate == ShapePredicateMatch::DoesNotMatch)
        {
            ShapePredicateMatch::DoesNotMatch
        } else if relationships
            .iter()
            .all(|candidate| *candidate == ShapePredicateMatch::Matches)
        {
            ShapePredicateMatch::Matches
        } else {
            ShapePredicateMatch::Inconclusive
        };
        let keep = match (relationship, require_match) {
            (ShapePredicateMatch::Matches, true)
            | (ShapePredicateMatch::DoesNotMatch, false)
            | (ShapePredicateMatch::Inconclusive, true)
            | (ShapePredicateMatch::Inconclusive, false) => true,
            (ShapePredicateMatch::Matches, false) | (ShapePredicateMatch::DoesNotMatch, true) => {
                false
            }
        };
        if keep {
            retained.push(RubyType::Shape(shape));
        }
    }
    if retained.is_empty() {
        return Ok(None);
    }
    let narrowed = RubyType::union(retained);
    (narrowed != RubyType::Unknown)
        .then_some(Some(narrowed))
        .ok_or(UnknownReason::ShapeBoundExceeded)
}

fn narrow_shape_presence_type(
    ruby_type: &RubyType,
    key: &LiteralKey,
    truth: bool,
) -> Result<Option<RubyType>, UnknownReason> {
    let mut retained = Vec::new();
    for alternative in shape_alternatives(ruby_type)? {
        let RubyType::Shape(shape) = alternative else {
            panic!(
                "INVARIANT VIOLATED: presence narrowing received a non-shape alternative. This is a bug because shape_alternatives guarantees Shape members. Fix: keep the narrowing input filter exhaustive."
            );
        };
        match (shape.field(key), truth) {
            (Some(field), true) if field.is_required() => {
                retained.push(RubyType::Shape(shape));
            }
            (Some(field), true) => {
                let mut fields = shape
                    .fields()
                    .iter()
                    .map(|field| (field.key().clone(), field.clone()))
                    .collect::<BTreeMap<_, _>>();
                fields.insert(
                    key.clone(),
                    ShapeField::required(key.clone(), field.value().clone()),
                );
                retained.push(RubyType::Shape(Box::new(rebuild_shape(
                    &shape,
                    fields.into_values(),
                    shape.stability(),
                )?)));
            }
            (Some(field), false) if field.is_required() => {}
            (Some(_), false) => {
                retained.push(shape_without_field(&RubyType::Shape(shape), key)?);
            }
            (None, true) if shape.is_exact() => {}
            (None, false) if shape.is_exact() => {
                retained.push(RubyType::Shape(shape));
            }
            (None, true) => {
                if let Some(rest) = shape.rest() {
                    if key.generic_type().is_subtype_of(rest.key()) {
                        let mut fields = shape
                            .fields()
                            .iter()
                            .map(|field| (field.key().clone(), field.clone()))
                            .collect::<BTreeMap<_, _>>();
                        fields.insert(
                            key.clone(),
                            ShapeField::required(key.clone(), rest.value().clone()),
                        );
                        retained.push(RubyType::Shape(Box::new(rebuild_shape(
                            &shape,
                            fields.into_values(),
                            shape.stability(),
                        )?)));
                    }
                } else {
                    // An open shape without a rest contract proves only that
                    // the branch can be reached, not the new field's value.
                    retained.push(RubyType::Shape(shape));
                }
            }
            (None, false) => retained.push(RubyType::Shape(shape)),
        }
    }
    if retained.is_empty() {
        return Ok(None);
    }
    let joined = RubyType::union(retained);
    if joined == RubyType::Unknown {
        return Err(UnknownReason::ShapeBoundExceeded);
    }
    Ok(Some(joined))
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
        | RubyType::Literal(_)
        | RubyType::Array(_)
        | RubyType::Hash(_, _)
        | RubyType::Shape(_) => Truthiness::AlwaysTruthy,
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
    branches: &mut Vec<(FlowEnvironment, RubyType, bool)>,
    env_before: &FlowEnvironment,
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

#[derive(Default)]
struct EscapedCallableReadCollector {
    names: HashSet<String>,
}

impl<'pr> Visit<'pr> for EscapedCallableReadCollector {
    fn visit_local_variable_read_node(&mut self, node: &LocalVariableReadNode<'pr>) {
        self.names
            .insert(String::from_utf8_lossy(node.name().as_slice()).to_string());
    }

    fn visit_call_node(&mut self, node: &CallNode<'pr>) {
        if let Some(receiver) = node.receiver() {
            let direct_invoke = node.name().as_slice() == b"call"
                && receiver.as_local_variable_read_node().is_some();
            if !direct_invoke {
                self.visit(&receiver);
            }
        }
        if let Some(arguments) = node.arguments() {
            self.visit_arguments_node(&arguments);
        }
        if node
            .block()
            .is_some_and(|block| block.as_block_node().is_some())
        {
            self.visit(node.block().as_ref().expect("checked block presence"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::ShapeRest;

    fn create_test_tracker<'a>(source: &'a str) -> TypeTracker<'a> {
        TypeTracker::new(source.as_bytes())
    }

    fn instance_type(name: &str) -> RubyType {
        RubyType::Class(FullyQualifiedName::constant(vec![
            RubyConstant::new(name).expect("test class name must be a valid Ruby constant")
        ]))
    }

    fn tracked_method_type(source: &str) -> RubyType {
        let parse_result = ruby_prism::parse(source.as_bytes());
        let definition = parse_result
            .node()
            .as_program_node()
            .expect("test source must parse as a program")
            .statements()
            .body()
            .iter()
            .next()
            .expect("test source must contain a method")
            .as_def_node()
            .expect("test source must begin with a method definition");
        TypeTracker::new(source.as_bytes()).track_method(&definition)
    }

    fn exact_local_read_type(
        tracker: &mut TypeTracker<'_>,
        source: &str,
        needle: &str,
    ) -> RubyType {
        exact_local_read(tracker, source, needle).ruby_type
    }

    fn exact_local_read(
        tracker: &mut TypeTracker<'_>,
        source: &str,
        needle: &str,
    ) -> LocalReadType {
        let start_offset = source.rfind(needle).expect(
            "INVARIANT VIOLATED: the test local-read needle is absent. This is a bug because the fixture and assertion must identify the same source token. Fix: keep the needle synchronized with the fixture.",
        );
        tracker
            .take_local_read_types()
            .into_iter()
            .find(|read| read.start_offset == start_offset)
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
    fn local_hash_shape_recursively_uses_proven_local_values() {
        let source = r#"def build
  label = "ready"
  { payload: { label: label, count: 1 } }
end"#;

        assert_eq!(
            tracked_method_type(source).to_string(),
            "{ payload: { count: Integer, label: String } }"
        );
    }

    #[test]
    fn if_join_preserves_correlated_shape_variants() {
        let source = r#"def build(condition)
  if condition
    { kind: :number, value: 1 }
  else
    { kind: :text, value: "ready" }
  end
end"#;

        assert_eq!(
            tracked_method_type(source).to_string(),
            "({ kind: :number, value: Integer } | { kind: :text, value: String })"
        );
    }

    #[test]
    fn missing_shape_branch_contributes_implicit_nil() {
        let source = r#"def build(condition)
  if condition
    { state: :ready }
  end
end"#;

        assert_eq!(
            tracked_method_type(source).to_string(),
            "(NilClass | { state: :ready })"
        );
    }

    #[test]
    fn missing_shape_branch_preserves_the_prior_value() {
        let source = r#"def build(condition)
  result = { state: :waiting, value: "cached" }
  if condition
    result = { state: :ready, value: 1 }
  end
  result
end"#;

        assert_eq!(
            tracked_method_type(source).to_string(),
            "({ state: :ready, value: Integer } | { state: :waiting, value: String })"
        );
    }

    #[test]
    fn diverging_shape_branch_does_not_reach_join() {
        let source = r#"def build(condition)
  if condition
    { state: :ready }
  else
    raise "failed"
  end
end"#;

        assert_eq!(tracked_method_type(source).to_string(), "{ state: :ready }");
    }

    #[test]
    fn unless_join_preserves_correlated_shape_variants() {
        let source = r#"def build(condition)
  unless condition
    { kind: :offline, value: "cached" }
  else
    { kind: :online, value: 1 }
  end
end"#;

        assert_eq!(
            tracked_method_type(source).to_string(),
            "({ kind: :offline, value: String } | { kind: :online, value: Integer })"
        );
    }

    #[test]
    fn case_join_preserves_shape_variants_and_unmatched_nil() {
        let source = r#"def build(mode)
  case mode
  when :number
    { kind: :number, value: 1 }
  when :text
    { kind: :text, value: "ready" }
  end
end"#;

        assert_eq!(
            tracked_method_type(source).to_string(),
            "(NilClass | { kind: :number, value: Integer } | { kind: :text, value: String })"
        );
    }

    #[test]
    fn local_shape_splat_uses_ruby_overwrite_order() {
        let source = r#"def build
  base = { state: :waiting, count: 1 }
  { before: true, **base, state: :ready }
end"#;

        assert_eq!(
            tracked_method_type(source).to_string(),
            "{ before: TrueClass, count: Integer, state: :ready }"
        );
    }

    #[test]
    fn arrays_recursively_retain_local_shape_evidence() {
        let source = r#"def build
  label = "ready"
  [{ label: label }]
end"#;

        assert_eq!(
            tracked_method_type(source).to_string(),
            "Array<{ label: String }>"
        );
    }

    #[test]
    fn local_literal_key_read_uses_the_valid_shape_state() {
        let source = r#"def read
  payload = { count: 1 }
  payload[:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::integer());
    }

    #[test]
    fn keyed_reads_observe_alias_mutation_and_invalidation() {
        let mutated = r#"def read
  payload = { count: 1 }
  copy = payload
  copy[:count] = "many"
  payload[:count]
end"#;
        assert_eq!(tracked_method_type(mutated), RubyType::string());

        let invalidated = r#"def read
  payload = { count: 1 }
  dynamic_sink(payload)
  payload[:count]
end"#;
        assert_eq!(tracked_method_type(invalidated), RubyType::Unknown);
    }

    #[test]
    fn absent_and_dynamic_shape_reads_include_nil() {
        let absent = r#"def read
  payload = { count: 1 }
  payload[:missing]
end"#;
        assert_eq!(tracked_method_type(absent), RubyType::nil_class());

        let dynamic = r#"def read(key)
  payload = { count: 1, label: "ready" }
  payload[key]
end"#;
        assert_eq!(
            tracked_method_type(dynamic),
            RubyType::union([
                RubyType::integer(),
                RubyType::nil_class(),
                RubyType::string(),
            ])
        );
    }

    #[test]
    fn fetch_and_dig_use_complete_shape_evidence() {
        let fetch_source = r#"def read
  payload = { count: 1 }
  payload.fetch(:missing, "fallback")
end"#;
        assert_eq!(tracked_method_type(fetch_source), RubyType::string());

        let dig_source = r#"def read
  payload = { user: { profile: { name: "Ada" } } }
  payload.dig(:user, :profile, :name)
end"#;
        assert_eq!(tracked_method_type(dig_source), RubyType::string());
    }

    #[test]
    fn keys_values_and_each_project_the_current_shape() {
        let keys_source = r#"def read
  payload = { count: 1, label: "ready" }
  payload.keys
end"#;
        assert_eq!(
            tracked_method_type(keys_source),
            RubyType::Array(vec![RubyType::symbol()])
        );

        let values_source = r#"def read
  payload = { count: 1, label: "ready" }
  payload.values
end"#;
        assert_eq!(
            tracked_method_type(values_source),
            RubyType::Array(vec![RubyType::integer(), RubyType::string()])
        );

        let each_source = r#"def read
  payload = { count: 1 }
  payload.each
end"#;
        assert_eq!(tracked_method_type(each_source).to_string(), "Enumerator");

        let each_with_block_source = r#"def read
  payload = { count: 1 }
  payload.each { |_key, _value| nil }
end"#;
        assert_eq!(
            tracked_method_type(each_with_block_source).to_string(),
            "{ count: Integer }"
        );
    }

    #[test]
    fn key_presence_guard_narrows_complete_shape_variants() {
        let source = r#"def read(condition)
  payload = if condition
    { count: 1 }
  else
    { label: "ready" }
  end
  if payload.key?(:count)
    payload[:count]
  else
    payload[:label]
  end
end"#;

        assert_eq!(
            tracked_method_type(source),
            RubyType::union([RubyType::integer(), RubyType::string()])
        );
    }

    #[test]
    fn literal_discriminator_narrows_correlated_shape_variants_on_both_paths() {
        let source = r#"def read(condition)
  result = if condition
    { kind: :number, value: 1 }
  else
    { kind: :text, value: "ready" }
  end
  if result[:kind] == :number
    result[:value]
  else
    result[:value]
  end
end"#;
        let parse = ruby_prism::parse(source.as_bytes());
        let definition = parse
            .node()
            .as_program_node()
            .expect("test source must parse as a program")
            .statements()
            .body()
            .iter()
            .next()
            .expect("test source must contain a method")
            .as_def_node()
            .expect("test source must begin with a method definition");
        let mut tracker = create_test_tracker(source).with_local_read_types();
        tracker.track_method(&definition);
        let reads = tracker.take_local_read_types();
        let read_type_at = |needle: &str| {
            let start_offset = source.find(needle).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: discriminator test needle `{needle}` is absent. This is a bug because the fixture and assertion must identify the same branch read. Fix: keep the needle synchronized with the source."
                )
            });
            reads
                .iter()
                .find(|read| read.start_offset == start_offset)
                .unwrap_or_else(|| {
                    panic!(
                        "INVARIANT VIOLATED: discriminator branch read at {start_offset} was not retained. This is a bug because flow evidence is enabled after an if join. Fix: record the exact local receiver read on every reachable branch."
                    )
                })
                .ruby_type
                .clone()
        };

        assert_eq!(
            read_type_at("result[:value]\n  else").to_string(),
            "{ kind: :number, value: Integer }"
        );
        assert_eq!(
            read_type_at("result[:value]\n  end").to_string(),
            "{ kind: :text, value: String }"
        );
    }

    #[test]
    fn reversed_inequality_discriminator_preserves_true_false_semantics() {
        let source = r#"def read(condition)
  result = if condition
    { kind: :number, value: 1 }
  else
    { kind: :text, value: "ready" }
  end
  if :number != result[:kind]
    result[:value]
  else
    result[:value]
  end
end"#;
        let parse = ruby_prism::parse(source.as_bytes());
        let definition = parse
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
        tracker.track_method(&definition);
        let reads = tracker.take_local_read_types();
        let read_type_at = |needle: &str| {
            let start_offset = source.find(needle).unwrap();
            reads
                .iter()
                .find(|read| read.start_offset == start_offset)
                .unwrap()
                .ruby_type
                .clone()
        };

        assert_eq!(
            read_type_at("result[:value]\n  else").to_string(),
            "{ kind: :text, value: String }"
        );
        assert_eq!(
            read_type_at("result[:value]\n  end").to_string(),
            "{ kind: :number, value: Integer }"
        );
    }

    #[test]
    fn optional_and_rest_discriminators_remain_on_both_inconclusive_paths() {
        let key = LiteralKey::symbol("kind");
        let number = LiteralValue::symbol("number");
        let optional = RubyType::Shape(Box::new(
            ShapeType::try_new(
                [ShapeField::optional(
                    key.clone(),
                    RubyType::Literal(Box::new(number.clone())),
                )],
                None,
                ShapeExactness::Open,
                ShapeStability::TrackedMutable,
            )
            .expect("test optional shape must satisfy canonical bounds"),
        ));
        let rest = RubyType::Shape(Box::new(
            ShapeType::try_new(
                [],
                Some(ShapeRest::new(RubyType::symbol(), RubyType::symbol())),
                ShapeExactness::Open,
                ShapeStability::TrackedMutable,
            )
            .expect("test rest shape must satisfy canonical bounds"),
        ));
        let variants = RubyType::union([optional.clone(), rest.clone()]);

        for require_match in [true, false] {
            assert_eq!(
                narrow_shape_literal_type(&variants, &key, &number, require_match),
                Ok(Some(variants.clone())),
                "an optional field or rest contract cannot prove either discriminator path"
            );
        }
    }

    #[test]
    fn case_literal_discriminators_narrow_each_branch_and_the_else_path() {
        let source = r#"def read(condition)
  result = if condition
    { kind: :number, value: 1 }
  else
    { kind: :text, value: "ready" }
  end
  case result[:kind]
  when :number
    result[:value]
  when :text
    result[:value]
  else
    result
  end
end"#;
        let parse = ruby_prism::parse(source.as_bytes());
        let definition = parse
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
        tracker.track_method(&definition);
        let reads = tracker.take_local_read_types();
        let read_type_at = |needle: &str| {
            let start_offset = source.find(needle).unwrap();
            reads
                .iter()
                .find(|read| read.start_offset == start_offset)
                .unwrap()
                .ruby_type
                .clone()
        };

        assert_eq!(
            read_type_at("result[:value]\n  when :text").to_string(),
            "{ kind: :number, value: Integer }"
        );
        assert_eq!(
            read_type_at("result[:value]\n  else").to_string(),
            "{ kind: :text, value: String }"
        );
    }

    #[test]
    fn hash_patterns_narrow_variants_and_bind_correlated_field_types() {
        let source = r#"def read(condition)
  result = if condition
    { kind: :number, value: 1 }
  else
    { kind: :text, value: "ready" }
  end
  case result
  in { kind: :number, value: captured }
    captured
  in { kind: :text, value: captured }
    captured
  end
end"#;
        let parse = ruby_prism::parse(source.as_bytes());
        let definition = parse
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
        tracker.track_method(&definition);
        let reads = tracker.take_local_read_types();
        let first = source.find("captured\n  in").unwrap();
        let second = source.rfind("captured\n  end").unwrap();
        assert_eq!(
            reads
                .iter()
                .find(|read| read.start_offset == first)
                .unwrap()
                .ruby_type,
            RubyType::integer()
        );
        assert_eq!(
            reads
                .iter()
                .find(|read| read.start_offset == second)
                .unwrap()
                .ruby_type,
            RubyType::string()
        );
    }

    #[test]
    fn known_alias_write_updates_every_live_shape_alias() {
        let source = r#"def build
  payload = { count: 1, state: :ready }
  copy = payload
  copy[:count] = "many"
  payload
end"#;

        let parse = ruby_prism::parse(source.as_bytes());
        let definition = parse
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
            tracker.track_method(&definition).to_string(),
            "{ count: String, state: :ready }"
        );
        assert_eq!(tracker.max_live_shape_aliases(), 2);
    }

    #[test]
    fn nested_shape_read_alias_updates_the_parent_field() {
        let source = r#"def read
  payload = { nested: { count: 1 } }
  nested = payload[:nested]
  nested[:count] = "many"
  payload[:nested][:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::string());
    }

    #[test]
    fn hash_literal_containing_a_shape_alias_observes_child_mutation() {
        let source = r#"def read
  child = { count: 1 }
  payload = { nested: child }
  child[:count] = "many"
  payload[:nested][:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::string());
    }

    #[test]
    fn replacing_a_parent_field_detaches_the_old_child_identity() {
        let source = r#"def read
  payload = { nested: { count: 1 } }
  old_child = payload[:nested]
  payload[:nested] = { count: 2 }
  old_child[:count] = "detached"
  payload[:nested][:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::integer());
    }

    #[test]
    fn frozen_outer_shape_observes_its_mutable_child_identity() {
        let source = r#"def read
  payload = { nested: { count: 1 } }
  payload.freeze
  child = payload[:nested]
  child[:count] = "many"
  payload[:nested][:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::string());
    }

    #[test]
    fn repeated_nested_reads_share_one_child_identity() {
        let source = r#"def read
  payload = { nested: { count: 1 } }
  first = payload[:nested]
  second = payload[:nested]
  second[:count] = "many"
  first[:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::string());
    }

    #[test]
    fn fetch_of_a_nested_shape_retains_the_child_identity() {
        let source = r#"def read
  payload = { nested: { count: 1 } }
  child = payload.fetch(:nested)
  child[:count] = "many"
  payload[:nested][:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::string());
    }

    #[test]
    fn dig_of_a_nested_shape_retains_every_containment_edge() {
        let source = r#"def read
  payload = { outer: { nested: { count: 1 } } }
  child = payload.dig(:outer, :nested)
  child[:count] = "many"
  payload[:outer][:nested][:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::string());
    }

    #[test]
    fn escaping_a_nested_child_invalidates_the_parent_shape_proof() {
        let source = r#"def read
  payload = { nested: { count: 1 } }
  child = payload[:nested]
  dynamic_sink(child)
  payload[:nested][:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::Unknown);
    }

    #[test]
    fn one_child_identity_updates_every_containing_parent() {
        let source = r#"def read
  child = { count: 1 }
  first = { nested: child }
  second = { nested: child }
  child[:count] = "many"
  [first[:nested][:count], second[:nested][:count]]
end"#;

        assert_eq!(
            tracked_method_type(source),
            RubyType::Array(vec![RubyType::string()])
        );
    }

    #[test]
    fn nested_child_mutation_preserves_variant_correlations_through_parent_updates() {
        let source = r#"def read(condition)
  child = { count: 1, state: :ready }
  payload = { nested: child }
  if condition
    child[:state] = :left
  else
    child[:state] = :right
  end
  child[:count] = "many"
  payload[:nested][:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::string());
    }

    #[test]
    fn assigning_a_shape_alias_into_a_parent_field_tracks_containment() {
        let source = r#"def read
  payload = { state: :empty }
  child = { count: 1 }
  payload[:nested] = child
  child[:count] = "many"
  payload[:nested][:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::string());
    }

    #[test]
    fn array_element_read_preserves_the_contained_shape_identity() {
        let source = r#"def read
  child = { count: 1 }
  items = [child]
  extracted = items.first
  extracted[:count] = "many"
  child[:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::string());
    }

    #[test]
    fn array_literal_index_read_preserves_the_contained_shape_identity() {
        let source = r#"def read
  child = { count: 1 }
  items = [child]
  extracted = items[0]
  extracted[:count] = "many"
  child[:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::string());
    }

    #[test]
    fn array_at_negative_index_preserves_the_contained_shape_identity() {
        let source = r#"def read
  first = { count: 1 }
  last = { count: 2 }
  items = [first, last]
  extracted = items.at(-1)
  extracted[:count] = "many"
  last[:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::string());
    }

    #[test]
    fn array_fetch_preserves_the_contained_shape_identity() {
        let source = r#"def read
  first = { count: 1 }
  second = { count: 2 }
  items = [first, second]
  extracted = items.fetch(1)
  extracted[:count] = "many"
  second[:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::string());
    }

    #[test]
    fn dynamic_array_index_invalidates_possible_contained_shape_aliases() {
        let source = r#"def read(index)
  first = { count: 1 }
  second = { count: 2 }
  items = [first, second]
  extracted = items[index]
  extracted[:count] = "many"
  first[:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::Unknown);
    }

    #[test]
    fn array_slice_read_invalidates_possible_contained_shape_aliases() {
        let source = r#"def read
  child = { count: 1 }
  items = [child]
  copy = items.first(1)
  extracted = copy.first
  extracted[:count] = "many"
  child[:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::Unknown);
    }

    #[test]
    fn inline_shape_array_element_receives_a_stable_identity() {
        let source = r#"def read
  items = [{ count: 1 }]
  extracted = items.first
  extracted[:count] = "many"
  items.first[:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::string());
    }

    #[test]
    fn local_array_alias_preserves_contained_shape_identities() {
        let source = r#"def read
  child = { count: 1 }
  items = [child]
  copy = items
  extracted = copy.last
  extracted[:count] = "many"
  child[:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::string());
    }

    #[test]
    fn array_to_a_preserves_contained_shape_identities() {
        let source = r#"def read
  child = { count: 1 }
  items = [child]
  copy = items.to_a
  extracted = copy.first
  extracted[:count] = "many"
  child[:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::string());
    }

    #[test]
    fn contained_shape_mutation_updates_the_array_element_projection() {
        let source = r#"def read
  child = { count: 1 }
  items = [child]
  child[:count] = "many"
  items
end"#;

        assert_eq!(
            tracked_method_type(source).to_string(),
            "Array<{ count: String }>"
        );
    }

    #[test]
    fn contained_shape_escape_invalidates_the_array_element_projection() {
        let source = r#"def read
  child = { count: 1 }
  items = [child]
  dynamic_sink(child)
  items
end"#;

        assert_eq!(
            tracked_method_type(source),
            RubyType::Array(vec![RubyType::Unknown])
        );
    }

    #[test]
    fn branch_invalidation_retains_only_the_array_constructor_and_unknown_reason() {
        let source = r#"def read(condition)
  child = { count: 1 }
  items = [child]
  if condition
    dynamic_sink(child)
  end
  items
end"#;
        let parse = ruby_prism::parse(source.as_bytes());
        let definition = parse
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

        assert_eq!(
            tracker.track_method(&definition),
            RubyType::Array(vec![RubyType::Unknown])
        );
        let read = exact_local_read(&mut tracker, source, "items\nend");
        assert_eq!(read.ruby_type, RubyType::Array(vec![RubyType::Unknown]));
        assert_eq!(
            read.unknown_reason,
            Some(UnknownReason::MutableShapeInvalidated)
        );
    }

    #[test]
    fn exceeding_the_array_positional_shape_bound_fails_closed() {
        let source = r#"def read
  child = { count: 1 }
  items = [child, child, child, child, child, child, child, child, child]
  items.first[:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::Unknown);
    }

    #[test]
    fn hash_to_h_preserves_the_shape_identity() {
        let source = r#"def read
  payload = { count: 1 }
  copy = payload.to_h
  copy[:count] = "many"
  payload[:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::string());
    }

    #[test]
    fn escaping_an_array_invalidates_its_contained_shape_identities() {
        let source = r#"def read
  child = { count: 1 }
  items = [child]
  dynamic_sink(items)
  child[:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::Unknown);
    }

    #[test]
    fn unsupported_array_mutation_invalidates_positional_shape_evidence() {
        let source = r#"def read
  first_child = { count: 1 }
  second_child = { count: "two" }
  items = [first_child, second_child]
  items.reverse!
  extracted = items.first
  extracted[:count] = true
  first_child[:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::Unknown);
    }

    #[test]
    fn non_mutating_merge_shares_nested_child_identities() {
        let source = r#"def read
  child = { count: 1 }
  payload = { nested: child }
  combined = payload.merge({ state: :ready })
  extracted = combined[:nested]
  extracted[:count] = "many"
  payload[:nested][:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::string());
    }

    #[test]
    fn merge_bang_with_a_literal_links_nested_child_identities() {
        let source = r#"def read
  child = { count: 1 }
  payload = { state: :empty }
  payload.merge!({ nested: child })
  child[:count] = "many"
  payload[:nested][:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::string());
    }

    #[test]
    fn merge_bang_with_a_shape_alias_links_nested_child_identities() {
        let source = r#"def read
  child = { count: 1 }
  addition = { nested: child }
  payload = { state: :empty }
  payload.merge!(addition)
  child[:count] = "many"
  payload[:nested][:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::string());
    }

    #[test]
    fn hash_splat_shares_nested_child_identities() {
        let source = r#"def read
  child = { count: 1 }
  payload = { nested: child }
  copy = { **payload }
  extracted = copy[:nested]
  extracted[:count] = "many"
  payload[:nested][:count]
end"#;

        assert_eq!(tracked_method_type(source), RubyType::string());
    }

    #[test]
    fn branch_local_alias_write_joins_complete_shape_states() {
        let source = r#"def build(condition)
  payload = { count: 1, state: :ready }
  copy = payload
  if condition
    copy[:count] = "many"
  end
  payload
end"#;

        assert_eq!(
            tracked_method_type(source).to_string(),
            "({ count: Integer, state: :ready } | { count: String, state: :ready })"
        );
    }

    #[test]
    fn known_delete_clear_and_merge_bang_transform_the_shared_shape() {
        let delete_source = r#"def build
  payload = { count: 1, state: :ready }
  copy = payload
  copy.delete(:count)
  payload
end"#;
        assert_eq!(
            tracked_method_type(delete_source).to_string(),
            "{ state: :ready }"
        );

        let clear_source = r#"def build
  payload = { count: 1 }
  payload.clear
  payload
end"#;
        assert_eq!(tracked_method_type(clear_source).to_string(), "{ }");

        let merge_source = r#"def build
  payload = { count: 1, state: :waiting }
  copy = payload
  copy.merge!({ state: :ready, label: "done" })
  payload
end"#;
        assert_eq!(
            tracked_method_type(merge_source).to_string(),
            "{ count: Integer, label: String, state: :ready }"
        );
    }

    #[test]
    fn non_mutating_merge_creates_an_independent_shape_identity() {
        let source = r#"def build
  payload = { count: 1 }
  combined = payload.merge({ label: "done" })
  combined[:count] = "many"
  payload
end"#;

        assert_eq!(
            tracked_method_type(source).to_string(),
            "{ count: Integer }"
        );
    }

    #[test]
    fn unresolved_argument_escape_invalidates_every_shape_alias() {
        let source = r#"def build
  payload = { count: 1 }
  copy = payload
  dynamic_sink(copy)
  payload
end"#;

        assert_eq!(tracked_method_type(source), RubyType::Unknown);
    }

    #[test]
    fn unsupported_receiver_mutation_invalidates_every_shape_alias() {
        let source = r#"def build
  payload = { count: 1 }
  copy = payload
  copy.transform_values! { |value| value.to_s }
  payload
end"#;

        assert_eq!(tracked_method_type(source), RubyType::Unknown);
    }

    #[test]
    fn freeze_preserves_only_the_outer_shape_key_stability() {
        let source = r#"def build
  payload = { nested: { count: 1 } }
  copy = payload
  copy.freeze
  payload
end"#;

        assert_eq!(
            tracked_method_type(source).to_string(),
            "frozen { nested: { count: Integer } }"
        );
    }

    #[test]
    fn mutable_escape_retains_the_machine_readable_unknown_reason() {
        let source = r#"def build
  payload = { count: 1 }
  copy = payload
  dynamic_sink(copy)
  payload
end"#;
        let parse = ruby_prism::parse(source.as_bytes());
        let definition = parse
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

        tracker.track_method(&definition);
        let read = exact_local_read(&mut tracker, source, "payload\nend");

        assert_eq!(read.ruby_type, RubyType::Unknown);
        assert_eq!(
            read.unknown_reason,
            Some(UnknownReason::MutableShapeInvalidated)
        );
    }

    #[test]
    fn nonlocal_storage_invalidates_the_local_shape_identity() {
        for write in [
            "@stored = payload",
            "@@stored = payload",
            "$stored = payload",
            "STORED = payload",
            "Container::STORED = payload",
        ] {
            let source =
                format!("def build\n  payload = {{ count: 1 }}\n  {write}\n  payload\nend");
            assert_eq!(
                tracked_method_type(&source),
                RubyType::Unknown,
                "nonlocal write `{write}` must invalidate the escaped mutable identity"
            );
        }
    }

    #[test]
    fn exceeding_the_fixed_alias_bound_fails_closed() {
        let source = r#"def build
  original = { count: 1 }
  alias_1 = original
  alias_2 = original
  alias_3 = original
  alias_4 = original
  alias_5 = original
  alias_6 = original
  alias_7 = original
  alias_8 = original
  original
end"#;

        assert_eq!(tracked_method_type(source), RubyType::Unknown);
    }

    #[test]
    fn rebinding_an_alias_releases_it_from_the_identity_bound() {
        let source = r#"def build
  original = { count: 1 }
  alias_1 = original
  alias_2 = original
  alias_3 = original
  alias_4 = original
  alias_5 = original
  alias_6 = original
  alias_7 = original
  alias_1 = 1
  alias_8 = original
  original
end"#;

        assert_eq!(
            tracked_method_type(source).to_string(),
            "{ count: Integer }"
        );
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
