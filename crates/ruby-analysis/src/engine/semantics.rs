//! Engine implementations of [`Semantics`]: one consistent [`View`], and the
//! shared project that takes a short read guard per call.

use crate::core::callables::callable_body::CallableBodySummary;
use crate::core::MethodReceiver;
use crate::core::{
    ConstantTypeDependency, FullyQualifiedName, GraphNodeKind, NamespaceKind, ResolvedMethodCallee,
    RubyConstant, RubyMethod, RubyType, SourceFileId, TypeFact, TypeSubject, UnknownReason,
};
use crate::engine::{AnalysisQueryCache, Project, View};
use crate::inference::higher_order::PreparedCallableSet;
use crate::inference::method::constructor::ConstructorResult;
use crate::inference::method::return_type::method_call_return_type;
use crate::inference::semantics::{LocalType, ReceiverAccess, Semantics};
use parking_lot::RwLock;

/// One consistent view of the project: every read sees the same state.
impl Semantics for View<'_> {
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
        cache: &AnalysisQueryCache,
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
        self.resolve_method_callees_cached(&namespace_fqn, method, cache)
            .unwrap_or_default()
    }

    fn type_facts_for(&self, subject: &TypeSubject) -> Vec<TypeFact> {
        self.engine.type_facts_for(subject)
    }

    fn method_fqns_in_file(&self, file_id: SourceFileId) -> Vec<FullyQualifiedName> {
        self.engine
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
        cache: Option<&AnalysisQueryCache>,
        receiver_type: Option<&RubyType>,
        implicit_namespace: &FullyQualifiedName,
        method_name: &str,
        argument_types: &[RubyType],
    ) -> Result<PreparedCallableSet, UnknownReason> {
        crate::inference::rbs::prepare_higher_order_call_with_fallbacks(
            Some(self),
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
        match (access, cache) {
            (ReceiverAccess::Any, None) => self.method_return_type_for_receiver(receiver, method),
            (ReceiverAccess::Any, Some(cache)) => {
                self.method_return_type_for_receiver_cached(receiver, method, cache)
            }
            (ReceiverAccess::Protected { caller }, None) => {
                self.method_return_type_for_protected_receiver(receiver, method, caller)
            }
            (ReceiverAccess::Protected { caller }, Some(cache)) => self
                .method_return_type_for_protected_receiver_cached(receiver, method, caller, cache),
            (ReceiverAccess::Public, None) => {
                self.method_return_type_for_public_receiver(receiver, method)
            }
            (ReceiverAccess::Public, Some(cache)) => {
                self.method_return_type_for_public_receiver_cached(receiver, method, cache)
            }
        }
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
}

/// The shared project engine: each read takes its own short read guard and
/// delegates to a [`View`] of the guarded state.
impl Semantics for RwLock<Project> {
    fn has_graph_node(&self, namespace: &FullyQualifiedName) -> bool {
        Semantics::has_graph_node(&self.read().view(), namespace)
    }

    fn namespace_node_kind(&self, namespace: &FullyQualifiedName) -> Option<GraphNodeKind> {
        Semantics::namespace_node_kind(&self.read().view(), namespace)
    }

    fn resolve_constant_in_context(
        &self,
        parts: &[RubyConstant],
        lexical_context: &[RubyConstant],
    ) -> Option<FullyQualifiedName> {
        Semantics::resolve_constant_in_context(&self.read().view(), parts, lexical_context)
    }

    fn type_namespace(&self, ruby_type: &RubyType) -> Option<FullyQualifiedName> {
        Semantics::type_namespace(&self.read().view(), ruby_type)
    }

    fn extension_call_callees(
        &self,
        receiver: &MethodReceiver,
        method: &RubyMethod,
        current_namespace: &[RubyConstant],
        namespace_kind: NamespaceKind,
        cache: &AnalysisQueryCache,
    ) -> Vec<ResolvedMethodCallee> {
        Semantics::extension_call_callees(
            &self.read().view(),
            receiver,
            method,
            current_namespace,
            namespace_kind,
            cache,
        )
    }

    fn type_facts_for(&self, subject: &TypeSubject) -> Vec<TypeFact> {
        Semantics::type_facts_for(&self.read().view(), subject)
    }

    fn method_fqns_in_file(&self, file_id: SourceFileId) -> Vec<FullyQualifiedName> {
        Semantics::method_fqns_in_file(&self.read().view(), file_id)
    }

    fn first_constant_value_type(
        &self,
        candidates: &[FullyQualifiedName],
        local: LocalType<'_>,
    ) -> Option<(FullyQualifiedName, RubyType)> {
        Semantics::first_constant_value_type(&self.read().view(), candidates, local)
    }

    fn constant_value_or_reference_type(
        &self,
        constant: &FullyQualifiedName,
        path: &[RubyConstant],
    ) -> Option<RubyType> {
        Semantics::constant_value_or_reference_type(&self.read().view(), constant, path)
    }

    fn constant_reference_type(&self, path: &[RubyConstant]) -> Option<RubyType> {
        Semantics::constant_reference_type(&self.read().view(), path)
    }

    fn resolved_constant_value_type(
        &self,
        name: &RubyConstant,
        current_namespace: &[RubyConstant],
    ) -> Option<RubyType> {
        Semantics::resolved_constant_value_type(&self.read().view(), name, current_namespace)
    }

    fn constant_value_type_in_context(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
        lexical_context: &[RubyConstant],
    ) -> Option<RubyType> {
        Semantics::constant_value_type_in_context(
            &self.read().view(),
            parts,
            absolute,
            lexical_context,
        )
    }

    fn constant_callable_body(
        &self,
        constant: &FullyQualifiedName,
    ) -> Option<Result<CallableBodySummary, UnknownReason>> {
        Semantics::constant_callable_body(&self.read().view(), constant)
    }

    fn constant_callable_body_in_context(
        &self,
        parts: &[RubyConstant],
        absolute: bool,
        lexical_context: &[RubyConstant],
    ) -> Option<Result<CallableBodySummary, UnknownReason>> {
        Semantics::constant_callable_body_in_context(
            &self.read().view(),
            parts,
            absolute,
            lexical_context,
        )
    }

    fn constant_dependency_type(&self, dependency: &ConstantTypeDependency) -> Option<RubyType> {
        Semantics::constant_dependency_type(&self.read().view(), dependency)
    }

    fn constructor_result(&self, class: &FullyQualifiedName) -> ConstructorResult {
        Semantics::constructor_result(&self.read().view(), class)
    }

    fn prepare_higher_order_call(
        &self,
        cache: Option<&AnalysisQueryCache>,
        receiver_type: Option<&RubyType>,
        implicit_namespace: &FullyQualifiedName,
        method_name: &str,
        argument_types: &[RubyType],
    ) -> Result<PreparedCallableSet, UnknownReason> {
        Semantics::prepare_higher_order_call(
            &self.read().view(),
            cache,
            receiver_type,
            implicit_namespace,
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
        Semantics::receiver_method_return_type(&self.read().view(), receiver, method, access, cache)
    }

    fn method_call_return_type(
        &self,
        receiver_type: &RubyType,
        method_name: &str,
    ) -> Option<RubyType> {
        Semantics::method_call_return_type(&self.read().view(), receiver_type, method_name)
    }

    fn rbs_parameter_contract_types(
        &self,
        method: &FullyQualifiedName,
        owner: &FullyQualifiedName,
        parameter_names: &[&str],
    ) -> Vec<Option<RubyType>> {
        Semantics::rbs_parameter_contract_types(&self.read().view(), method, owner, parameter_names)
    }

    fn rbs_return_contract_type(
        &self,
        method: &FullyQualifiedName,
        owner: &FullyQualifiedName,
    ) -> Option<RubyType> {
        Semantics::rbs_return_contract_type(&self.read().view(), method, owner)
    }

    fn super_method_return_type(
        &self,
        namespace: &FullyQualifiedName,
        method: &RubyMethod,
        local: LocalType<'_>,
    ) -> Option<RubyType> {
        Semantics::super_method_return_type(&self.read().view(), namespace, method, local)
    }

    fn has_method_return_equation(&self, method: &FullyQualifiedName) -> bool {
        Semantics::has_method_return_equation(&self.read().view(), method)
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
