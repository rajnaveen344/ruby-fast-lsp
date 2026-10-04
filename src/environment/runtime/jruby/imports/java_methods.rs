//! Java method selection for `java_alias`, `java_send`/`java_method`
//! dispatch, and proxy constructors.

use super::call_host::{CallHostStat, CALL_HOST_STATS};
use super::java_types::{display_java_signature, ruby_type_for_jvm};
use super::JrubyImportProvider;
use crate::invariant::ExpectInvariant;
use ruby_analysis::core::{
    FullyQualifiedName, MethodParamFact, MethodParamKind, MethodReferenceAccess,
    MethodReferenceCandidate, MethodReferenceDiagnostics, NamespaceKind, ReferenceCandidate,
    RubyConstant, RubyMethod, RubyType, TextRange, TypeFact, TypeProvenance, TypeSubject,
};
use ruby_analysis::indexer::fact_collector::FactCollector;
use ruby_fast_lsp_jruby_support::syntax::static_symbol_or_string;
use ruby_fast_lsp_jruby_support::JavaClassName;
use ruby_fast_lsp_jvm_metadata::{
    parse_method_descriptor, JvmType, MemberInfo, MethodDescriptor, Visibility,
};
use ruby_prism::{CallNode, Node};
use std::collections::{BTreeSet, VecDeque};

const MAX_JAVA_HIERARCHY_TYPES: usize = 4_096;

impl JrubyImportProvider {
    pub(super) fn process_java_alias_call(&self, visitor: &mut FactCollector, node: &CallNode<'_>) {
        if node.receiver().is_some() || node.name().as_slice() != b"java_alias" {
            return;
        }
        let Some(arguments) = node.arguments() else {
            return;
        };
        let mut arguments = arguments.arguments().iter();
        let Some(new_name_node) = arguments.next() else {
            return;
        };
        let Some(old_name_node) = arguments.next() else {
            return;
        };
        let Some(new_name) = static_symbol_or_string(&new_name_node) else {
            return;
        };
        let Some(old_name) = static_symbol_or_string(&old_name_node) else {
            return;
        };
        let Some(new_method) = RubyMethod::new(&new_name).ok() else {
            return;
        };
        let Some(old_method) = RubyMethod::new(&old_name).ok() else {
            return;
        };
        let signature = if let Some(signature_node) = arguments.next() {
            if arguments.next().is_some() {
                return;
            }
            let Some(signature) = self.static_java_signature(visitor, &signature_node) else {
                visitor.push_warning_diagnostic(
                    visitor.text_range_from_offsets(
                        signature_node.location().start_offset(),
                        signature_node.location().end_offset(),
                    ),
                    "unsupported-jruby-java-alias",
                    "java_alias parameter types must be a static array of Java primitive or fully qualified class names.".to_string(),
                );
                return;
            };
            Some(signature)
        } else {
            None
        };

        let current_namespace =
            FullyQualifiedName::namespace(visitor.scope_tracker().get_ns_stack());
        let Some(proxy) = current_runtime_proxy(visitor).or_else(|| {
            self.proxy_to_internal
                .contains_key(&current_namespace.to_string())
                .then_some(current_namespace)
        }) else {
            return;
        };
        let proxy_name = proxy.to_string();
        let Some(internal_names) = self.proxy_to_internal.get(&proxy_name) else {
            return;
        };
        if internal_names.len() != 1 {
            visitor.push_error_diagnostic(
                visitor.text_range_from_offsets(
                    node.location().start_offset(),
                    node.location().end_offset(),
                ),
                "ambiguous-java-proxy",
                format!(
                    "Java proxy `{proxy_name}` maps to multiple classpath identities: {}.",
                    internal_names.join(", ")
                ),
            );
            return;
        }
        let declaration = self
            .catalog
            .classes
            .get(&internal_names[0])
            .expect_invariant(
                "Java proxy reverse index points at a missing catalog class",
                "both structures are built atomically from the same catalog",
                "keep JrubyImportProvider::new reverse-index construction synchronized",
            );
        let matching = declaration
            .class
            .methods
            .iter()
            .filter(|method| {
                *method.name == *old_name
                    && !method.is_static()
                    && signature.as_ref().is_none_or(|expected| {
                        parse_method_descriptor(&method.descriptor)
                            .is_ok_and(|descriptor| descriptor.parameters == *expected)
                    })
            })
            .collect::<Vec<_>>();
        if matching.is_empty() {
            visitor.push_error_diagnostic(
                visitor.text_range_from_offsets(
                    old_name_node.location().start_offset(),
                    old_name_node.location().end_offset(),
                ),
                "unresolved-java-method-alias",
                format!(
                    "Java method `{old_name}` with the selected parameter signature is not present on `{proxy_name}`."
                ),
            );
            return;
        }

        let range = visitor
            .text_range_from_offsets(node.location().start_offset(), node.location().end_offset());
        let name_range = visitor.text_range_from_offsets(
            new_name_node.location().start_offset(),
            new_name_node.location().end_offset(),
        );
        let old_name_range = visitor.text_range_from_offsets(
            old_name_node.location().start_offset(),
            old_name_node.location().end_offset(),
        );
        for method in matching {
            self.push_java_alias_method(
                visitor,
                &proxy,
                new_method,
                old_method,
                method,
                range,
                name_range,
                old_name_range,
            );
        }
    }

    pub(super) fn process_java_dispatch_call(
        &self,
        visitor: &mut FactCollector,
        node: &CallNode<'_>,
    ) {
        if !matches!(node.name().as_slice(), b"java_send" | b"java_method") {
            return;
        }
        let Some(receiver) = node.receiver() else {
            return;
        };
        let dispatch_name = if node.name().as_slice() == b"java_send" {
            "java_send"
        } else {
            "java_method"
        };
        let call_range = visitor
            .text_range_from_offsets(node.location().start_offset(), node.location().end_offset());
        let Some((proxy, receiver_kind)) = self.runtime_proxy_for_expression(visitor, &receiver)
        else {
            return;
        };
        let Some(arguments) = node.arguments() else {
            visitor.push_warning_diagnostic(
                call_range,
                "unsupported-jruby-java-dispatch",
                format!(
                    "{dispatch_name} requires a static Java method name and an optional static parameter-type array."
                ),
            );
            return;
        };
        let arguments = arguments.arguments().iter().collect::<Vec<_>>();
        let Some(method_name_node) = arguments.first() else {
            visitor.push_warning_diagnostic(
                call_range,
                "unsupported-jruby-java-dispatch",
                format!(
                    "{dispatch_name} requires a static Java method name and an optional static parameter-type array."
                ),
            );
            return;
        };
        let Some(method_name) = static_symbol_or_string(method_name_node) else {
            visitor.push_warning_diagnostic(
                visitor.text_range_from_offsets(
                    method_name_node.location().start_offset(),
                    method_name_node.location().end_offset(),
                ),
                "unsupported-jruby-java-dispatch",
                format!("{dispatch_name} method names must be static symbols or strings."),
            );
            return;
        };
        let Ok(ruby_method) = RubyMethod::new(&method_name) else {
            visitor.push_error_diagnostic(
                visitor.text_range_from_offsets(
                    method_name_node.location().start_offset(),
                    method_name_node.location().end_offset(),
                ),
                "invalid-java-method-name",
                format!("`{method_name}` cannot be represented as a Ruby method name."),
            );
            return;
        };

        let (signature, actual_argument_count) = match arguments.get(1) {
            Some(signature_node) => {
                let Some(signature) = self.static_java_signature(visitor, signature_node) else {
                    visitor.push_warning_diagnostic(
                        visitor.text_range_from_offsets(
                            signature_node.location().start_offset(),
                            signature_node.location().end_offset(),
                        ),
                        "unsupported-jruby-java-dispatch",
                        format!(
                            "{dispatch_name} parameter types must be a static array of Java primitive, imported, canonical, or fully qualified class names."
                        ),
                    );
                    return;
                };
                (signature, arguments.len().saturating_sub(2))
            }
            None => (Vec::new(), 0),
        };
        if dispatch_name == "java_method" && arguments.len() > 2 {
            visitor.push_error_diagnostic(
                call_range,
                "invalid-java-method-handle",
                "java_method accepts only a method name and parameter-type array.".to_string(),
            );
            return;
        }
        if dispatch_name == "java_send" && actual_argument_count != signature.len() {
            visitor.push_error_diagnostic(
                call_range,
                "invalid-java-method-arguments",
                format!(
                    "java_send selected {} Java parameter(s) but received {actual_argument_count} argument(s).",
                    signature.len()
                ),
            );
            return;
        }

        let candidates = match self.java_method_candidates(&proxy, &method_name, &signature) {
            Ok(candidates) => candidates,
            Err(message) => {
                visitor.push_error_diagnostic(call_range, "invalid-java-hierarchy", message);
                return;
            }
        };
        let candidates = candidates
            .into_iter()
            .filter(|candidate| match (dispatch_name, receiver_kind) {
                ("java_send", NamespaceKind::Instance) => !candidate.method.is_static(),
                ("java_send", NamespaceKind::Singleton) => candidate.method.is_static(),
                ("java_method", NamespaceKind::Instance) => !candidate.method.is_static(),
                ("java_method", NamespaceKind::Singleton) => true,
                (_, NamespaceKind::Instance | NamespaceKind::Singleton) => false,
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            visitor.push_error_diagnostic(
                visitor.text_range_from_offsets(
                    method_name_node.location().start_offset(),
                    method_name_node.location().end_offset(),
                ),
                "unresolved-java-method",
                format!(
                    "Java method `{method_name}` with parameter signature `{}` is not present on `{proxy}` for this receiver.",
                    display_java_signature(&signature)
                ),
            );
            return;
        }
        if candidates.len() != 1 {
            visitor.push_error_diagnostic(
                visitor.text_range_from_offsets(
                    method_name_node.location().start_offset(),
                    method_name_node.location().end_offset(),
                ),
                "ambiguous-java-method",
                format!(
                    "Java method `{method_name}` with parameter signature `{}` resolves to {} declarations on `{proxy}`.",
                    display_java_signature(&signature),
                    candidates.len()
                ),
            );
            return;
        }
        let selected = &candidates[0];
        let owner = JavaClassName::parse(&selected.owner).expect_invariant(
            "selected Java method owner is not a valid internal class name",
            "java catalog construction validates every class identity",
            "retain the canonical catalog key as the selected method owner",
        );
        let owner_parts = owner
            .ruby_namespace_parts()
            .into_iter()
            .map(|part| {
                RubyConstant::new(&part).expect_invariant(
                    "validated Java method owner is not Ruby-constant-safe",
                    "JavaClassName owns proxy validation",
                    "keep Java method reference owner conversion single-sourced",
                )
            })
            .collect::<Vec<_>>();
        let method_range = visitor.text_range_from_offsets(
            method_name_node.location().start_offset(),
            method_name_node.location().end_offset(),
        );
        visitor.add_reference_candidate(ReferenceCandidate::method(
            method_range,
            MethodReferenceCandidate {
                owner: owner_parts,
                owner_kind: if selected.method.is_static() {
                    NamespaceKind::Singleton
                } else {
                    NamespaceKind::Instance
                },
                method: ruby_method,
                is_super: false,
                access: MethodReferenceAccess::VisibilityBypass,
                caller: visitor.scope_tracker().current_method_fqn().cloned(),
                call_expression_range: None,
                preferred_definition_range: self
                    .preferred_method_definition_range(&selected.owner, &selected.method),
                diagnostics: MethodReferenceDiagnostics {
                    diagnostic_range: method_range,
                    receiver_label: Some(proxy.to_string()),
                    receiver_expression_range: None,
                    receiver_type: None,
                    diagnose_unresolved: false,
                    allow_unindexed_owner: false,
                    safe_navigation: false,
                    signature: None,
                },
            },
        ));

        let return_type = if dispatch_name == "java_send" {
            ruby_type_for_jvm(&selected.descriptor.returns)
        } else if receiver_kind == NamespaceKind::Singleton && !selected.method.is_static() {
            RubyType::Class(
                FullyQualifiedName::try_from("UnboundMethod").expect_invariant(
                    "built-in UnboundMethod FQN is invalid",
                    "it is a static Ruby core constant",
                    "keep built-in runtime type names valid Ruby constants",
                ),
            )
        } else {
            RubyType::Class(FullyQualifiedName::try_from("Method").expect_invariant(
                "built-in Method FQN is invalid",
                "it is a static Ruby core constant",
                "keep built-in runtime type names valid Ruby constants",
            ))
        };
        visitor.direct_push_expression_type(&node.as_node(), return_type, TypeProvenance::Runtime);
    }

    pub(super) fn process_java_constructor_call(
        &self,
        visitor: &mut FactCollector,
        node: &CallNode<'_>,
    ) {
        if node.name().as_slice() != b"new" {
            return;
        }
        let Some(receiver) = node.receiver() else {
            return;
        };
        let Some((proxy, NamespaceKind::Singleton)) =
            self.runtime_proxy_for_expression(visitor, &receiver)
        else {
            return;
        };
        CALL_HOST_STATS.increment(CallHostStat::JavaCtorInferred);
        visitor.direct_push_expression_type(
            &node.as_node(),
            RubyType::Class(proxy),
            TypeProvenance::Runtime,
        );
    }

    fn runtime_proxy_for_expression(
        &self,
        visitor: &FactCollector,
        node: &Node<'_>,
    ) -> Option<(FullyQualifiedName, NamespaceKind)> {
        let (proxy, kind) = match visitor.infer_type_from_value(node) {
            RubyType::Class(proxy) | RubyType::Module(proxy) => (proxy, NamespaceKind::Instance),
            RubyType::ClassReference(proxy) | RubyType::ModuleReference(proxy) => {
                (proxy, NamespaceKind::Singleton)
            }
            RubyType::Literal(_)
            | RubyType::Array(_)
            | RubyType::Hash(_, _)
            | RubyType::Shape(_)
            | RubyType::Union(_)
            | RubyType::Unknown => return None,
        };
        self.proxy_to_internal
            .contains_key(&proxy.to_string())
            .then_some((proxy, kind))
    }

    fn java_method_candidates(
        &self,
        proxy: &FullyQualifiedName,
        method_name: &str,
        signature: &[JvmType],
    ) -> Result<Vec<SelectedJavaMethod>, String> {
        let Some(roots) = self.proxy_to_internal.get(&proxy.to_string()) else {
            return Ok(Vec::new());
        };
        if roots.len() != 1 {
            return Err(format!(
                "Java proxy `{proxy}` maps to multiple classpath identities: {}.",
                roots.join(", ")
            ));
        }
        let mut queue = VecDeque::from([(roots[0].clone(), 0usize)]);
        let mut visited = BTreeSet::new();
        let mut identities = BTreeSet::new();
        let mut selected = Vec::new();
        while let Some((owner, depth)) = queue.pop_front() {
            if !visited.insert(owner.clone()) {
                continue;
            }
            if visited.len() > MAX_JAVA_HIERARCHY_TYPES {
                return Err(format!(
                    "Java hierarchy for `{proxy}` exceeds the bounded limit of {MAX_JAVA_HIERARCHY_TYPES} types."
                ));
            }
            let Some(declaration) = self.catalog.classes.get(&owner) else {
                continue;
            };
            for method in &declaration.class.methods {
                if &*method.name != method_name || method.visibility() != Visibility::Public {
                    continue;
                }
                let Ok(descriptor) = parse_method_descriptor(&method.descriptor) else {
                    continue;
                };
                if descriptor.parameters != signature {
                    continue;
                }
                let identity = (
                    method.name.clone(),
                    method.descriptor.clone(),
                    method.is_static(),
                );
                if identities.insert(identity) {
                    selected.push(SelectedJavaMethod {
                        owner: owner.clone(),
                        method: method.clone(),
                        descriptor,
                        depth,
                    });
                }
            }
            if let Some(super_name) = &declaration.class.super_name {
                queue.push_back((super_name.to_string(), depth + 1));
            }
            for interface in &declaration.class.interfaces {
                queue.push_back((interface.to_string(), depth + 1));
            }
        }
        selected.sort_by(|left, right| {
            left.depth
                .cmp(&right.depth)
                .then_with(|| left.owner.cmp(&right.owner))
                .then_with(|| left.method.descriptor.cmp(&right.method.descriptor))
        });
        Ok(selected)
    }

    fn push_java_alias_method(
        &self,
        visitor: &mut FactCollector,
        proxy: &FullyQualifiedName,
        new_method: RubyMethod,
        old_method: RubyMethod,
        method: &MemberInfo,
        range: TextRange,
        name_range: TextRange,
        old_name_range: TextRange,
    ) {
        let descriptor = parse_method_descriptor(&method.descriptor).expect_invariant(
            "catalog method descriptor failed after alias selection",
            "selection parsed the same descriptor successfully",
            "keep Java alias descriptor validation single-sourced",
        );
        let params = descriptor
            .parameters
            .iter()
            .enumerate()
            .map(|(index, parameter_type)| {
                let name = method
                    .parameters
                    .get(index)
                    .map(|parameter| {
                        ruby_fast_lsp_jruby_support::ruby_parameter_name(&parameter.name, index)
                    })
                    .unwrap_or_else(|| format!("arg{index}"));
                let kind = if method.is_varargs() && index + 1 == descriptor.parameters.len() {
                    MethodParamKind::Rest
                } else {
                    MethodParamKind::Required
                };
                MethodParamFact::new(name, kind).with_signature_metadata(
                    Some(ruby_fast_lsp_jruby_support::ruby_type_for_jvm_type(
                        parameter_type,
                    )),
                    None,
                )
            })
            .collect();
        visitor.direct_push_method_fact_with_signature_and_name_range(
            proxy.namespace_parts().to_vec(),
            NamespaceKind::Instance,
            new_method,
            range,
            name_range,
            params,
            Some(format!(
                "JRuby alias of Java method `{}` with descriptor `{}`.",
                method.name, method.descriptor
            )),
            Some(ruby_fast_lsp_jruby_support::ruby_type_for_jvm_type(
                &descriptor.returns,
            )),
        );
        visitor.add_reference_candidate(ReferenceCandidate::method(
            old_name_range,
            MethodReferenceCandidate {
                owner: proxy.namespace_parts().to_vec(),
                owner_kind: NamespaceKind::Instance,
                method: old_method,
                is_super: false,
                access: MethodReferenceAccess::Normal,
                caller: visitor.scope_tracker().current_method_fqn().cloned(),
                call_expression_range: None,
                preferred_definition_range: None,
                diagnostics: MethodReferenceDiagnostics {
                    diagnostic_range: old_name_range,
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
        let alias_fqn = FullyQualifiedName::method(proxy.namespace_parts().to_vec(), new_method);
        let return_type = ruby_type_for_jvm(&descriptor.returns);
        let fact = TypeFact::new(
            TypeSubject::MethodReturn(alias_fqn),
            return_type,
            range,
            TypeProvenance::Runtime,
        );
        visitor.add_type_fact(fact.clone());
        visitor.add_direct_type_fact(fact);
    }
}

#[derive(Debug, Clone)]
struct SelectedJavaMethod {
    owner: String,
    method: MemberInfo,
    descriptor: MethodDescriptor,
    depth: usize,
}

fn current_runtime_proxy(visitor: &FactCollector) -> Option<FullyQualifiedName> {
    let subject = TypeSubject::Constant(FullyQualifiedName::constant(
        visitor.scope_tracker().get_ns_stack(),
    ));
    let direct = visitor
        .analysis()
        .types
        .iter()
        .rev()
        .find(|fact| fact.subject == subject && fact.provenance == TypeProvenance::Runtime)
        .map(|fact| fact.ruby_type.clone());
    let local = direct.or_else(|| {
        visitor
            .type_facts_for(&subject)
            .into_iter()
            .rev()
            .find(|fact| fact.provenance == TypeProvenance::Runtime)
            .map(|fact| fact.ruby_type)
    });
    let ruby_type = local.or_else(|| {
        visitor
            .project_type_facts_for(&subject)
            .into_iter()
            .rev()
            .find(|fact| fact.provenance == TypeProvenance::Runtime)
            .map(|fact| fact.ruby_type)
    })?;
    match ruby_type {
        RubyType::ClassReference(proxy) | RubyType::ModuleReference(proxy) => Some(proxy),
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::Literal(_)
        | RubyType::Array(_)
        | RubyType::Hash(_, _)
        | RubyType::Shape(_)
        | RubyType::Union(_)
        | RubyType::Unknown => None,
    }
}
