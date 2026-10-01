//! ActiveSupport `delegate` and Forwardable `def_delegator(s)` method facts.

use crate::core::{FullyQualifiedName, MethodFact, RubyMethod, TypeFact, TypeSubject};
use crate::engine::AnalysisQuery;
use log::trace;
use ruby_prism::CallNode;

use crate::inference::method::method_call_return_type;

use super::names::direct_attr_name_and_range;
use crate::indexer::fact_collector::FactCollector;

impl FactCollector {
    pub(super) fn push_direct_delegate_method_facts(&mut self, node: &CallNode) {
        let Some((methods, receiver_method)) = delegate_methods_and_receiver(self, node) else {
            return;
        };
        let namespace = self.scope_tracker.get_ns_stack();
        let owner_kind = self.scope_tracker.current_macro_definition_context();
        let range = self.direct_range(&node.location());

        let receiver_type = {
            let engine = self.semantics.engine.read();
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
            self.facts.direct.symbols.push(crate::core::SymbolFact::new(
                fqn.clone(),
                crate::core::SymbolKind::Method,
                range,
            ));
            self.push_direct_method_fact(MethodFact::with_delegate_receiver(
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
                let engine = self.semantics.engine.read();
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
            self.facts.types.add(TypeFact::new(
                TypeSubject::MethodReturn(delegated_fqn),
                return_type,
                range,
                crate::core::TypeProvenance::Inferred,
            ));
        }
    }

    pub(super) fn push_direct_forwardable_delegate_method_facts(&mut self, node: &CallNode) {
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
            let engine = self.semantics.engine.read();
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
            self.facts.direct.symbols.push(crate::core::SymbolFact::new(
                fqn.clone(),
                crate::core::SymbolKind::Method,
                range,
            ));
            self.push_direct_method_fact(MethodFact::with_delegate_receiver(
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
                let engine = self.semantics.engine.read();
                let query = AnalysisQuery::new(&engine);
                method_call_return_type(Some(&query), receiver_type, target_method.as_str())
            };
            let Some(return_type) = return_type else {
                continue;
            };
            self.facts.types.add(TypeFact::new(
                TypeSubject::MethodReturn(fqn),
                return_type,
                range,
                crate::core::TypeProvenance::Inferred,
            ));
        }
    }
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
