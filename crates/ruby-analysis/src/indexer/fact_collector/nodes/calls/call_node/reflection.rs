//! Reflective constant (`const_get`) and method (`method`, `instance_method`) references.

use crate::core::{
    MethodReferenceAccess, NamespaceKind, ReferenceCandidate, RubyConstant, RubyMethod,
};
use crate::indexer::mixin_ref_from_node;
use ruby_prism::{CallNode, Node};

use super::names::define_method_name_and_range;
use crate::indexer::fact_collector::FactCollector;

impl FactCollector {
    pub(super) fn push_const_lookup_reference_candidate(&mut self, node: &CallNode) {
        let Some((parts, range)) = self.const_lookup_target_parts_and_range(node) else {
            return;
        };
        self.facts
            .analysis
            .reference_candidates
            .push(ReferenceCandidate::constant(range, parts, Vec::new()));
    }

    fn const_lookup_target_parts_and_range(
        &self,
        node: &CallNode<'_>,
    ) -> Option<(Vec<RubyConstant>, crate::core::TextRange)> {
        if !matches!(node.name().as_slice(), b"const_get" | b"const_defined?") {
            return None;
        }
        let (name, range) = define_method_name_and_range(self, node, 0)?;
        let constant = RubyConstant::new(&name).ok()?;
        let mut parts = match node.receiver() {
            Some(receiver) if receiver.as_self_node().is_some() => {
                self.scope_tracker.get_ns_stack()
            }
            Some(receiver) => self.const_lookup_base_namespace_parts(&receiver)?,
            None => self.scope_tracker.get_ns_stack(),
        };
        parts.push(constant);
        Some((parts, range))
    }

    fn const_lookup_base_namespace_parts(&self, receiver: &Node<'_>) -> Option<Vec<RubyConstant>> {
        if let Some((parts, _range)) = receiver
            .as_call_node()
            .and_then(|call| self.const_lookup_target_parts_and_range(&call))
        {
            return Some(parts);
        }

        let receiver_ref = mixin_ref_from_node(receiver)?;
        if let Some(fqn) = self.direct_resolve_namespace(&receiver_ref.parts, receiver_ref.absolute)
        {
            return Some(fqn.namespace_parts());
        }

        let context = if receiver_ref.absolute {
            Vec::new()
        } else {
            self.scope_tracker.get_ns_stack()
        };
        if let Some(fqn) = self.resolve_constant_from_analysis(&receiver_ref.parts, &context) {
            return Some(fqn.namespace_parts());
        }

        Some(receiver_ref.parts)
    }

    pub(super) fn push_reflected_method_reference_candidate(&mut self, node: &CallNode) {
        let Some((method_name, range)) = reflected_method_name_and_range(self, node) else {
            return;
        };
        let Ok(method) = RubyMethod::new(&method_name) else {
            return;
        };
        let current_namespace = self.scope_tracker.get_ns_stack();
        let (owner, owner_kind) = match node.name().as_slice() {
            b"method" => match node.receiver() {
                Some(receiver_node) => {
                    let (owner, kind, _receiver_info, _inferred_expr_type) =
                        self.handle_receiver_node_with_info(&receiver_node, &current_namespace);
                    (owner, kind)
                }
                None => (
                    current_namespace,
                    self.scope_tracker.current_method_context(),
                ),
            },
            b"instance_method" => match node.receiver() {
                Some(receiver_node) => {
                    let (owner, _kind, _receiver_info, _inferred_expr_type) =
                        self.handle_receiver_node_with_info(&receiver_node, &current_namespace);
                    (owner, NamespaceKind::Instance)
                }
                None => (current_namespace, NamespaceKind::Instance),
            },
            b"delegate" | b"def_delegator" | b"def_delegators" | b"class_attribute"
            | b"attr_reader" | b"attr_writer" | b"attr_accessor" | b"module_function"
            | b"alias_method" | b"define_method" | b"include" | b"prepend" | b"extend"
            | b"send" | b"public_send" | b"__send__" => return,
            _ => return,
        };

        self.facts
            .analysis
            .reference_candidates
            .push(ReferenceCandidate::method(
                range,
                crate::core::MethodReferenceCandidate {
                    owner,
                    owner_kind,
                    method,
                    is_super: false,
                    access: if node.name().as_slice() == b"instance_method" {
                        MethodReferenceAccess::InstanceMethodReflection
                    } else {
                        MethodReferenceAccess::Normal
                    },
                    caller: self.scope_tracker.current_method_fqn().cloned(),
                    call_expression_range: None,
                    preferred_definition_range: None,
                    diagnostics: crate::core::MethodReferenceDiagnostics {
                        diagnostic_range: range,
                        receiver_label: None,
                        receiver_expression_range: None,
                        receiver_type: None,
                        diagnose_unresolved: false,
                        allow_unindexed_owner: false,
                        safe_navigation: false,
                        signature: None,
                    },
                },
            ));
    }
}

fn reflected_method_name_and_range(
    visitor: &FactCollector,
    node: &CallNode<'_>,
) -> Option<(String, crate::core::TextRange)> {
    match node.name().as_slice() {
        b"method" | b"instance_method" => {}
        _ => return None,
    }

    let arguments = node.arguments()?;
    let arg = arguments.arguments().iter().next()?;
    if let Some(symbol) = arg.as_symbol_node() {
        let location = symbol.value_loc().unwrap_or_else(|| symbol.location());
        return Some((
            String::from_utf8_lossy(symbol.unescaped()).to_string(),
            visitor.direct_range(&location),
        ));
    }
    if let Some(string) = arg.as_string_node() {
        return Some((
            String::from_utf8_lossy(string.unescaped()).to_string(),
            visitor.direct_range(&string.content_loc()),
        ));
    }
    None
}
