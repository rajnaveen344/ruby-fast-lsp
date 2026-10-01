//! Direct class-body macros: visibility, attributes, aliases, module functions, and mixins.

use crate::core::MethodVisibility;
use crate::core::{
    FullyQualifiedName, GraphEdgeKind, MethodFact, MethodParamFact, MethodParamKind,
    MethodReferenceAccess, NamespaceKind, ReferenceCandidate, RubyMethod, TypeFact, TypeSubject,
};
use ruby_prism::CallNode;

use super::names::{
    define_method_name_and_range, direct_attr_name_and_range, direct_method_name_and_range,
    direct_symbol_name_and_range,
};
use crate::indexer::fact_collector::FactCollector;

impl FactCollector {
    pub(super) fn push_direct_visibility_modifier(
        &mut self,
        node: &CallNode,
        visibility: MethodVisibility,
    ) {
        let Some(arguments) = node.arguments() else {
            self.direct_set_visibility(visibility);
            return;
        };
        if arguments.arguments().iter().next().is_none() {
            self.direct_set_visibility(visibility);
            return;
        }

        for arg in arguments.arguments().iter() {
            let Some((name, range)) = direct_method_name_and_range(self, &arg) else {
                continue;
            };
            let Ok(method) = RubyMethod::new(&name) else {
                continue;
            };
            self.direct_set_method_visibility(method, visibility, range);
        }
    }

    pub(super) fn push_direct_included_hook_mixin_edges(&mut self, node: &CallNode) {
        if !self.inside_singleton_included_method() {
            return;
        }
        if node.receiver().is_none() {
            return;
        }

        let Some((kind, first_mixin_index)) = included_hook_mixin_call_kind(self, node) else {
            return;
        };
        let Some(arguments) = node.arguments() else {
            return;
        };

        let source = FullyQualifiedName::namespace(self.scope_tracker.get_ns_stack());
        let range = self.direct_range(&node.location());
        for arg in arguments.arguments().iter().skip(first_mixin_index) {
            let Some(mixin_ref) = crate::indexer::mixin_ref_from_node(&arg) else {
                continue;
            };
            self.direct_push_edge(
                source.clone(),
                &mixin_ref.parts,
                mixin_ref.absolute,
                kind,
                range,
            );
        }
    }

    fn inside_singleton_included_method(&self) -> bool {
        if self.scope_tracker.current_method_context() != NamespaceKind::Singleton {
            return false;
        }
        let Some(FullyQualifiedName::Method(_, method)) = self.scope_tracker.current_method_fqn()
        else {
            return false;
        };
        method.as_str() == "included"
    }

    pub(super) fn push_direct_alias_method_fact(&mut self, node: &CallNode) {
        let Some((new_name, old_name)) = call_two_symbol_or_string_args(self, node) else {
            return;
        };
        let Ok(new_method) = RubyMethod::new(&new_name) else {
            return;
        };
        let Ok(old_method) = RubyMethod::new(&old_name) else {
            return;
        };

        if let Some((_old_name, old_range)) = define_method_name_and_range(self, node, 1) {
            self.facts.references.push(ReferenceCandidate::method(
                old_range,
                crate::core::MethodReferenceCandidate {
                    owner: self.scope_tracker.get_ns_stack(),
                    owner_kind: self.scope_tracker.current_macro_definition_context(),
                    method: old_method,
                    is_super: false,
                    access: MethodReferenceAccess::Normal,
                    caller: self.scope_tracker.current_method_fqn().cloned(),
                    call_expression_range: None,
                    preferred_definition_range: None,
                    diagnostics: crate::core::MethodReferenceDiagnostics {
                        diagnostic_range: old_range,
                        receiver_label: None,
                        receiver_expression_range: None,
                        receiver_type: None,
                        diagnose_unresolved: false,
                        allow_unindexed_owner: false,
                        signature: None,
                    },
                },
            ));
        }

        let namespace = self.scope_tracker.get_ns_stack();
        let owner_kind = self.scope_tracker.current_macro_definition_context();
        let range = self.direct_range(&node.location());
        self.direct_push_method_fact_with_visibility(
            namespace.clone(),
            owner_kind,
            new_method,
            range,
            self.scope_tracker.current_visibility(),
        );

        let old_fqn = FullyQualifiedName::method(namespace.clone(), old_method);
        let new_fqn = FullyQualifiedName::method(
            namespace,
            RubyMethod::new(&new_name).expect(
                "INVARIANT VIOLATED: alias_method new method became invalid after validation. \
                 This is a bug because the same string was already accepted. \
                 Fix: keep alias_method validation single-sourced.",
            ),
        );
        let old_subject = TypeSubject::MethodReturn(old_fqn);
        let Some(old_type) = self.facts.types.facts_for(&old_subject).into_iter().next() else {
            return;
        };
        self.facts.types.add(TypeFact::new(
            TypeSubject::MethodReturn(new_fqn),
            old_type.ruby_type,
            range,
            old_type.provenance,
        ));
    }

    pub(super) fn push_direct_attr_method_facts(
        &mut self,
        node: &CallNode,
        reader: bool,
        writer: bool,
    ) {
        let Some(arguments) = node.arguments() else {
            return;
        };
        // `attr_*` executes on the current class/module object but defines
        // methods in that lexical owner's macro-definition context. Treating
        // the implicit receiver as the generated owner incorrectly made an
        // ordinary class-body `attr_accessor` a singleton method during cold
        // indexing, allowing a same-named included method to win dispatch.
        let owner_kind = self.scope_tracker.current_macro_definition_context();
        for arg in arguments.arguments().iter() {
            let Some((name, range)) = direct_attr_name_and_range(self, &arg) else {
                continue;
            };
            if reader {
                if let Ok(method) = RubyMethod::new(&name) {
                    self.direct_push_method_fact(
                        self.scope_tracker.get_ns_stack(),
                        owner_kind,
                        method,
                        range,
                        Vec::new(),
                    );
                }
            }
            if writer {
                if let Ok(method) = RubyMethod::new(&format!("{name}=")) {
                    self.direct_push_method_fact(
                        self.scope_tracker.get_ns_stack(),
                        owner_kind,
                        method,
                        range,
                        vec![MethodParamFact::new("value", MethodParamKind::Required)],
                    );
                }
            }
        }
    }

    pub(super) fn push_direct_class_attribute_method_facts(&mut self, node: &CallNode) {
        let Some(arguments) = node.arguments() else {
            return;
        };

        let namespace = node
            .receiver()
            .and_then(|receiver| self.resolve_constant_receiver_namespace(&receiver))
            .unwrap_or_else(|| self.scope_tracker.get_ns_stack());

        for arg in arguments.arguments().iter() {
            let Some((name, range)) = direct_attr_name_and_range(self, &arg) else {
                continue;
            };

            if let Ok(method) = RubyMethod::new(&name) {
                self.direct_push_method_fact(
                    namespace.clone(),
                    NamespaceKind::Singleton,
                    method,
                    range,
                    Vec::new(),
                );
                self.direct_push_method_fact(
                    namespace.clone(),
                    NamespaceKind::Instance,
                    method,
                    range,
                    Vec::new(),
                );
            }

            if let Ok(method) = RubyMethod::new(&format!("{name}=")) {
                let params = vec![MethodParamFact::new("value", MethodParamKind::Required)];
                self.direct_push_method_fact(
                    namespace.clone(),
                    NamespaceKind::Singleton,
                    method,
                    range,
                    params.clone(),
                );
                self.direct_push_method_fact(
                    namespace.clone(),
                    NamespaceKind::Instance,
                    method,
                    range,
                    params,
                );
            }
        }
    }

    pub(super) fn push_direct_module_function_facts(&mut self, node: &CallNode) {
        let Some(arguments) = node.arguments() else {
            self.scope_tracker.enable_module_function_mode();
            return;
        };
        if arguments.arguments().iter().next().is_none() {
            self.scope_tracker.enable_module_function_mode();
            return;
        }
        for arg in arguments.arguments().iter() {
            let Some((name, fallback_range)) = direct_symbol_name_and_range(self, &arg) else {
                continue;
            };
            let Ok(method) = RubyMethod::new(&name) else {
                continue;
            };
            let namespace = self.scope_tracker.get_ns_stack();
            let fqn = FullyQualifiedName::method(namespace.clone(), method);
            let instance_owner =
                FullyQualifiedName::namespace_with_kind(namespace.clone(), NamespaceKind::Instance);
            let range = self
                .facts
                .direct
                .methods
                .iter()
                .find(|fact| fact.fqn == fqn && fact.owner == instance_owner)
                .map(|fact| fact.range)
                .unwrap_or(fallback_range);
            let owner =
                FullyQualifiedName::namespace_with_kind(namespace, NamespaceKind::Singleton);
            self.push_direct_method_fact(MethodFact::new(fqn, owner, range));
        }
    }

    pub(super) fn push_direct_mixin_edges(&mut self, node: &CallNode, kind: GraphEdgeKind) {
        let Some(arguments) = node.arguments() else {
            return;
        };
        let source = FullyQualifiedName::namespace(self.scope_tracker.get_ns_stack());
        let in_singleton = self.scope_tracker.in_singleton();
        let source_for_edge = if in_singleton {
            source.to_singleton_namespace().expect(
                "INVARIANT VIOLATED: singleton class mixin source could not convert to singleton namespace. \
                 This is a bug because class << self can only appear inside a namespace. \
                 Fix: guard singleton mixin indexing to namespace scopes.",
            )
        } else {
            source.clone()
        };
        let range = self.direct_range(&node.location());
        for arg in arguments.arguments().iter() {
            let mixin_ref = if arg.as_self_node().is_some() {
                Some((source.namespace_parts(), true)).filter(|(parts, _)| !parts.is_empty())
            } else {
                crate::indexer::mixin_ref_from_node(&arg).map(|mixin| (mixin.parts, mixin.absolute))
            };
            if let Some((parts, absolute)) = mixin_ref {
                self.direct_push_edge(source_for_edge.clone(), &parts, absolute, kind, range);
                if kind == GraphEdgeKind::Extend && !in_singleton {
                    if let Some(source_singleton) = source.to_singleton_namespace() {
                        self.direct_push_edge(
                            source_singleton,
                            &parts,
                            absolute,
                            GraphEdgeKind::Include,
                            range,
                        );
                    }
                }
            }
        }
    }
}

fn included_hook_mixin_call_kind(
    visitor: &FactCollector,
    node: &CallNode<'_>,
) -> Option<(GraphEdgeKind, usize)> {
    match node.name().as_slice() {
        b"include" => Some((GraphEdgeKind::Include, 0)),
        b"extend" => Some((GraphEdgeKind::Extend, 0)),
        b"send" | b"public_send" | b"__send__" => {
            let arguments = node.arguments()?;
            let first = arguments.arguments().iter().next()?;
            let (selector, _) = direct_attr_name_and_range(visitor, &first)?;
            match selector.as_str() {
                "include" => Some((GraphEdgeKind::Include, 1)),
                "extend" => Some((GraphEdgeKind::Extend, 1)),
                _ => None,
            }
        }
        _ => None,
    }
}

fn call_two_symbol_or_string_args(
    visitor: &FactCollector,
    node: &CallNode<'_>,
) -> Option<(String, String)> {
    let arguments = node.arguments()?;
    let args = arguments.arguments();
    let mut iter = args.iter();
    let (new_name, _) = direct_attr_name_and_range(visitor, &iter.next()?)?;
    let (old_name, _) = direct_attr_name_and_range(visitor, &iter.next()?)?;
    Some((new_name, old_name))
}
