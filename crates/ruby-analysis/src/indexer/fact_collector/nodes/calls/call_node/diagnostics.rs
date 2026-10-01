//! Call-shape signature, `raise`, and bad-splat diagnostic candidates.

use crate::core::{
    DiagnosticCandidate, DiagnosticCandidateKind, KeywordArgCandidate,
    MethodCallSignatureCandidate, RaiseArgCandidate, RubyMethod,
};
use crate::indexer::{build_constant_path_name, utf8_str};
use ruby_prism::{CallNode, Node};

use super::super::bad_splat::BadSplatCandidate;

use crate::indexer::fact_collector::FactCollector;

impl FactCollector {
    pub(super) fn method_call_signature_candidate(
        &self,
        node: &CallNode,
        skip_positional_args: usize,
    ) -> MethodCallSignatureCandidate {
        let Some(args) = node.arguments() else {
            return MethodCallSignatureCandidate::default();
        };

        self.method_signature_candidate_from_arguments(
            args.arguments().iter(),
            skip_positional_args,
        )
    }

    pub(in crate::indexer::fact_collector) fn method_signature_candidate_from_arguments<'prism>(
        &self,
        arguments: impl Iterator<Item = Node<'prism>>,
        skip_positional_args: usize,
    ) -> MethodCallSignatureCandidate {
        let mut signature = MethodCallSignatureCandidate::default();

        for (index, arg) in arguments.enumerate() {
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
                    signature.has_nonempty_keyword_hash = true;
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
            signature.trailing_positional_may_be_options_hash =
                positional_argument_may_be_options_hash(&arg);
        }

        signature
    }

    pub(super) fn raise_non_exception_candidate(
        &self,
        node: &CallNode,
    ) -> Option<DiagnosticCandidate> {
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
            if self.scope_tracker.current_method_fqn().is_some() {
                RaiseArgCandidate::LocalRead(range)
            } else {
                let var_name = utf8_str(local.name().as_slice());
                let byte_offset = range.start_byte;
                let file_id = self.document.analysis_file_id();
                let scopes = self.document.variable_scopes();
                let scope_id = scopes
                    .find_scope_for_variable_at(var_name, file_id, byte_offset)
                    .or_else(|| scopes.scope_at_position(file_id, byte_offset));
                scope_id
                    .and_then(|scope_id| {
                        scopes.get_type_at_position(var_name, scope_id, file_id, byte_offset)
                    })
                    .cloned()
                    .map(RaiseArgCandidate::Type)
                    .unwrap_or(RaiseArgCandidate::Unknown)
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

    pub(super) fn bad_splat_candidate(&self, entry: BadSplatCandidate) -> DiagnosticCandidate {
        DiagnosticCandidate::new(
            entry.location,
            DiagnosticCandidateKind::BadSplat {
                operator: entry.operator,
                arg_repr: entry.arg_repr,
                expected: entry.expected,
            },
        )
    }
}

fn positional_argument_may_be_options_hash(arg: &Node<'_>) -> bool {
    if arg.as_hash_node().is_some() {
        return true;
    }

    // These literal forms are conclusively not Hash values. Every other
    // expression remains a possible options hash until type inference proves
    // otherwise; call-shape diagnostics must not guess its runtime value.
    !(arg.as_string_node().is_some()
        || arg.as_integer_node().is_some()
        || arg.as_float_node().is_some()
        || arg.as_array_node().is_some()
        || arg.as_symbol_node().is_some()
        || arg.as_true_node().is_some()
        || arg.as_false_node().is_some()
        || arg.as_nil_node().is_some()
        || arg.as_range_node().is_some()
        || arg.as_regular_expression_node().is_some())
}
