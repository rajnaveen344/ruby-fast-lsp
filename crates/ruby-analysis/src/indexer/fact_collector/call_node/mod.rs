use crate::core::method_store::MethodVisibility;
use crate::core::{
    DiagnosticCandidate, DiagnosticCandidateKind, FullyQualifiedName, GraphEdgeKind,
    KeywordArgCandidate, MethodCallSignatureCandidate, MethodFact, MethodParamFact,
    MethodParamKind, MethodReferenceAccess, NamespaceKind, RaiseArgCandidate, ReferenceCandidate,
    RubyConstant, RubyMethod, TypeFact, TypeProvenance, TypeSubject,
};
use crate::engine::{AnalysisQuery, VariableTypeKind};
use crate::{build_constant_path_name, mixin_ref_from_node, utf8_str};
use log::trace;
use ruby_prism::{CallNode, Node};

use super::bad_splat::BadSplatCandidate;
use crate::inference::method::{
    method_call_return_type, rbs_class_exists_for_type, rbs_method_exists_for_type,
};
use crate::inference::RubyType;
use crate::yard::YardTypeConverter;

use super::FactCollector;

#[derive(Debug, Clone)]
enum ReceiverInfo {
    NoReceiver,
    SelfReceiver,
    ConstantReceiver(String),
    ExpressionReceiver,
    InvalidConstantPath,
}

impl FactCollector {
    pub fn process_call_node_entry(&mut self, node: &CallNode) {
        let extension_host = self.extension_host.clone();
        extension_host.process_call_node(self, node);

        let track_call = extension_host.should_track_enclosing_call(self, node);
        self.extension_call_stack_marks.push(track_call);
        if track_call {
            let resolved_call = extension_host.resolved_call_for_stack(self, node);
            self.extension_call_stack.push(resolved_call);
        }

        let direct_call_handled = self.process_direct_call_facts(node);
        if !direct_call_handled {
            self.process_call_reference_candidate(node);
        }
    }

    fn process_direct_call_facts(&mut self, node: &CallNode) -> bool {
        self.push_direct_included_hook_mixin_edges(node);

        if node.receiver().is_some() && node.name().as_slice() == b"class_attribute" {
            self.push_direct_class_attribute_method_facts(node);
        }

        if node.receiver().is_some() {
            self.push_direct_send_define_method_fact(node);
            return false;
        }

        match node.name().as_slice() {
            b"attr_reader" => {
                self.push_direct_attr_method_facts(node, true, false);
                true
            }
            b"attr_writer" => {
                self.push_direct_attr_method_facts(node, false, true);
                true
            }
            b"attr_accessor" => {
                self.push_direct_attr_method_facts(node, true, true);
                true
            }
            b"class_attribute" => {
                self.push_direct_class_attribute_method_facts(node);
                true
            }
            b"private" => {
                self.push_direct_visibility_modifier(node, MethodVisibility::Private);
                true
            }
            b"protected" => {
                self.push_direct_visibility_modifier(node, MethodVisibility::Protected);
                true
            }
            b"public" => {
                self.push_direct_visibility_modifier(node, MethodVisibility::Public);
                true
            }
            b"module_function" => {
                self.push_direct_module_function_facts(node);
                true
            }
            b"alias_method" => {
                self.push_direct_alias_method_fact(node);
                true
            }
            b"define_method" => {
                self.push_direct_define_method_fact(node);
                true
            }
            b"delegate" => {
                self.push_direct_delegate_method_facts(node);
                true
            }
            b"def_delegator" | b"def_delegators" => {
                self.push_direct_forwardable_delegate_method_facts(node);
                true
            }
            b"include" => {
                self.push_direct_mixin_edges(node, GraphEdgeKind::Include);
                true
            }
            b"prepend" => {
                self.push_direct_mixin_edges(node, GraphEdgeKind::Prepend);
                true
            }
            b"extend" => {
                self.push_direct_mixin_edges(node, GraphEdgeKind::Extend);
                true
            }
            _ => false,
        }
    }

    fn push_direct_visibility_modifier(&mut self, node: &CallNode, visibility: MethodVisibility) {
        let Some(arguments) = node.arguments() else {
            self.direct_set_visibility(visibility);
            return;
        };
        if arguments.arguments().iter().next().is_none() {
            self.direct_set_visibility(visibility);
            return;
        }

        for arg in arguments.arguments().iter() {
            let Some((name, range)) = direct_attr_name_and_range(self, &arg) else {
                continue;
            };
            let Ok(method) = RubyMethod::new(&name) else {
                continue;
            };
            self.direct_set_method_visibility(method, visibility, range);
        }
    }

    fn push_direct_included_hook_mixin_edges(&mut self, node: &CallNode) {
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
            let Some(mixin_ref) = crate::mixin_ref_from_node(&arg) else {
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

    fn push_direct_define_method_fact(&mut self, node: &CallNode) {
        let Some((name, range)) = define_method_name_and_range(self, node, 0) else {
            return;
        };
        let Ok(method) = RubyMethod::new(&name) else {
            return;
        };
        let namespace = self.scope_tracker.get_ns_stack();
        let owner_kind = self.scope_tracker.current_macro_definition_context();
        self.direct_push_method_fact(namespace.clone(), owner_kind, method, range, Vec::new());
        self.push_direct_define_method_return_type(namespace, method, range, node);
    }

    fn push_direct_send_define_method_fact(&mut self, node: &CallNode) {
        if node.name().as_slice() != b"send" {
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
        if selector != "define_method" {
            return;
        }
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
        self.direct_push_method_fact(
            namespace.clone(),
            NamespaceKind::Instance,
            method,
            range,
            Vec::new(),
        );
        self.push_direct_define_method_return_type(namespace, method, range, node);
    }

    fn resolve_constant_receiver_namespace(
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
            if self.direct_known_namespaces.contains(&fqn) {
                return Some(candidate);
            }
            if receiver_ref.absolute || search.is_empty() {
                break;
            }
            search.pop();
        }

        let fqn = FullyQualifiedName::namespace(receiver_ref.parts.clone());
        self.direct_known_namespaces
            .contains(&fqn)
            .then_some(receiver_ref.parts)
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
        self.direct_known_namespaces
            .contains(&fqn)
            .then_some(namespace)
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
        self.type_store.add(TypeFact::new(
            TypeSubject::MethodReturn(FullyQualifiedName::method(namespace, method)),
            return_type,
            range,
            TypeProvenance::Yard,
        ));
    }

    fn push_direct_alias_method_fact(&mut self, node: &CallNode) {
        let Some((new_name, old_name)) = call_two_symbol_or_string_args(self, node) else {
            return;
        };
        let Ok(new_method) = RubyMethod::new(&new_name) else {
            return;
        };
        let Ok(old_method) = RubyMethod::new(&old_name) else {
            return;
        };

        let namespace = self.scope_tracker.get_ns_stack();
        let owner_kind = self.scope_tracker.current_macro_definition_context();
        let range = self.direct_range(&node.location());
        self.direct_push_method_fact(namespace.clone(), owner_kind, new_method, range, Vec::new());

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
        let Some(old_type) = self.type_store.facts_for(&old_subject).into_iter().next() else {
            return;
        };
        self.type_store.add(TypeFact::new(
            TypeSubject::MethodReturn(new_fqn),
            old_type.ruby_type,
            range,
            old_type.provenance,
        ));
    }

    fn push_direct_delegate_method_facts(&mut self, node: &CallNode) {
        let Some((methods, receiver_method)) = delegate_methods_and_receiver(self, node) else {
            return;
        };
        let namespace = self.scope_tracker.get_ns_stack();
        let owner_kind = self.scope_tracker.current_macro_definition_context();
        let range = self.direct_range(&node.location());

        let receiver_type = {
            let engine = self.analysis_engine.read();
            let query = AnalysisQuery::new(&engine);
            let owner = FullyQualifiedName::namespace_with_kind(namespace.clone(), owner_kind);
            let Ok(method) = RubyMethod::new(&receiver_method) else {
                return;
            };
            query.method_return_type_for_receiver(&owner, &method)
        };

        for method_name in methods {
            let Ok(method) = RubyMethod::new(&method_name) else {
                continue;
            };
            let fqn = FullyQualifiedName::method(namespace.clone(), method);
            let owner = FullyQualifiedName::namespace_with_kind(namespace.clone(), owner_kind);
            self.direct_facts.symbols.push(crate::core::SymbolFact::new(
                fqn.clone(),
                crate::core::SymbolKind::Method,
                range,
            ));
            self.direct_facts
                .methods
                .push(MethodFact::with_delegate_receiver(
                    fqn,
                    owner,
                    range,
                    RubyMethod::new(&receiver_method).expect(
                        "INVARIANT VIOLATED: delegate receiver method became invalid after validation. \
                         This is a bug because the same string was already accepted. \
                         Fix: keep delegate receiver validation single-sourced.",
                    ),
                ));

            let Some(receiver_type) = receiver_type.as_ref() else {
                continue;
            };
            let return_type = {
                let engine = self.analysis_engine.read();
                let query = AnalysisQuery::new(&engine);
                method_call_return_type(Some(&query), receiver_type, &method_name)
            };
            let Some(return_type) = return_type else {
                continue;
            };
            let delegated_fqn = FullyQualifiedName::method(
                namespace.clone(),
                RubyMethod::new(&method_name).expect(
                    "INVARIANT VIOLATED: delegate method became invalid after validation. \
                     This is a bug because the same string was already accepted. \
                     Fix: keep delegate method validation single-sourced.",
                ),
            );
            self.type_store.add(TypeFact::new(
                TypeSubject::MethodReturn(delegated_fqn),
                return_type,
                range,
                crate::core::TypeProvenance::Inferred,
            ));
        }
    }

    fn push_direct_forwardable_delegate_method_facts(&mut self, node: &CallNode) {
        let Some((receiver_method, methods)) = forwardable_delegates_and_receiver(self, node)
        else {
            return;
        };
        let namespace = self.scope_tracker.get_ns_stack();
        let owner_kind = self.scope_tracker.current_macro_definition_context();
        let range = self.direct_range(&node.location());
        let Ok(receiver_method) = RubyMethod::new(&receiver_method) else {
            return;
        };

        let receiver_type = {
            let engine = self.analysis_engine.read();
            let query = AnalysisQuery::new(&engine);
            let owner = FullyQualifiedName::namespace_with_kind(namespace.clone(), owner_kind);
            query.method_return_type_for_receiver(&owner, &receiver_method)
        };

        for (defined_name, target_name) in methods {
            let Ok(method) = RubyMethod::new(&defined_name) else {
                continue;
            };
            let fqn = FullyQualifiedName::method(namespace.clone(), method);
            let owner = FullyQualifiedName::namespace_with_kind(namespace.clone(), owner_kind);
            self.direct_facts.symbols.push(crate::core::SymbolFact::new(
                fqn.clone(),
                crate::core::SymbolKind::Method,
                range,
            ));
            self.direct_facts
                .methods
                .push(MethodFact::with_delegate_receiver(
                    fqn.clone(),
                    owner,
                    range,
                    receiver_method,
                ));

            let (Some(receiver_type), Ok(target_method)) =
                (receiver_type.as_ref(), RubyMethod::new(&target_name))
            else {
                continue;
            };
            let return_type = {
                let engine = self.analysis_engine.read();
                let query = AnalysisQuery::new(&engine);
                method_call_return_type(Some(&query), receiver_type, target_method.as_str())
            };
            let Some(return_type) = return_type else {
                continue;
            };
            self.type_store.add(TypeFact::new(
                TypeSubject::MethodReturn(fqn),
                return_type,
                range,
                crate::core::TypeProvenance::Inferred,
            ));
        }
    }

    fn push_direct_attr_method_facts(&mut self, node: &CallNode, reader: bool, writer: bool) {
        let Some(arguments) = node.arguments() else {
            return;
        };
        let owner_kind = self.scope_tracker.current_method_context();
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

    fn push_direct_class_attribute_method_facts(&mut self, node: &CallNode) {
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

    fn push_direct_module_function_facts(&mut self, node: &CallNode) {
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
                .direct_facts
                .methods
                .iter()
                .find(|fact| fact.fqn == fqn && fact.owner == instance_owner)
                .map(|fact| fact.range)
                .unwrap_or(fallback_range);
            let owner =
                FullyQualifiedName::namespace_with_kind(namespace, NamespaceKind::Singleton);
            self.direct_facts
                .methods
                .push(MethodFact::new(fqn, owner, range));
        }
    }

    fn push_direct_mixin_edges(&mut self, node: &CallNode, kind: GraphEdgeKind) {
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
                crate::mixin_ref_from_node(&arg).map(|mixin| (mixin.parts, mixin.absolute))
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

    fn process_call_reference_candidate(&mut self, node: &CallNode) {
        self.push_const_lookup_reference_candidate(node);
        self.push_reflected_method_reference_candidate(node);

        let static_send_target = static_send_target_name_and_range(self, node);
        let method_name = static_send_target
            .as_ref()
            .map(|(name, _range)| name.as_str())
            .unwrap_or_else(|| utf8_str(node.name().as_slice()));
        if !RubyMethod::is_valid_ruby_method_name(method_name) {
            trace!("Skipping method call with invalid name: {}", method_name);
            return;
        }

        let call_range =
            self.text_range_from_prism_location(&node.location(), "method reference candidate");
        let message_range = static_send_target
            .as_ref()
            .map(|(_name, range)| *range)
            .or_else(|| {
                node.message_loc().map(|loc| {
                    self.text_range_from_prism_location(&loc, "method diagnostic candidate")
                })
            })
            .unwrap_or(call_range);
        let current_namespace = self.scope_tracker.get_ns_stack();
        let (target_namespace, namespace_kind, receiver_info, inferred_expr_type) =
            match node.receiver() {
                Some(receiver_node) => {
                    self.handle_receiver_node_with_info(&receiver_node, &current_namespace)
                }
                None => {
                    let (ns, kind) = self.handle_no_receiver(&current_namespace);
                    (ns, kind, ReceiverInfo::NoReceiver, None)
                }
            };

        let inference_failed = matches!(
            receiver_info,
            ReceiverInfo::ExpressionReceiver | ReceiverInfo::InvalidConstantPath
        ) && inferred_expr_type.is_none()
            && target_namespace == current_namespace;

        let method = match RubyMethod::new(method_name) {
            Ok(method) => method,
            Err(err) => {
                trace!("Failed to create RubyMethod for '{}': {}", method_name, err);
                return;
            }
        };

        if !inference_failed {
            let receiver_label = match (&receiver_info, inferred_expr_type.as_ref()) {
                (ReceiverInfo::ConstantReceiver(name), _) => Some(name.clone()),
                (
                    ReceiverInfo::ExpressionReceiver | ReceiverInfo::InvalidConstantPath,
                    Some(ruby_type),
                ) => Some(ruby_type.to_string()),
                (ReceiverInfo::NoReceiver | ReceiverInfo::SelfReceiver, _)
                | (ReceiverInfo::ExpressionReceiver | ReceiverInfo::InvalidConstantPath, None) => {
                    None
                }
            };
            let signature = if self.diagnostics_enabled {
                self.method_call_signature_candidate(
                    node,
                    usize::from(static_send_target.is_some()),
                )
            } else {
                crate::core::MethodCallSignatureCandidate::default()
            };
            let access =
                method_reference_access(node, &receiver_info, static_send_target.is_some());
            let rbs_resolves_method = inferred_expr_type.as_ref().is_some_and(|ruby_type| {
                rbs_method_exists_for_type(ruby_type, &method, namespace_kind)
            });
            let rbs_receiver_class_exists = inferred_expr_type
                .as_ref()
                .is_some_and(rbs_class_exists_for_type);
            self.reference_candidates.push(ReferenceCandidate::method(
                message_range,
                crate::core::MethodReferenceCandidate {
                    owner: target_namespace,
                    owner_kind: namespace_kind,
                    method,
                    is_super: false,
                    access,
                    caller: self.scope_tracker.current_method_fqn().cloned(),
                    diagnostics: crate::core::MethodReferenceDiagnostics {
                        diagnostic_range: message_range,
                        receiver_label,
                        diagnose_unresolved: self.diagnostics_enabled
                            && !rbs_resolves_method
                            && !matches!(receiver_info, ReceiverInfo::SelfReceiver),
                        allow_unindexed_owner: rbs_receiver_class_exists,
                        signature,
                    },
                },
            ));
        }

        if self.diagnostics_enabled && method_name == "raise" && node.receiver().is_none() {
            if let Some(candidate) = self.raise_non_exception_candidate(node) {
                self.diagnostic_candidates.push(candidate);
            }
        }

        if self.diagnostics_enabled {
            for entry in super::bad_splat::check(node, &self.document) {
                let candidate = self.bad_splat_candidate(entry);
                self.diagnostic_candidates.push(candidate);
            }
        }
    }

    fn push_const_lookup_reference_candidate(&mut self, node: &CallNode) {
        let Some((parts, range)) = self.const_lookup_target_parts_and_range(node) else {
            return;
        };
        self.reference_candidates
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

    fn push_reflected_method_reference_candidate(&mut self, node: &CallNode) {
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

        self.reference_candidates.push(ReferenceCandidate::method(
            range,
            crate::core::MethodReferenceCandidate {
                owner,
                owner_kind,
                method,
                is_super: false,
                access: MethodReferenceAccess::Normal,
                caller: self.scope_tracker.current_method_fqn().cloned(),
                diagnostics: crate::core::MethodReferenceDiagnostics {
                    diagnostic_range: range,
                    receiver_label: None,
                    diagnose_unresolved: false,
                    allow_unindexed_owner: false,
                    signature: crate::core::MethodCallSignatureCandidate::default(),
                },
            },
        ));
    }

    fn handle_no_receiver(
        &self,
        current_namespace: &[RubyConstant],
    ) -> (Vec<RubyConstant>, NamespaceKind) {
        (
            current_namespace.to_vec(),
            self.scope_tracker.current_method_context(),
        )
    }

    fn handle_receiver_node_with_info(
        &self,
        receiver_node: &Node,
        current_namespace: &[RubyConstant],
    ) -> (
        Vec<RubyConstant>,
        NamespaceKind,
        ReceiverInfo,
        Option<RubyType>,
    ) {
        if receiver_node.as_self_node().is_some() {
            (
                current_namespace.to_vec(),
                NamespaceKind::Instance,
                ReceiverInfo::SelfReceiver,
                None,
            )
        } else if let Some(constant_read) = receiver_node.as_constant_read_node() {
            let name = utf8_str(constant_read.name().as_slice()).to_string();
            let (ns, kind, inferred) =
                self.handle_constant_read_receiver(&constant_read, current_namespace);
            (ns, kind, ReceiverInfo::ConstantReceiver(name), inferred)
        } else if let Some(constant_path) = receiver_node.as_constant_path_node() {
            if is_valid_constant_path_receiver(receiver_node) {
                let receiver_name = build_constant_path_name(receiver_node);
                let (ns, kind) = self.handle_constant_path_receiver(
                    &constant_path,
                    receiver_node,
                    current_namespace,
                );
                (
                    ns,
                    kind,
                    ReceiverInfo::ConstantReceiver(receiver_name),
                    None,
                )
            } else {
                let (ns, kind, inferred) =
                    self.handle_expression_receiver(receiver_node, current_namespace);
                (ns, kind, ReceiverInfo::InvalidConstantPath, inferred)
            }
        } else {
            let (ns, kind, inferred) =
                self.handle_expression_receiver(receiver_node, current_namespace);
            (ns, kind, ReceiverInfo::ExpressionReceiver, inferred)
        }
    }

    fn handle_constant_read_receiver(
        &self,
        constant_read: &ruby_prism::ConstantReadNode,
        current_namespace: &[RubyConstant],
    ) -> (Vec<RubyConstant>, NamespaceKind, Option<RubyType>) {
        let name = utf8_str(constant_read.name().as_slice());
        if let Ok(constant) = RubyConstant::new(name) {
            let mut lexical_namespace = current_namespace.to_vec();
            let value_type = loop {
                let mut parts = lexical_namespace.clone();
                parts.push(constant.clone());
                let constant_fqn = FullyQualifiedName::constant(parts);
                let namespace_fqn = FullyQualifiedName::namespace(constant_fqn.namespace_parts());
                let is_namespace = self
                    .direct_facts
                    .graph_nodes
                    .iter()
                    .any(|fact| fact.fqn == namespace_fqn)
                    || {
                        let engine = self.analysis_engine.read();
                        !AnalysisQuery::new(&engine)
                            .graph_nodes_for(&namespace_fqn)
                            .is_empty()
                    };
                if is_namespace {
                    return (
                        constant_fqn.namespace_parts(),
                        NamespaceKind::Singleton,
                        None,
                    );
                }
                if let Some(ruby_type) = self.direct_constant_value_type(&constant_fqn) {
                    break Some(ruby_type);
                }
                if lexical_namespace.pop().is_none() {
                    break None;
                }
            }
            .or_else(|| {
                let engine = self.analysis_engine.read();
                let query = AnalysisQuery::new(&engine);
                query
                    .resolve_constant_in_context(std::slice::from_ref(&constant), current_namespace)
                    .and_then(|resolved| {
                        query.constant_value_type(&FullyQualifiedName::constant(
                            resolved.namespace_parts(),
                        ))
                    })
            });
            if let Some(ref ruby_type) = value_type {
                if let Some(namespace) = self.type_to_namespace_parts(ruby_type) {
                    return (namespace, NamespaceKind::Instance, value_type);
                }
            }
            let mut receiver_namespace = current_namespace.to_vec();
            receiver_namespace.push(constant);
            (receiver_namespace, NamespaceKind::Singleton, None)
        } else {
            (current_namespace.to_vec(), NamespaceKind::Instance, None)
        }
    }

    fn handle_constant_path_receiver(
        &self,
        _constant_path: &ruby_prism::ConstantPathNode,
        receiver_node: &Node,
        current_namespace: &[RubyConstant],
    ) -> (Vec<RubyConstant>, NamespaceKind) {
        if let Some(mixin_ref) = mixin_ref_from_node(receiver_node) {
            let context = if mixin_ref.absolute {
                Vec::new()
            } else {
                current_namespace.to_vec()
            };
            if let Some(resolved_fqn) =
                self.direct_resolve_namespace(&mixin_ref.parts, mixin_ref.absolute)
            {
                return (resolved_fqn.namespace_parts(), NamespaceKind::Singleton);
            }
            if let Some(resolved_fqn) =
                self.resolve_constant_from_analysis(&mixin_ref.parts, &context)
            {
                return (resolved_fqn.namespace_parts(), NamespaceKind::Singleton);
            }
        }

        if let Some(mixin_ref) = mixin_ref_from_node(receiver_node) {
            (mixin_ref.parts, NamespaceKind::Singleton)
        } else {
            (current_namespace.to_vec(), NamespaceKind::Instance)
        }
    }

    fn handle_expression_receiver(
        &self,
        receiver_node: &Node,
        current_namespace: &[RubyConstant],
    ) -> (Vec<RubyConstant>, NamespaceKind, Option<RubyType>) {
        let inferred = self.infer_expression_receiver_type(receiver_node);
        if let Some(ref resolved_type) = inferred {
            if let Some(ns) = self.type_to_namespace_parts(resolved_type) {
                return (ns, NamespaceKind::Instance, Some(resolved_type.clone()));
            }
        }

        (
            current_namespace.to_vec(),
            NamespaceKind::Instance,
            inferred,
        )
    }

    fn infer_expression_receiver_type(&self, receiver_node: &Node) -> Option<RubyType> {
        if !self.infer_expression_receivers {
            return None;
        }

        if let Some(local_var) = receiver_node.as_local_variable_read_node() {
            let var_name = utf8_str(local_var.name().as_slice());
            if let Some(ty) = self.get_local_var_type(var_name, &local_var.location()) {
                return Some(ty);
            }
            return self.infer_variable_type_cached(var_name);
        }

        if let Some(ivar) = receiver_node.as_instance_variable_read_node() {
            let var_name = utf8_str(ivar.name().as_slice());
            let byte_offset = u32::try_from(ivar.location().start_offset()).expect(
                "INVARIANT VIOLATED: Prism location offset exceeded u32. \
                 This is a bug because ruby-analysis::core TextRange currently stores u32 offsets. \
                 Fix: widen TextRange offsets before indexing files larger than u32::MAX bytes.",
            );
            let engine = self.analysis_engine.read();
            return AnalysisQuery::new(&engine).variable_type_before(
                VariableTypeKind::Instance,
                var_name,
                self.document.analysis_file_id(),
                byte_offset,
            );
        }

        if let Some(call) = receiver_node.as_call_node() {
            let inner_method = utf8_str(call.name().as_slice());
            let inner_type = if let Some(inner_receiver) = call.receiver() {
                if let Some(constant_read) = inner_receiver.as_constant_read_node() {
                    let name = utf8_str(constant_read.name().as_slice());
                    Some(RubyType::ClassReference(FullyQualifiedName::constant(
                        vec![RubyConstant::new(name).ok()?],
                    )))
                } else {
                    self.infer_expression_receiver_type(&inner_receiver)
                }
            } else {
                let ns = self.scope_tracker.get_ns_stack();
                if ns.is_empty() {
                    None
                } else {
                    Some(RubyType::Class(FullyQualifiedName::constant(ns)))
                }
            }?;

            return self.resolve_method_return_type(&inner_type, inner_method);
        }

        Some(self.infer_type_from_value(receiver_node)).filter(|ty| *ty != RubyType::Unknown)
    }

    fn type_to_namespace_parts(&self, ruby_type: &RubyType) -> Option<Vec<RubyConstant>> {
        let engine = self.analysis_engine.read();
        AnalysisQuery::new(&engine)
            .type_to_namespace(ruby_type)
            .map(|namespace| namespace.namespace_parts())
    }

    fn resolve_constant_from_analysis(
        &self,
        parts: &[RubyConstant],
        current_namespace: &[RubyConstant],
    ) -> Option<FullyQualifiedName> {
        let engine = self.analysis_engine.read();
        crate::engine::AnalysisQuery::new(&engine)
            .resolve_constant_in_context(parts, current_namespace)
    }

    fn method_call_signature_candidate(
        &self,
        node: &CallNode,
        skip_positional_args: usize,
    ) -> MethodCallSignatureCandidate {
        let mut signature = MethodCallSignatureCandidate::default();
        let Some(args) = node.arguments() else {
            return signature;
        };

        for (index, arg) in args.arguments().iter().enumerate() {
            if index < skip_positional_args {
                continue;
            }
            if arg.as_forwarding_arguments_node().is_some() {
                signature.has_positional_splat = true;
                signature.has_keyword_splat = true;
                continue;
            }
            if arg.as_splat_node().is_some() {
                signature.has_positional_splat = true;
                continue;
            }
            if let Some(keyword_hash) = arg.as_keyword_hash_node() {
                for elem in keyword_hash.elements().iter() {
                    if elem.as_assoc_splat_node().is_some() {
                        signature.has_keyword_splat = true;
                        continue;
                    }
                    let Some(assoc) = elem.as_assoc_node() else {
                        continue;
                    };
                    let Some(symbol) = assoc.key().as_symbol_node() else {
                        continue;
                    };
                    let Some(value_loc) = symbol.value_loc() else {
                        continue;
                    };
                    let name = utf8_str(value_loc.as_slice()).to_string();
                    signature.keyword_args.push(KeywordArgCandidate {
                        name,
                        range: self.text_range_from_prism_location(
                            &value_loc,
                            "keyword argument candidate",
                        ),
                    });
                }
                continue;
            }
            if arg.as_block_argument_node().is_some() {
                continue;
            }
            signature.positional_count += 1;
        }

        signature
    }

    fn raise_non_exception_candidate(&self, node: &CallNode) -> Option<DiagnosticCandidate> {
        let args = node.arguments()?;
        let first_arg = args.arguments().iter().next()?;
        let arg_repr = String::from_utf8_lossy(first_arg.location().as_slice()).to_string();
        let range = self.text_range_from_prism_location(&first_arg.location(), "raise argument");

        let arg = if first_arg.as_string_node().is_some() {
            RaiseArgCandidate::StringLiteral
        } else if first_arg.as_integer_node().is_some()
            || first_arg.as_float_node().is_some()
            || first_arg.as_array_node().is_some()
            || first_arg.as_hash_node().is_some()
            || first_arg.as_symbol_node().is_some()
            || first_arg.as_true_node().is_some()
            || first_arg.as_false_node().is_some()
            || first_arg.as_nil_node().is_some()
            || first_arg.as_range_node().is_some()
        {
            RaiseArgCandidate::NonExceptionLiteral
        } else if let Some(const_read) = first_arg.as_constant_read_node() {
            RaiseArgCandidate::Constant(utf8_str(const_read.name().as_slice()).to_string())
        } else if first_arg.as_constant_path_node().is_some() {
            let full_name = build_constant_path_name(&first_arg);
            let last_segment = full_name
                .split("::")
                .last()
                .unwrap_or(&full_name)
                .to_string();
            RaiseArgCandidate::Constant(last_segment)
        } else if let Some(local) = first_arg.as_local_variable_read_node() {
            let var_name = utf8_str(local.name().as_slice());
            let byte_offset = u32::try_from(first_arg.location().start_offset()).expect(
                "INVARIANT VIOLATED: Prism location offset exceeded u32. \
                 This is a bug because ruby-analysis::core TextRange currently stores u32 offsets. \
                 Fix: widen TextRange offsets before indexing files larger than u32::MAX bytes.",
            );
            let file_id = self.document.analysis_file_id();
            let scopes = self.document.variable_scopes();
            let scope_id = scopes
                .find_scope_for_variable_at(var_name, file_id, byte_offset)
                .or_else(|| scopes.scope_at_position(file_id, byte_offset));
            if let Some(scope_id) = scope_id {
                if let Some(ty) =
                    scopes.get_type_at_position(var_name, scope_id, file_id, byte_offset)
                {
                    RaiseArgCandidate::Type(ty.clone())
                } else {
                    RaiseArgCandidate::Unknown
                }
            } else {
                RaiseArgCandidate::Unknown
            }
        } else if let Some(inner_call) = first_arg.as_call_node() {
            if inner_call.receiver().is_none() {
                let method_name = utf8_str(inner_call.name().as_slice());
                match RubyMethod::new(method_name) {
                    Ok(method) => RaiseArgCandidate::BareMethodReturn {
                        current_namespace: self.scope_tracker.get_ns_stack(),
                        method,
                    },
                    Err(_) => RaiseArgCandidate::Unknown,
                }
            } else {
                RaiseArgCandidate::Unknown
            }
        } else {
            RaiseArgCandidate::Unknown
        };

        Some(DiagnosticCandidate::new(
            range,
            DiagnosticCandidateKind::RaiseNonException { arg_repr, arg },
        ))
    }

    fn bad_splat_candidate(&self, entry: BadSplatCandidate) -> DiagnosticCandidate {
        DiagnosticCandidate::new(
            self.text_range_from_lsp_range(entry.location.range, "bad splat"),
            DiagnosticCandidateKind::BadSplat {
                operator: entry.operator,
                arg_repr: entry.arg_repr,
                expected: entry.expected,
            },
        )
    }

    pub fn process_call_node_exit(&mut self, _node: &CallNode) {
        let tracked = self.extension_call_stack_marks.pop().expect(
            "INVARIANT VIOLATED: extension call stack mark underflow in FactCollector. \
             This is a bug because every call-node entry must push exactly one stack mark. \
             Fix: keep process_call_node_entry/process_call_node_exit balanced.",
        );
        if !tracked {
            return;
        }
        self.extension_call_stack.pop().expect(
            "INVARIANT VIOLATED: extension call stack underflow in FactCollector. \
             This is a bug because every call-node entry must push exactly one stack frame. \
             Fix: keep process_call_node_entry/process_call_node_exit balanced.",
        );
    }
}

fn is_valid_constant_path_receiver(node: &Node) -> bool {
    if node.as_constant_read_node().is_some() {
        return true;
    }

    if let Some(constant_path) = node.as_constant_path_node() {
        if let Some(parent) = constant_path.parent() {
            return is_valid_constant_path_receiver(&parent);
        }
        return true;
    }

    false
}

fn direct_attr_name_and_range(
    visitor: &FactCollector,
    node: &Node<'_>,
) -> Option<(String, crate::core::TextRange)> {
    if let Some(symbol) = node.as_symbol_node() {
        return Some((
            String::from_utf8_lossy(symbol.unescaped()).to_string(),
            visitor.direct_range(&symbol.location()),
        ));
    }
    if let Some(string) = node.as_string_node() {
        return Some((
            String::from_utf8_lossy(string.unescaped()).to_string(),
            visitor.direct_range(&string.content_loc()),
        ));
    }
    None
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

fn define_method_name_and_range(
    visitor: &FactCollector,
    node: &CallNode<'_>,
    name_index: usize,
) -> Option<(String, crate::core::TextRange)> {
    let arguments = node.arguments()?;
    let arg = arguments.arguments().iter().nth(name_index)?;
    if let Some(symbol) = arg.as_symbol_node() {
        let location = symbol.value_loc().unwrap_or_else(|| symbol.location());
        return Some((
            String::from_utf8_lossy(symbol.unescaped()).to_string(),
            visitor.direct_range(&location),
        ));
    }
    direct_attr_name_and_range(visitor, &arg)
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

fn static_send_target_name_and_range(
    visitor: &FactCollector,
    node: &CallNode<'_>,
) -> Option<(String, crate::core::TextRange)> {
    match node.name().as_slice() {
        b"send" | b"public_send" | b"__send__" => {}
        _ => return None,
    }

    let (name, range) = define_method_name_and_range(visitor, node, 0)?;
    if name == "define_method" {
        return None;
    }
    RubyMethod::is_valid_ruby_method_name(&name).then_some((name, range))
}

fn method_reference_access(
    node: &CallNode<'_>,
    receiver_info: &ReceiverInfo,
    static_send_target: bool,
) -> MethodReferenceAccess {
    if static_send_target {
        return match node.name().as_slice() {
            b"send" | b"__send__" => MethodReferenceAccess::VisibilityBypass,
            b"public_send" => MethodReferenceAccess::ExplicitReceiver,
            other => panic!(
                "INVARIANT VIOLATED: static send target came from unsupported call `{}`. \
                 This is a bug because only send/public_send/__send__ calls expose a reflected target. \
                 Fix: keep static_send_target_name_and_range and method_reference_access in sync.",
                String::from_utf8_lossy(other)
            ),
        };
    }

    match receiver_info {
        ReceiverInfo::NoReceiver => MethodReferenceAccess::Normal,
        ReceiverInfo::SelfReceiver
        | ReceiverInfo::ConstantReceiver(_)
        | ReceiverInfo::ExpressionReceiver
        | ReceiverInfo::InvalidConstantPath => MethodReferenceAccess::ExplicitReceiver,
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

fn delegate_methods_and_receiver(
    visitor: &FactCollector,
    node: &CallNode<'_>,
) -> Option<(Vec<String>, String)> {
    let arguments = node.arguments()?;
    let mut methods = Vec::new();
    let mut receiver = None;
    for arg in arguments.arguments().iter() {
        if let Some(keyword_hash) = arg.as_keyword_hash_node() {
            for element in keyword_hash.elements().iter() {
                let Some(assoc) = element.as_assoc_node() else {
                    continue;
                };
                let Some((key, _)) = direct_attr_name_and_range(visitor, &assoc.key()) else {
                    continue;
                };
                if key.trim_end_matches(':') == "to" {
                    receiver =
                        direct_attr_name_and_range(visitor, &assoc.value()).map(|(name, _)| name);
                }
            }
        } else if let Some((name, _range)) = direct_attr_name_and_range(visitor, &arg) {
            methods.push(name);
        }
    }

    let receiver = receiver?;
    (!methods.is_empty()).then_some((methods, receiver))
}

fn forwardable_delegates_and_receiver(
    visitor: &FactCollector,
    node: &CallNode<'_>,
) -> Option<(String, Vec<(String, String)>)> {
    let arguments = node.arguments()?;
    let mut args = arguments.arguments().iter();
    let (receiver, _) = direct_attr_name_and_range(visitor, &args.next()?)?;
    let mut methods = Vec::new();

    match node.name().as_slice() {
        b"def_delegators" => {
            for arg in args {
                let Some((name, _)) = direct_attr_name_and_range(visitor, &arg) else {
                    continue;
                };
                methods.push((name.clone(), name));
            }
        }
        b"def_delegator" => {
            let (target_name, _) = direct_attr_name_and_range(visitor, &args.next()?)?;
            let defined_name = args
                .next()
                .and_then(|arg| direct_attr_name_and_range(visitor, &arg).map(|(name, _)| name))
                .unwrap_or_else(|| target_name.clone());
            methods.push((defined_name, target_name));
        }
        b"delegate" => return None,
        b"alias_method" => return None,
        b"define_method" => return None,
        b"module_function" => return None,
        b"attr_reader" => return None,
        b"attr_writer" => return None,
        b"attr_accessor" => return None,
        b"include" => return None,
        b"prepend" => return None,
        b"extend" => return None,
        b"send" | b"public_send" | b"__send__" => return None,
        other => {
            trace!(
                "Skipping non-Forwardable delegate macro: {}",
                String::from_utf8_lossy(other)
            );
            return None;
        }
    }

    (!methods.is_empty()).then_some((receiver, methods))
}

fn direct_symbol_name_and_range(
    visitor: &FactCollector,
    node: &Node<'_>,
) -> Option<(String, crate::core::TextRange)> {
    node.as_symbol_node().map(|symbol| {
        (
            String::from_utf8_lossy(symbol.unescaped()).to_string(),
            visitor.direct_range(&symbol.location()),
        )
    })
}
