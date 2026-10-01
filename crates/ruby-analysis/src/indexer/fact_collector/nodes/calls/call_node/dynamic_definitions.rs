//! `define_method`, `define_singleton_method`, and `send`-driven dynamic method definitions.

use crate::core::{
    FullyQualifiedName, NamespaceKind, RubyConstant, RubyMethod, TypeFact, TypeProvenance,
    TypeSubject,
};
use crate::indexer::mixin_ref_from_node;
use ruby_prism::{CallNode, Node};

use crate::indexer::yard::converter::YardTypeConverter;

use super::names::{define_method_name_and_range, direct_attr_name_and_range};
use crate::indexer::fact_collector::FactCollector;

impl FactCollector {
    pub(super) fn push_direct_define_method_fact(&mut self, node: &CallNode) {
        let Some((name, range)) = define_method_name_and_range(self, node, 0) else {
            return;
        };
        let Ok(method) = RubyMethod::new(&name) else {
            return;
        };
        let (namespace, receiver_kind) = self.scope_tracker.implicit_receiver_context();
        if receiver_kind != NamespaceKind::Singleton || namespace.is_empty() {
            return;
        }
        let owner_kind = if !self.scope_tracker.execution_context_active()
            && self.scope_tracker.in_singleton()
        {
            NamespaceKind::Singleton
        } else {
            NamespaceKind::Instance
        };
        self.direct_push_method_fact_with_visibility(
            namespace.clone(),
            owner_kind,
            method,
            range,
            self.scope_tracker.current_visibility(),
        );
        self.push_direct_define_method_return_type(namespace, method, range, node);
    }

    pub(super) fn push_direct_define_singleton_method_fact(&mut self, node: &CallNode) {
        let (namespace, receiver_kind) = self.scope_tracker.implicit_receiver_context();
        if receiver_kind != NamespaceKind::Singleton || namespace.is_empty() {
            return;
        }
        self.push_direct_define_singleton_method_for_namespace(node, namespace);
    }

    pub(super) fn push_direct_receiver_define_singleton_method_fact(&mut self, node: &CallNode) {
        if node.name().as_slice() != b"define_singleton_method" {
            return;
        }
        let Some(receiver) = node.receiver() else {
            return;
        };
        let Some(namespace) = self.resolve_constant_receiver_namespace(&receiver) else {
            return;
        };
        self.push_direct_define_singleton_method_for_namespace(node, namespace);
    }

    fn push_direct_define_singleton_method_for_namespace(
        &mut self,
        node: &CallNode,
        namespace: Vec<RubyConstant>,
    ) {
        let Some((name, range)) = define_method_name_and_range(self, node, 0) else {
            return;
        };
        let Ok(method) = RubyMethod::new(&name) else {
            return;
        };
        self.direct_push_method_fact_with_visibility(
            namespace.clone(),
            NamespaceKind::Singleton,
            method,
            range,
            self.scope_tracker.current_visibility(),
        );
        self.push_direct_define_method_return_type(namespace, method, range, node);
    }

    pub(super) fn push_direct_send_dynamic_method_fact(&mut self, node: &CallNode) {
        if !matches!(
            node.name().as_slice(),
            b"send" | b"public_send" | b"__send__"
        ) {
            return;
        }
        let Some(arguments) = node.arguments() else {
            return;
        };
        let mut args = arguments.arguments().iter();
        let Some((selector, _)) = args
            .next()
            .and_then(|arg| direct_attr_name_and_range(self, &arg))
        else {
            return;
        };
        let owner_kind = match selector.as_str() {
            "define_method" if node.name().as_slice() != b"public_send" => NamespaceKind::Instance,
            "define_singleton_method" => NamespaceKind::Singleton,
            _ => return,
        };
        let Some(receiver) = node.receiver() else {
            return;
        };
        let Some(namespace) = self.resolve_constant_receiver_namespace(&receiver) else {
            return;
        };
        let Some((name, range)) = define_method_name_and_range(self, node, 1) else {
            return;
        };
        let Ok(method) = RubyMethod::new(&name) else {
            return;
        };
        self.direct_push_method_fact_with_visibility(
            namespace.clone(),
            owner_kind,
            method,
            range,
            self.scope_tracker.current_visibility(),
        );
        self.push_direct_define_method_return_type(namespace, method, range, node);
    }

    pub(in crate::indexer::fact_collector) fn resolve_constant_receiver_namespace(
        &self,
        receiver: &Node<'_>,
    ) -> Option<Vec<RubyConstant>> {
        if let Some(namespace) = self.resolve_const_get_receiver_namespace(receiver) {
            return Some(namespace);
        }

        let receiver_ref = mixin_ref_from_node(receiver)?;
        let mut search = if receiver_ref.absolute {
            Vec::new()
        } else {
            self.scope_tracker.get_ns_stack()
        };

        loop {
            let mut candidate = search.clone();
            candidate.extend(receiver_ref.parts.iter().cloned());
            let fqn = FullyQualifiedName::namespace(candidate.clone());
            if self.namespace_is_known(&fqn) {
                return Some(candidate);
            }
            if receiver_ref.absolute || search.is_empty() {
                break;
            }
            search.pop();
        }

        let fqn = FullyQualifiedName::namespace(receiver_ref.parts.clone());
        self.namespace_is_known(&fqn).then_some(receiver_ref.parts)
    }

    fn resolve_const_get_receiver_namespace(
        &self,
        receiver: &Node<'_>,
    ) -> Option<Vec<RubyConstant>> {
        let call = receiver.as_call_node()?;
        if call.name().as_slice() != b"const_get" {
            return None;
        }
        let Some(base_receiver) = call.receiver() else {
            return None;
        };
        let arguments = call.arguments()?;
        let first = arguments.arguments().iter().next()?;
        let (name, _) = direct_attr_name_and_range(self, &first)?;
        let Ok(constant) = RubyConstant::new(&name) else {
            return None;
        };
        let mut namespace = self.resolve_constant_receiver_namespace(&base_receiver)?;
        namespace.push(constant);
        let fqn = FullyQualifiedName::namespace(namespace.clone());
        self.namespace_is_known(&fqn).then_some(namespace)
    }

    fn push_direct_define_method_return_type(
        &mut self,
        namespace: Vec<RubyConstant>,
        method: RubyMethod,
        range: crate::core::TextRange,
        node: &CallNode,
    ) {
        let Some(doc) = self.extract_doc_comments(node.location().start_offset()) else {
            return;
        };
        if doc.returns.is_empty() {
            return;
        }
        let all_return_types = doc
            .returns
            .iter()
            .flat_map(|r| r.types.clone())
            .collect::<Vec<_>>();
        if all_return_types.is_empty() {
            return;
        }
        let return_type = YardTypeConverter::convert_multiple(&all_return_types);
        self.facts.types.add(TypeFact::new(
            TypeSubject::MethodReturn(FullyQualifiedName::method(namespace, method)),
            return_type,
            range,
            TypeProvenance::Yard,
        ));
    }

    pub(in crate::indexer::fact_collector) fn push_direct_dynamic_definition_block_return_type(
        &mut self,
        node: &CallNode,
        namespace: Vec<RubyConstant>,
    ) {
        let name_index = if matches!(
            node.name().as_slice(),
            b"send" | b"public_send" | b"__send__"
        ) {
            1
        } else {
            0
        };
        let Some((name, range)) = define_method_name_and_range(self, node, name_index) else {
            return;
        };
        let Ok(method) = RubyMethod::new(&name) else {
            return;
        };
        let subject = TypeSubject::MethodReturn(FullyQualifiedName::method(namespace, method));
        if self
            .facts
            .types
            .facts_for(&subject)
            .iter()
            .any(|fact| fact.range == range)
        {
            return;
        }
        let Some(return_type) = self.infer_call_block_return_type(node) else {
            return;
        };
        let fact = TypeFact::new(subject, return_type, range, TypeProvenance::Inferred);
        self.facts.types.add(fact.clone());
        self.facts.direct.types.push(fact);
    }
}
