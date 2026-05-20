use log::trace;
use ruby_analysis_core::{
    DiagnosticCandidate, DiagnosticCandidateKind, KeywordArgCandidate,
    MethodCallSignatureCandidate, NamespaceKind, RaiseArgCandidate, ReferenceCandidate,
};
use ruby_prism::{CallNode, Node};

use crate::{
    analyzer_prism::{diagnostics::bad_splat::BadSplatCandidate, diagnostics::ReceiverInfo, utils},
    types::{
        fully_qualified_name::FullyQualifiedName, ruby_method::RubyMethod,
        ruby_namespace::RubyConstant,
    },
};
use ruby_analysis_inference::RubyType;

use super::FactCollector;

impl FactCollector {
    pub fn process_call_node_entry(&mut self, node: &CallNode) {
        let extension_registry = self.extension_registry.clone();
        extension_registry.process_call_node(self, node);
        self.extension_call_stack
            .push(crate::extensions::resolved_call_for_stack(self, node));

        self.process_call_reference_candidate(node);
    }

    fn process_call_reference_candidate(&mut self, node: &CallNode) {
        let method_name = utils::utf8_str(node.name().as_slice());
        if !RubyMethod::is_valid_ruby_method_name(method_name) {
            trace!("Skipping method call with invalid name: {}", method_name);
            return;
        }

        let call_location = self
            .document
            .prism_location_to_lsp_location(&node.location());
        let message_location = node
            .message_loc()
            .map(|loc| self.document.prism_location_to_lsp_location(&loc))
            .unwrap_or_else(|| call_location.clone());
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
        ) && target_namespace == current_namespace;

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
            self.reference_candidates.push(ReferenceCandidate::method(
                self.text_range_from_lsp_range(call_location.range, "method reference candidate"),
                target_namespace,
                namespace_kind,
                method,
                self.scope_tracker.current_method_fqn().cloned(),
                self.text_range_from_lsp_range(
                    message_location.range,
                    "method diagnostic candidate",
                ),
                receiver_label,
                !matches!(receiver_info, ReceiverInfo::SelfReceiver),
                self.method_call_signature_candidate(node),
            ));
        }

        if method_name == "raise" && node.receiver().is_none() {
            if let Some(candidate) = self.raise_non_exception_candidate(node) {
                self.diagnostic_candidates.push(candidate);
            }
        }

        for entry in crate::analyzer_prism::diagnostics::bad_splat::check(node, &self.document) {
            let candidate = self.bad_splat_candidate(entry);
            self.diagnostic_candidates.push(candidate);
        }
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
            let name = utils::utf8_str(constant_read.name().as_slice()).to_string();
            let (ns, kind) = self.handle_constant_read_receiver(&constant_read, current_namespace);
            (ns, kind, ReceiverInfo::ConstantReceiver(name), None)
        } else if let Some(constant_path) = receiver_node.as_constant_path_node() {
            if self.is_valid_constant_path_receiver(receiver_node) {
                let receiver_name = utils::build_constant_path_name(receiver_node);
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
    ) -> (Vec<RubyConstant>, NamespaceKind) {
        let name = utils::utf8_str(constant_read.name().as_slice());
        if let Ok(constant) = RubyConstant::new(name) {
            let mut receiver_namespace = current_namespace.to_vec();
            receiver_namespace.push(constant);
            (receiver_namespace, NamespaceKind::Singleton)
        } else {
            (current_namespace.to_vec(), NamespaceKind::Instance)
        }
    }

    fn handle_constant_path_receiver(
        &self,
        _constant_path: &ruby_prism::ConstantPathNode,
        receiver_node: &Node,
        current_namespace: &[RubyConstant],
    ) -> (Vec<RubyConstant>, NamespaceKind) {
        if let Some(mixin_ref) = utils::mixin_ref_from_node(receiver_node) {
            let context = if mixin_ref.absolute {
                Vec::new()
            } else {
                current_namespace.to_vec()
            };
            if let Some(resolved_fqn) =
                self.resolve_constant_from_analysis(&mixin_ref.parts, &context)
            {
                return (resolved_fqn.namespace_parts(), NamespaceKind::Singleton);
            }
        }

        if let Some(mixin_ref) = utils::mixin_ref_from_node(receiver_node) {
            let final_namespace = if mixin_ref.absolute {
                mixin_ref.parts
            } else {
                self.resolve_relative_constant_path(&mixin_ref.parts, current_namespace)
            };
            (final_namespace, NamespaceKind::Singleton)
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
        if let Some(local_var) = receiver_node.as_local_variable_read_node() {
            let var_name = utils::utf8_str(local_var.name().as_slice());
            if let Some(ty) = self.get_local_var_type(var_name, &local_var.location()) {
                return Some(ty);
            }
            return self.infer_variable_type_cached(var_name);
        }

        if let Some(call) = receiver_node.as_call_node() {
            let inner_method = utils::utf8_str(call.name().as_slice());
            let inner_type = if let Some(inner_receiver) = call.receiver() {
                if let Some(constant_read) = inner_receiver.as_constant_read_node() {
                    let name = utils::utf8_str(constant_read.name().as_slice());
                    Some(RubyType::ClassReference(FullyQualifiedName::Constant(
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
                    Some(RubyType::Class(FullyQualifiedName::Constant(ns)))
                }
            }?;

            return self.resolve_method_return_type(&inner_type, inner_method);
        }

        Some(self.infer_type_from_value(receiver_node)).filter(|ty| *ty != RubyType::Unknown)
    }

    fn type_to_namespace_parts(&self, ruby_type: &RubyType) -> Option<Vec<RubyConstant>> {
        match ruby_type {
            RubyType::Class(fqn) | RubyType::Module(fqn) => Some(fqn.namespace_parts()),
            _ => None,
        }
    }

    fn resolve_constant_from_analysis(
        &self,
        parts: &[RubyConstant],
        current_namespace: &[RubyConstant],
    ) -> Option<FullyQualifiedName> {
        let engine = self.analysis_engine.lock();
        ruby_analysis_engine::AnalysisQuery::new(&engine)
            .resolve_constant_in_context(parts, current_namespace)
    }

    fn resolve_relative_constant_path(
        &self,
        parts: &[RubyConstant],
        current_namespace: &[RubyConstant],
    ) -> Vec<RubyConstant> {
        if let Some(first_part) = parts.first() {
            let mut resolved = None;
            for i in (0..=current_namespace.len()).rev() {
                let test_namespace = &current_namespace[..i];
                if test_namespace
                    .iter()
                    .any(|c| c.to_string() == first_part.to_string())
                {
                    if let Some(pos) = test_namespace
                        .iter()
                        .position(|c| c.to_string() == first_part.to_string())
                    {
                        let mut result = test_namespace[..=pos].to_vec();
                        result.extend(parts.iter().skip(1).cloned());
                        resolved = Some(result);
                        break;
                    }
                }
            }

            resolved.unwrap_or_else(|| {
                if current_namespace.len() >= 2 {
                    let parent_ns = &current_namespace[..current_namespace.len() - 1];
                    if parent_ns.last().map(|c| c.to_string()) == Some(first_part.to_string()) {
                        let mut result = parent_ns.to_vec();
                        result.extend(parts.iter().cloned());
                        return result;
                    }
                }

                let mut ns = current_namespace.to_vec();
                ns.extend(parts.iter().cloned());
                ns
            })
        } else {
            current_namespace.to_vec()
        }
    }

    fn is_valid_constant_path_receiver(&self, node: &Node) -> bool {
        if node.as_constant_read_node().is_some() {
            return true;
        }

        if let Some(constant_path) = node.as_constant_path_node() {
            if let Some(parent) = constant_path.parent() {
                return self.is_valid_constant_path_receiver(&parent);
            }
            return true;
        }

        false
    }

    fn method_call_signature_candidate(&self, node: &CallNode) -> MethodCallSignatureCandidate {
        let mut signature = MethodCallSignatureCandidate::default();
        let Some(args) = node.arguments() else {
            return signature;
        };

        for arg in args.arguments().iter() {
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
                    let name = utils::utf8_str(value_loc.as_slice()).to_string();
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
            RaiseArgCandidate::Constant(utils::utf8_str(const_read.name().as_slice()).to_string())
        } else if first_arg.as_constant_path_node().is_some() {
            let full_name = utils::build_constant_path_name(&first_arg);
            let last_segment = full_name
                .split("::")
                .last()
                .unwrap_or(&full_name)
                .to_string();
            RaiseArgCandidate::Constant(last_segment)
        } else if let Some(local) = first_arg.as_local_variable_read_node() {
            let var_name = utils::utf8_str(local.name().as_slice());
            let var_pos = self
                .document
                .offset_to_position(first_arg.location().start_offset());
            let scopes = self.document.variable_scopes();
            let scope_id = scopes
                .find_scope_for_variable_at(var_name, var_pos)
                .or_else(|| scopes.scope_at_position(var_pos));
            if let Some(scope_id) = scope_id {
                if let Some(ty) = scopes.get_type_at_position(var_name, scope_id, var_pos) {
                    RaiseArgCandidate::Type(ty.clone())
                } else {
                    RaiseArgCandidate::Unknown
                }
            } else {
                RaiseArgCandidate::Unknown
            }
        } else if let Some(inner_call) = first_arg.as_call_node() {
            if inner_call.receiver().is_none() {
                let method_name = utils::utf8_str(inner_call.name().as_slice());
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
        self.extension_call_stack.pop().expect(
            "INVARIANT VIOLATED: extension call stack underflow in FactCollector. \
             This is a bug because every call-node entry must push exactly one stack frame. \
             Fix: keep process_call_node_entry/process_call_node_exit balanced.",
        );
    }
}
