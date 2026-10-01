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
//! shared engine implementation takes a short read guard per call and never
//! holds one across the walk, so writers on other files are not stalled. Reads
//! that a walk site performs together under one guard are one method here; a
//! `local` callback lets such a method consult the walking file's own facts at
//! the same point in the sequence as before. The walk never writes the engine.

use crate::core::callables::callable_body::CallableBodySummary;
use crate::core::{
    ConstantTypeDependency, FullyQualifiedName, GraphNodeKind, NamespaceKind, ResolvedMethodCallee,
    RubyConstant, RubyMethod, RubyType, SourceFileId, TypeFact, TypeSubject, UnknownReason,
};
use crate::engine::{AnalysisQueryCache, Project, View};
use crate::indexer::MethodReceiver;
use crate::inference::higher_order::PreparedCallableSet;
use crate::inference::method::constructor::ConstructorResult;
use crate::inference::method::return_type::method_call_return_type;
use parking_lot::RwLock;

/// Which receiver methods a dispatched return lookup may see.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ReceiverAccess<'a> {
    /// Implicit or `self` receivers: private methods are visible.
    Any,
    /// Explicit receivers seen from `caller`: protected methods of related
    /// namespaces are visible.
    Protected { caller: &'a FullyQualifiedName },
    /// Explicit receivers with no caller namespace: public methods only.
    Public,
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

/// The shared project engine: each read takes its own short read guard.
impl Semantics for RwLock<Project> {
    fn has_graph_node(&self, namespace: &FullyQualifiedName) -> bool {
        let engine = self.read();
        View::new(&engine).has_graph_node(namespace)
    }

    fn namespace_node_kind(&self, namespace: &FullyQualifiedName) -> Option<GraphNodeKind> {
        let engine = self.read();
        View::new(&engine).namespace_node_kind(namespace)
    }

    fn resolve_constant_in_context(
        &self,
        parts: &[RubyConstant],
        lexical_context: &[RubyConstant],
    ) -> Option<FullyQualifiedName> {
        let engine = self.read();
        View::new(&engine).resolve_constant_in_context(parts, lexical_context)
    }

    fn type_namespace(&self, ruby_type: &RubyType) -> Option<FullyQualifiedName> {
        let engine = self.read();
        View::new(&engine).type_to_namespace(ruby_type)
    }

    fn extension_call_callees(
        &self,
        receiver: &MethodReceiver,
        method: &RubyMethod,
        current_namespace: &[RubyConstant],
        namespace_kind: NamespaceKind,
        cache: &AnalysisQueryCache,
    ) -> Vec<ResolvedMethodCallee> {
        let engine = self.read();
        let query = View::new(&engine);
        let namespace_fqn = match receiver {
            MethodReceiver::Constant(path) => {
                query.resolve_constant_receiver(path, current_namespace)
            }
            MethodReceiver::None | MethodReceiver::SelfReceiver | MethodReceiver::Super => {
                FullyQualifiedName::namespace_with_kind(current_namespace.to_vec(), namespace_kind)
            }
            MethodReceiver::LocalVariable(_)
            | MethodReceiver::InstanceVariable(_)
            | MethodReceiver::ClassVariable(_)
            | MethodReceiver::GlobalVariable(_)
            | MethodReceiver::Expression
            | MethodReceiver::MethodCall { .. }
            | MethodReceiver::Literal(_) => return Vec::new(),
        };
        query
            .resolve_method_callees_cached(&namespace_fqn, method, cache)
            .unwrap_or_default()
    }

    fn type_facts_for(&self, subject: &TypeSubject) -> Vec<TypeFact> {
        self.read().type_facts_for(subject)
    }

    fn method_fqns_in_file(&self, file_id: SourceFileId) -> Vec<FullyQualifiedName> {
        self.read()
            .method_facts_in_file(file_id)
            .into_iter()
            .map(|fact| fact.fqn)
            .collect()
    }

    fn first_constant_value_type(
        &self,
        candidates: &[FullyQualifiedName],
        local: LocalType<'_>,
    ) -> Option<(FullyQualifiedName, RubyType)> {
        let engine = self.read();
        let query = View::new(&engine);
        candidates.iter().find_map(|constant| {
            local(constant)
                .or_else(|| query.constant_value_type(constant))
                .map(|ruby_type| (constant.clone(), ruby_type))
        })
    }

    fn constant_value_or_reference_type(
        &self,
        constant: &FullyQualifiedName,
        path: &[RubyConstant],
    ) -> Option<RubyType> {
        let engine = self.read();
        let query = View::new(&engine);
        query
            .constant_value_type(constant)
            .or_else(|| query.constant_reference_type(path))
    }

    fn constant_reference_type(&self, path: &[RubyConstant]) -> Option<RubyType> {
        let engine = self.read();
        View::new(&engine).constant_reference_type(path)
    }

    fn resolved_constant_value_type(
        &self,
        name: &RubyConstant,
        current_namespace: &[RubyConstant],
    ) -> Option<RubyType> {
        let engine = self.read();
        let query = View::new(&engine);
        query
            .resolve_constant_in_context(std::slice::from_ref(name), current_namespace)
            .and_then(|resolved| {
                query.constant_value_type(&FullyQualifiedName::constant(resolved.namespace_parts()))
            })
    }

    fn constant_value_type_in_context(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
        lexical_context: &[RubyConstant],
    ) -> Option<RubyType> {
        let engine = self.read();
        let query = View::new(&engine);
        let constant = contextual_constant(&query, parts, absolute, lexical_context)?;
        query
            .constant_value_type(&constant)
            .or_else(|| query.constant_reference_type(constant.namespace_parts_slice()))
    }

    fn constant_callable_body(
        &self,
        constant: &FullyQualifiedName,
    ) -> Option<Result<CallableBodySummary, UnknownReason>> {
        let engine = self.read();
        View::new(&engine).constant_callable_body(constant)
    }

    fn constant_callable_body_in_context(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
        lexical_context: &[RubyConstant],
    ) -> Option<Result<CallableBodySummary, UnknownReason>> {
        let engine = self.read();
        let query = View::new(&engine);
        let constant = contextual_constant(&query, parts, absolute, lexical_context)?;
        query.constant_callable_body(&constant)
    }

    fn constant_dependency_type(&self, dependency: &ConstantTypeDependency) -> Option<RubyType> {
        let engine = self.read();
        View::new(&engine).constant_dependency_type(dependency)
    }

    fn constructor_result(&self, class: &FullyQualifiedName) -> ConstructorResult {
        let engine = self.read();
        View::new(&engine).constructor_result(class)
    }

    fn prepare_higher_order_call(
        &self,
        cache: Option<&AnalysisQueryCache>,
        receiver_type: Option<&RubyType>,
        implicit_namespace: &FullyQualifiedName,
        method_name: &str,
        argument_types: &[RubyType],
    ) -> Result<PreparedCallableSet, UnknownReason> {
        let engine = self.read();
        let query = View::new(&engine);
        crate::inference::rbs::prepare_higher_order_call_with_fallbacks(
            Some(&query),
            cache,
            receiver_type,
            Some(implicit_namespace),
            method_name,
            argument_types,
        )
    }

    fn receiver_method_return_type(
        &self,
        receiver: &FullyQualifiedName,
        method: &RubyMethod,
        access: ReceiverAccess<'_>,
        cache: Option<&AnalysisQueryCache>,
    ) -> Option<RubyType> {
        let engine = self.read();
        let query = View::new(&engine);
        match (access, cache) {
            (ReceiverAccess::Any, None) => query.method_return_type_for_receiver(receiver, method),
            (ReceiverAccess::Any, Some(cache)) => {
                query.method_return_type_for_receiver_cached(receiver, method, cache)
            }
            (ReceiverAccess::Protected { caller }, None) => {
                query.method_return_type_for_protected_receiver(receiver, method, caller)
            }
            (ReceiverAccess::Protected { caller }, Some(cache)) => query
                .method_return_type_for_protected_receiver_cached(receiver, method, caller, cache),
            (ReceiverAccess::Public, None) => {
                query.method_return_type_for_public_receiver(receiver, method)
            }
            (ReceiverAccess::Public, Some(cache)) => {
                query.method_return_type_for_public_receiver_cached(receiver, method, cache)
            }
        }
    }

    fn method_call_return_type(
        &self,
        receiver_type: &RubyType,
        method_name: &str,
    ) -> Option<RubyType> {
        let engine = self.read();
        let query = View::new(&engine);
        method_call_return_type(Some(&query), receiver_type, method_name)
    }

    fn rbs_parameter_contract_types(
        &self,
        method: &FullyQualifiedName,
        owner: &FullyQualifiedName,
        parameter_names: &[&str],
    ) -> Vec<Option<RubyType>> {
        let engine = self.read();
        let query = View::new(&engine);
        parameter_names
            .iter()
            .map(|name| query.rbs_parameter_contract_type(method, owner, name))
            .collect()
    }

    fn rbs_return_contract_type(
        &self,
        method: &FullyQualifiedName,
        owner: &FullyQualifiedName,
    ) -> Option<RubyType> {
        let engine = self.read();
        View::new(&engine).rbs_return_contract_type(method, owner)
    }

    fn super_method_return_type(
        &self,
        namespace: &FullyQualifiedName,
        method: &RubyMethod,
        local: LocalType<'_>,
    ) -> Option<RubyType> {
        let engine = self.read();
        let query = View::new(&engine);
        let callee = query.resolve_super_method_callee(namespace, method)?;
        let super_method =
            FullyQualifiedName::method(callee.owner.namespace_parts(), method.clone());
        local(&super_method).or_else(|| query.method_return_type_for_callee(&callee))
    }

    fn has_method_return_equation(&self, method: &FullyQualifiedName) -> bool {
        self.read().has_method_return_equation(method)
    }
}

/// The constant a reference names: itself when absolute, otherwise its
/// lexical resolution.
fn contextual_constant(
    query: &View<'_>,
    parts: &[RubyConstant],
    absolute: bool,
    lexical_context: &[RubyConstant],
) -> Option<FullyQualifiedName> {
    if absolute {
        return Some(FullyQualifiedName::constant(parts.to_vec()));
    }
    let resolved = query.resolve_constant_in_context(parts, lexical_context)?;
    Some(FullyQualifiedName::constant(resolved.namespace_parts()))
}
