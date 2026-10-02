//! Engine implementations of [`Semantics`]: one consistent [`View`], and the
//! shared project that takes a short read guard per call.

use crate::core::callables::callable_body::CallableBodySummary;
use crate::core::MethodReceiver;
use crate::core::{
    ConstantTypeDependency, FullyQualifiedName, GraphNodeKind, MethodFact, NamespaceKind,
    ResolvedMethodCallee, RubyConstant, RubyMethod, RubyType, SourceFileId, TypeFact, TypeSubject,
    UnknownReason, VariableTypeKind,
};
use crate::engine::lookup::{self, LookupReceiver, MethodRequest, MethodWant};
use crate::engine::{AnalysisQueryCache, Project, View};
use crate::inference::higher_order::PreparedCallableSet;
use crate::inference::method::constructor::ConstructorResult;
use crate::inference::method::return_type::method_call_return_type;
use crate::inference::semantics::{LocalType, ReceiverAccess, Semantics};
use parking_lot::RwLock;
use std::sync::Arc;

/// One consistent view of the project: every read sees the same state.
impl Semantics for View<'_> {
    fn for_walk<'s>(self: Arc<Self>) -> Arc<dyn Semantics + 's>
    where
        Self: 's,
    {
        self
    }

    fn memo_identity(&self) -> (u64, u64) {
        self.query_cache_identity()
    }

    fn has_graph_node(&self, namespace: &FullyQualifiedName) -> bool {
        View::has_graph_node(self, namespace)
    }

    fn namespace_node_kind(&self, namespace: &FullyQualifiedName) -> Option<GraphNodeKind> {
        View::namespace_node_kind(self, namespace)
    }

    fn resolve_constant_in_context(
        &self,
        parts: &[RubyConstant],
        lexical_context: &[RubyConstant],
    ) -> Option<FullyQualifiedName> {
        View::resolve_constant_in_context(self, parts, lexical_context)
    }

    fn type_namespace(&self, ruby_type: &RubyType) -> Option<FullyQualifiedName> {
        self.type_to_namespace(ruby_type)
    }

    fn extension_call_callees(
        &self,
        receiver: &MethodReceiver,
        method: &RubyMethod,
        current_namespace: &[RubyConstant],
        namespace_kind: NamespaceKind,
    ) -> Vec<ResolvedMethodCallee> {
        let namespace_fqn = match receiver {
            MethodReceiver::Constant(path) => {
                self.resolve_constant_receiver(path, current_namespace)
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
        let receiver = LookupReceiver::Namespace(&namespace_fqn);
        lookup::method(
            self,
            MethodRequest::new(receiver, *method, MethodWant::Callees),
        )
        .into_callees()
        .unwrap_or_default()
    }

    fn type_facts_for(&self, subject: &TypeSubject) -> Vec<TypeFact> {
        View::type_facts_for(self, subject)
    }

    fn method_fqns_in_file(&self, file_id: SourceFileId) -> Vec<FullyQualifiedName> {
        self.method_facts_in_file(file_id)
            .into_iter()
            .map(|fact| fact.fqn)
            .collect()
    }

    fn first_constant_value_type(
        &self,
        candidates: &[FullyQualifiedName],
        local: LocalType<'_>,
    ) -> Option<(FullyQualifiedName, RubyType)> {
        candidates.iter().find_map(|constant| {
            local(constant)
                .or_else(|| self.constant_value_type(constant))
                .map(|ruby_type| (constant.clone(), ruby_type))
        })
    }

    fn constant_value_or_reference_type(
        &self,
        constant: &FullyQualifiedName,
        path: &[RubyConstant],
    ) -> Option<RubyType> {
        self.constant_value_type(constant)
            .or_else(|| View::constant_reference_type(self, path))
    }

    fn constant_reference_type(&self, path: &[RubyConstant]) -> Option<RubyType> {
        View::constant_reference_type(self, path)
    }

    fn resolved_constant_value_type(
        &self,
        name: &RubyConstant,
        current_namespace: &[RubyConstant],
    ) -> Option<RubyType> {
        View::resolve_constant_in_context(self, std::slice::from_ref(name), current_namespace)
            .and_then(|resolved| {
                self.constant_value_type(&FullyQualifiedName::constant(resolved.namespace_parts()))
            })
    }

    fn constant_value_type_in_context(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
        lexical_context: &[RubyConstant],
    ) -> Option<RubyType> {
        let constant = contextual_constant(self, parts, absolute, lexical_context)?;
        self.constant_value_type(&constant)
            .or_else(|| View::constant_reference_type(self, constant.namespace_parts_slice()))
    }

    fn constant_callable_body(
        &self,
        constant: &FullyQualifiedName,
    ) -> Option<Result<CallableBodySummary, UnknownReason>> {
        View::constant_callable_body(self, constant)
    }

    fn constant_callable_body_in_context(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
        lexical_context: &[RubyConstant],
    ) -> Option<Result<CallableBodySummary, UnknownReason>> {
        let constant = contextual_constant(self, parts, absolute, lexical_context)?;
        View::constant_callable_body(self, &constant)
    }

    fn constant_dependency_type(&self, dependency: &ConstantTypeDependency) -> Option<RubyType> {
        View::constant_dependency_type(self, dependency)
    }

    fn constructor_result(&self, class: &FullyQualifiedName) -> ConstructorResult {
        View::constructor_result(self, class)
    }

    fn prepare_higher_order_call(
        &self,
        receiver_type: Option<&RubyType>,
        implicit_namespace: &FullyQualifiedName,
        method_name: &str,
        argument_types: &[RubyType],
    ) -> Result<PreparedCallableSet, UnknownReason> {
        crate::inference::rbs::prepare_higher_order_call_with_fallbacks(
            Some(self),
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
    ) -> Option<RubyType> {
        let receiver = LookupReceiver::Namespace(receiver);
        let request = MethodRequest::new(receiver, *method, MethodWant::Return).with_access(access);
        lookup::method(self, request).into_return_type()
    }

    fn method_signature_facts(
        &self,
        namespace: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Arc<Vec<MethodFact>> {
        let receiver = LookupReceiver::Namespace(namespace);
        lookup::method(
            self,
            MethodRequest::new(receiver, *method, MethodWant::Signatures),
        )
        .into_signatures()
    }

    fn method_signature_facts_for_type(
        &self,
        receiver_type: &RubyType,
        method: &RubyMethod,
    ) -> Arc<Vec<MethodFact>> {
        let receiver = LookupReceiver::Type(receiver_type);
        lookup::method(
            self,
            MethodRequest::new(receiver, *method, MethodWant::Signatures),
        )
        .into_signatures()
    }

    fn method_call_return_type(
        &self,
        receiver_type: &RubyType,
        method_name: &str,
    ) -> Option<RubyType> {
        method_call_return_type(Some(self), receiver_type, method_name)
    }

    fn rbs_parameter_contract_types(
        &self,
        method: &FullyQualifiedName,
        owner: &FullyQualifiedName,
        parameter_names: &[&str],
    ) -> Vec<Option<RubyType>> {
        parameter_names
            .iter()
            .map(|name| self.rbs_parameter_contract_type(method, owner, name))
            .collect()
    }

    fn rbs_return_contract_type(
        &self,
        method: &FullyQualifiedName,
        owner: &FullyQualifiedName,
    ) -> Option<RubyType> {
        View::rbs_return_contract_type(self, method, owner)
    }

    fn super_method_return_type(
        &self,
        namespace: &FullyQualifiedName,
        method: &RubyMethod,
        local: LocalType<'_>,
    ) -> Option<RubyType> {
        let callee = self.resolve_super_method_callee(namespace, method)?;
        let super_method =
            FullyQualifiedName::method(callee.owner.namespace_parts(), method.clone());
        local(&super_method).or_else(|| self.method_return_type_for_callee(&callee))
    }

    fn has_method_return_equation(&self, method: &FullyQualifiedName) -> bool {
        self.engine.has_method_return_equation(method)
    }

    fn namespace_type(&self, namespace: &FullyQualifiedName) -> Option<RubyType> {
        View::namespace_type(self, namespace)
    }

    fn constant_value_type(&self, constant: &FullyQualifiedName) -> Option<RubyType> {
        View::constant_value_type(self, constant)
    }

    fn resolve_constant_receiver(
        &self,
        path: &[RubyConstant],
        current_namespace: &[RubyConstant],
    ) -> FullyQualifiedName {
        View::resolve_constant_receiver(self, path, current_namespace)
    }

    fn local_variable_type_at(
        &self,
        name: &str,
        scope_id: u32,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<RubyType> {
        View::local_variable_type_at(self, name, scope_id, file_id, byte_offset)
    }

    fn variable_type_before_in_owner(
        &self,
        kind: VariableTypeKind,
        name: &str,
        owner: &FullyQualifiedName,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<RubyType> {
        View::variable_type_before_in_owner(self, kind, name, owner, file_id, byte_offset)
    }
}

/// A source of project views: each read takes its own short read guard and
/// answers through a [`View`] of the guarded state, so no guard is held
/// across a walk.
trait ProjectReads: Send + Sync {
    fn with_view<R>(&self, read: impl FnOnce(&View<'_>) -> R) -> R;

    fn walk<'s>(self: Arc<Self>) -> Arc<dyn Semantics + 's>
    where
        Self: 's;
}

/// The shared project engine. A walk over it reads through a [`ProjectWalk`]
/// so its method lookups share one memo.
impl ProjectReads for RwLock<Project> {
    fn with_view<R>(&self, read: impl FnOnce(&View<'_>) -> R) -> R {
        read(&self.read().view())
    }

    fn walk<'s>(self: Arc<Self>) -> Arc<dyn Semantics + 's>
    where
        Self: 's,
    {
        Arc::new(ProjectWalk {
            project: self,
            memo: AnalysisQueryCache::default(),
        })
    }
}

/// One file walk over the shared engine: every read takes its own short read
/// guard, and the walk's method lookups share one memo bound to the engine
/// identity, so a write between reads drops stale entries.
struct ProjectWalk {
    project: Arc<RwLock<Project>>,
    memo: AnalysisQueryCache,
}

impl ProjectReads for ProjectWalk {
    fn with_view<R>(&self, read: impl FnOnce(&View<'_>) -> R) -> R {
        read(&self.project.read().view().with_memo(&self.memo))
    }

    fn walk<'s>(self: Arc<Self>) -> Arc<dyn Semantics + 's>
    where
        Self: 's,
    {
        self
    }
}

impl<T: ProjectReads> Semantics for T {
    fn for_walk<'s>(self: Arc<Self>) -> Arc<dyn Semantics + 's>
    where
        Self: 's,
    {
        ProjectReads::walk(self)
    }

    fn memo_identity(&self) -> (u64, u64) {
        self.with_view(|view| Semantics::memo_identity(view))
    }

    fn has_graph_node(&self, namespace: &FullyQualifiedName) -> bool {
        self.with_view(|view| Semantics::has_graph_node(view, namespace))
    }

    fn namespace_node_kind(&self, namespace: &FullyQualifiedName) -> Option<GraphNodeKind> {
        self.with_view(|view| Semantics::namespace_node_kind(view, namespace))
    }

    fn resolve_constant_in_context(
        &self,
        parts: &[RubyConstant],
        lexical_context: &[RubyConstant],
    ) -> Option<FullyQualifiedName> {
        self.with_view(|view| Semantics::resolve_constant_in_context(view, parts, lexical_context))
    }

    fn type_namespace(&self, ruby_type: &RubyType) -> Option<FullyQualifiedName> {
        self.with_view(|view| Semantics::type_namespace(view, ruby_type))
    }

    fn extension_call_callees(
        &self,
        receiver: &MethodReceiver,
        method: &RubyMethod,
        current_namespace: &[RubyConstant],
        namespace_kind: NamespaceKind,
    ) -> Vec<ResolvedMethodCallee> {
        self.with_view(|view| {
            Semantics::extension_call_callees(
                view,
                receiver,
                method,
                current_namespace,
                namespace_kind,
            )
        })
    }

    fn type_facts_for(&self, subject: &TypeSubject) -> Vec<TypeFact> {
        self.with_view(|view| Semantics::type_facts_for(view, subject))
    }

    fn method_fqns_in_file(&self, file_id: SourceFileId) -> Vec<FullyQualifiedName> {
        self.with_view(|view| Semantics::method_fqns_in_file(view, file_id))
    }

    fn first_constant_value_type(
        &self,
        candidates: &[FullyQualifiedName],
        local: LocalType<'_>,
    ) -> Option<(FullyQualifiedName, RubyType)> {
        self.with_view(|view| Semantics::first_constant_value_type(view, candidates, local))
    }

    fn constant_value_or_reference_type(
        &self,
        constant: &FullyQualifiedName,
        path: &[RubyConstant],
    ) -> Option<RubyType> {
        self.with_view(|view| Semantics::constant_value_or_reference_type(view, constant, path))
    }

    fn constant_reference_type(&self, path: &[RubyConstant]) -> Option<RubyType> {
        self.with_view(|view| Semantics::constant_reference_type(view, path))
    }

    fn resolved_constant_value_type(
        &self,
        name: &RubyConstant,
        current_namespace: &[RubyConstant],
    ) -> Option<RubyType> {
        self.with_view(|view| {
            Semantics::resolved_constant_value_type(view, name, current_namespace)
        })
    }

    fn constant_value_type_in_context(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
        lexical_context: &[RubyConstant],
    ) -> Option<RubyType> {
        self.with_view(|view| {
            Semantics::constant_value_type_in_context(view, parts, absolute, lexical_context)
        })
    }

    fn constant_callable_body(
        &self,
        constant: &FullyQualifiedName,
    ) -> Option<Result<CallableBodySummary, UnknownReason>> {
        self.with_view(|view| Semantics::constant_callable_body(view, constant))
    }

    fn constant_callable_body_in_context(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
        lexical_context: &[RubyConstant],
    ) -> Option<Result<CallableBodySummary, UnknownReason>> {
        self.with_view(|view| {
            Semantics::constant_callable_body_in_context(view, parts, absolute, lexical_context)
        })
    }

    fn constant_dependency_type(&self, dependency: &ConstantTypeDependency) -> Option<RubyType> {
        self.with_view(|view| Semantics::constant_dependency_type(view, dependency))
    }

    fn constructor_result(&self, class: &FullyQualifiedName) -> ConstructorResult {
        self.with_view(|view| Semantics::constructor_result(view, class))
    }

    fn prepare_higher_order_call(
        &self,
        receiver_type: Option<&RubyType>,
        implicit_namespace: &FullyQualifiedName,
        method_name: &str,
        argument_types: &[RubyType],
    ) -> Result<PreparedCallableSet, UnknownReason> {
        self.with_view(|view| {
            Semantics::prepare_higher_order_call(
                view,
                receiver_type,
                implicit_namespace,
                method_name,
                argument_types,
            )
        })
    }

    fn receiver_method_return_type(
        &self,
        receiver: &FullyQualifiedName,
        method: &RubyMethod,
        access: ReceiverAccess<'_>,
    ) -> Option<RubyType> {
        self.with_view(|view| {
            Semantics::receiver_method_return_type(view, receiver, method, access)
        })
    }

    fn method_signature_facts(
        &self,
        namespace: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Arc<Vec<MethodFact>> {
        self.with_view(|view| Semantics::method_signature_facts(view, namespace, method))
    }

    fn method_signature_facts_for_type(
        &self,
        receiver_type: &RubyType,
        method: &RubyMethod,
    ) -> Arc<Vec<MethodFact>> {
        self.with_view(|view| {
            Semantics::method_signature_facts_for_type(view, receiver_type, method)
        })
    }

    fn method_call_return_type(
        &self,
        receiver_type: &RubyType,
        method_name: &str,
    ) -> Option<RubyType> {
        self.with_view(|view| Semantics::method_call_return_type(view, receiver_type, method_name))
    }

    fn rbs_parameter_contract_types(
        &self,
        method: &FullyQualifiedName,
        owner: &FullyQualifiedName,
        parameter_names: &[&str],
    ) -> Vec<Option<RubyType>> {
        self.with_view(|view| {
            Semantics::rbs_parameter_contract_types(view, method, owner, parameter_names)
        })
    }

    fn rbs_return_contract_type(
        &self,
        method: &FullyQualifiedName,
        owner: &FullyQualifiedName,
    ) -> Option<RubyType> {
        self.with_view(|view| Semantics::rbs_return_contract_type(view, method, owner))
    }

    fn super_method_return_type(
        &self,
        namespace: &FullyQualifiedName,
        method: &RubyMethod,
        local: LocalType<'_>,
    ) -> Option<RubyType> {
        self.with_view(|view| Semantics::super_method_return_type(view, namespace, method, local))
    }

    fn has_method_return_equation(&self, method: &FullyQualifiedName) -> bool {
        self.with_view(|view| Semantics::has_method_return_equation(view, method))
    }

    fn namespace_type(&self, namespace: &FullyQualifiedName) -> Option<RubyType> {
        self.with_view(|view| Semantics::namespace_type(view, namespace))
    }

    fn constant_value_type(&self, constant: &FullyQualifiedName) -> Option<RubyType> {
        self.with_view(|view| Semantics::constant_value_type(view, constant))
    }

    fn resolve_constant_receiver(
        &self,
        path: &[RubyConstant],
        current_namespace: &[RubyConstant],
    ) -> FullyQualifiedName {
        self.with_view(|view| Semantics::resolve_constant_receiver(view, path, current_namespace))
    }

    fn local_variable_type_at(
        &self,
        name: &str,
        scope_id: u32,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<RubyType> {
        self.with_view(|view| {
            Semantics::local_variable_type_at(view, name, scope_id, file_id, byte_offset)
        })
    }

    fn variable_type_before_in_owner(
        &self,
        kind: VariableTypeKind,
        name: &str,
        owner: &FullyQualifiedName,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<RubyType> {
        self.with_view(|view| {
            Semantics::variable_type_before_in_owner(view, kind, name, owner, file_id, byte_offset)
        })
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
