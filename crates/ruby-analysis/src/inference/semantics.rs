//! Read-only project semantics consulted while one file is being walked.
//!
//! The fact collector and `TypeTracker` see other files only through
//! [`Semantics`]. Most cross-file evidence is recorded as an equation and
//! solved after the file is installed; the reads here are the ones that cannot
//! wait for that solve. They come in two kinds:
//!
//! - reads that decide which facts get emitted (a namespace or singleton
//!   receiver, `initialize` inside a class, extension call targets), and
//! - reads that feed local flow (RBS parameter and return contracts,
//!   higher-order block parameters, callable constant bodies, method returns
//!   through dispatch, `super`, and constructors).
//!
//! Any new mid-walk read must become an equation or be added to `Semantics`
//! with a reason.
//!
//! Each method answers one question and returns plain domain values. The
//! trait lives with inference so the walk names no engine type; the engine
//! implements it. Its project view answers every read over one consistent
//! state. The shared engine implementation takes a short read guard per call,
//! delegates to a view of the guarded state, and never holds a guard across
//! the walk, so writers on other files are not stalled. Reads that a walk site performs together under one guard are one method here; a
//! `local` callback lets such a method consult the walking file's own facts at
//! the same point in the sequence as before. The walk never writes the engine.

use crate::core::callables::callable_body::CallableBodySummary;
use crate::core::MethodReceiver;
use crate::core::{
    ConstantTypeDependency, FullyQualifiedName, GraphNodeKind, NamespaceKind, ResolvedMethodCallee,
    RubyConstant, RubyMethod, RubyType, SourceFileId, TypeFact, TypeSubject, UnknownReason,
};
use crate::engine::AnalysisQueryCache;
use crate::inference::higher_order::PreparedCallableSet;
use crate::inference::method::constructor::ConstructorResult;

/// Which receiver methods a dispatched return lookup may see.
#[derive(Debug, Clone, Copy)]
pub enum ReceiverAccess<'a> {
    /// Implicit or `self` receivers: private methods are visible.
    Any,
    /// Explicit receivers seen from `caller`: protected methods of related
    /// namespaces are visible.
    Protected { caller: &'a FullyQualifiedName },
    /// Explicit receivers with no caller namespace: public methods only.
    Public,
}

impl<'a> ReceiverAccess<'a> {
    /// The engine's visibility filter: whether private methods are visible,
    /// and the caller namespace that protected methods are checked against.
    pub(crate) fn visibility(self) -> (bool, Option<&'a FullyQualifiedName>) {
        match self {
            ReceiverAccess::Any => (true, None),
            ReceiverAccess::Protected { caller } => (false, Some(caller)),
            ReceiverAccess::Public => (false, None),
        }
    }
}

/// Local evidence from the walking file, consulted before the engine.
pub(crate) type LocalType<'a> = &'a dyn Fn(&FullyQualifiedName) -> Option<RubyType>;

/// Project semantics readable in the middle of a file walk.
pub(crate) trait Semantics: Send + Sync {
    // Reads that decide which facts get emitted.

    /// Whether any graph node names `namespace`; a known namespace turns a
    /// constant receiver into a singleton receiver before its call is emitted.
    fn has_graph_node(&self, namespace: &FullyQualifiedName) -> bool;

    /// The latest class/module kind of `namespace`; it decides whether a `def
    /// initialize` is a constructor and which reference type a namespace gets.
    fn namespace_node_kind(&self, namespace: &FullyQualifiedName) -> Option<GraphNodeKind>;

    /// Lexical constant resolution; it picks the receiver namespace a call or
    /// mixin fact is emitted against.
    fn resolve_constant_in_context(
        &self,
        parts: &[RubyConstant],
        lexical_context: &[RubyConstant],
    ) -> Option<FullyQualifiedName>;

    /// The namespace a value type dispatches through; it picks the receiver
    /// namespace and kind a call fact is emitted against.
    fn type_namespace(&self, ruby_type: &RubyType) -> Option<FullyQualifiedName>;

    /// Callees an extension sees for a call; extensions emit facts from them
    /// during the walk.
    fn extension_call_callees(
        &self,
        receiver: &MethodReceiver,
        method: &RubyMethod,
        current_namespace: &[RubyConstant],
        namespace_kind: NamespaceKind,
        cache: &AnalysisQueryCache,
    ) -> Vec<ResolvedMethodCallee>;

    /// Installed type facts for `subject`; extensions read runtime proxy types
    /// from them to decide which facts to emit.
    fn type_facts_for(&self, subject: &TypeSubject) -> Vec<TypeFact>;

    // Reads that feed local flow.

    /// Methods the engine already holds for this file; their returns may
    /// become equation dependencies of the new pass.
    fn method_fqns_in_file(&self, file_id: SourceFileId) -> Vec<FullyQualifiedName>;

    /// The first candidate with a proven value type, checking `local` before
    /// the engine for each candidate; local flow assigns it immediately.
    fn first_constant_value_type(
        &self,
        candidates: &[FullyQualifiedName],
        local: LocalType<'_>,
    ) -> Option<(FullyQualifiedName, RubyType)>;

    /// A constant's value type, else the reference type of `path`; local flow
    /// assigns it immediately.
    fn constant_value_or_reference_type(
        &self,
        constant: &FullyQualifiedName,
        path: &[RubyConstant],
    ) -> Option<RubyType>;

    /// A constant path's class or module reference type; local flow assigns it
    /// immediately.
    fn constant_reference_type(&self, path: &[RubyConstant]) -> Option<RubyType>;

    /// The value type of `name` resolved in `current_namespace`; local flow
    /// types a constant receiver with it.
    fn resolved_constant_value_type(
        &self,
        name: &RubyConstant,
        current_namespace: &[RubyConstant],
    ) -> Option<RubyType>;

    /// The value type of a constant reference, resolved lexically unless
    /// `absolute`; `TypeTracker` assigns it immediately.
    fn constant_value_type_in_context(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
        lexical_context: &[RubyConstant],
    ) -> Option<RubyType>;

    /// The callable body stored for a constant; local flow calls it as a block.
    fn constant_callable_body(
        &self,
        constant: &FullyQualifiedName,
    ) -> Option<Result<CallableBodySummary, UnknownReason>>;

    /// The callable body of a constant reference, resolved lexically unless
    /// `absolute`; `TypeTracker` calls it as a block.
    fn constant_callable_body_in_context(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
        lexical_context: &[RubyConstant],
    ) -> Option<Result<CallableBodySummary, UnknownReason>>;

    /// A constructor-instance dependency's type; the local method-return solve
    /// needs it before the engine solve.
    fn constant_dependency_type(&self, dependency: &ConstantTypeDependency) -> Option<RubyType>;

    /// What `Class.new` returns for `class`; local flow types the call.
    fn constructor_result(&self, class: &FullyQualifiedName) -> ConstructorResult;

    /// The higher-order signature for a call; local flow types the block
    /// parameters and the call result from it.
    fn prepare_higher_order_call(
        &self,
        cache: Option<&AnalysisQueryCache>,
        receiver_type: Option<&RubyType>,
        implicit_namespace: &FullyQualifiedName,
        method_name: &str,
        argument_types: &[RubyType],
    ) -> Result<PreparedCallableSet, UnknownReason>;

    /// A method's return type through receiver dispatch; local flow types the
    /// call and delegated methods with it.
    fn receiver_method_return_type(
        &self,
        receiver: &FullyQualifiedName,
        method: &RubyMethod,
        access: ReceiverAccess<'_>,
        cache: Option<&AnalysisQueryCache>,
    ) -> Option<RubyType>;

    /// A method's return type for a receiver value type; delegated methods
    /// take it as their local return.
    fn method_call_return_type(
        &self,
        receiver_type: &RubyType,
        method_name: &str,
    ) -> Option<RubyType>;

    /// RBS contract types for each named parameter, index-aligned; they seed
    /// the method body's local flow.
    fn rbs_parameter_contract_types(
        &self,
        method: &FullyQualifiedName,
        owner: &FullyQualifiedName,
        parameter_names: &[&str],
    ) -> Vec<Option<RubyType>>;

    /// The project RBS return contract; it fixes the method's local return.
    fn rbs_return_contract_type(
        &self,
        method: &FullyQualifiedName,
        owner: &FullyQualifiedName,
    ) -> Option<RubyType>;

    /// The return of the `super` target, checking `local` for the resolved
    /// owner's method first; local flow types the `super` call with it.
    fn super_method_return_type(
        &self,
        namespace: &FullyQualifiedName,
        method: &RubyMethod,
        local: LocalType<'_>,
    ) -> Option<RubyType>;

    /// Whether the engine holds a return equation for `method`; local flow
    /// records it as a dependency instead of trusting its current type.
    fn has_method_return_equation(&self, method: &FullyQualifiedName) -> bool;
}
