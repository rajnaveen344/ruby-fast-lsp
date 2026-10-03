use crate::invariant::ExpectInvariant;
use ruby_analysis::core::{
    FullyQualifiedName, GeneratedOwnerId, GraphNodeKind,
    MethodVisibility as AnalysisMethodVisibility, NamespaceKind, ReferenceCandidate, RubyConstant,
    RubyMethod, SymbolFact, SymbolKind as AnalysisSymbolKind, TypeFact, TypeProvenance,
    TypeSubject,
};
use ruby_analysis::indexer::fact_collector::FactCollector;
use ruby_fast_lsp_extension_api::{
    ExecutionContextTarget, GeneratedOwnerScope, IndexPatch, NamespaceKind as AbiNamespaceKind,
};
use ruby_prism::CallNode;

use crate::environment::extensions::dispatch::execution_context::{
    extension_ruby_constants, generated_owner_scope_identity,
};
use crate::environment::extensions::patches::conflicts::namespace_kind_from_abi;
use crate::environment::extensions::patches::types::analysis_ruby_type_from_extension;
use crate::environment::extensions::responses::range_from_abi;

pub(super) fn apply_patch(visitor: &mut FactCollector, call: &CallNode, patch: IndexPatch) {
    match &patch {
        IndexPatch::DefineNamespace(namespace) => {
            let parts = namespace
                .namespace
                .iter()
                .map(|part| {
                    RubyConstant::new(part).expect_invariant(
                        "extension namespace reached application without validation",
                        "guest patches must be validated before conflict resolution",
                        "keep validate_index_patch_payloads before emitted patch collection",
                    )
                })
                .collect::<Vec<_>>();
            let range = crate::utils::lsp::text_range(
                visitor.document(),
                range_from_abi(namespace.location),
            );
            visitor.direct_push_namespace_facts(
                FullyQualifiedName::namespace(parts),
                match namespace.kind {
                    ruby_fast_lsp_extension_api::NamespaceDeclarationKind::Class => {
                        GraphNodeKind::Class
                    }
                    ruby_fast_lsp_extension_api::NamespaceDeclarationKind::Module => {
                        GraphNodeKind::Module
                    }
                },
                range,
                range,
            );
        }
        IndexPatch::DefineConstant(constant) => {
            let mut parts = constant
                .namespace
                .iter()
                .map(|part| {
                    RubyConstant::new(part).expect_invariant(
                        "extension constant namespace reached application without validation",
                        "guest patches must be validated before conflict resolution",
                        "keep validate_index_patch_payloads before emitted patch collection",
                    )
                })
                .collect::<Vec<_>>();
            parts.push(RubyConstant::new(&constant.name).expect_invariant(
                "extension constant name reached application without validation",
                "guest patches must be validated before conflict resolution",
                "keep validate_index_patch_payloads before emitted patch collection",
            ));
            let fqn = FullyQualifiedName::constant(parts);
            let range = crate::utils::lsp::text_range(
                visitor.document(),
                range_from_abi(constant.location),
            );
            visitor.add_symbol_fact(SymbolFact::new(
                fqn.clone(),
                AnalysisSymbolKind::Constant,
                range,
            ));
            if let Some(ruby_type) = analysis_ruby_type_from_extension(constant.ruby_type.as_ref())
                .expect_invariant(
                    "extension constant type reached application without validation",
                    "guest patches must be validated before conflict resolution",
                    "keep validate_index_patch_payloads before emitted patch collection",
                )
            {
                let fact = TypeFact::new(
                    TypeSubject::Constant(fqn),
                    ruby_type,
                    range,
                    TypeProvenance::Extension,
                );
                visitor.add_type_fact(fact.clone());
                visitor.add_direct_type_fact(fact);
            }
        }
        IndexPatch::AddReference(reference) => {
            let range = crate::utils::lsp::text_range(
                visitor.document(),
                range_from_abi(reference.location),
            );
            let target = match &reference.target {
                ruby_fast_lsp_extension_api::ReferenceTarget::Namespace(namespace) => {
                    FullyQualifiedName::namespace(
                        namespace
                            .iter()
                            .map(|part| RubyConstant::new(part).expect_invariant(
                                "extension reference namespace reached application without validation",
                                "guest patches must be validated before conflict resolution",
                                "keep validate_index_patch_payloads before emitted patch collection",
                            ))
                            .collect::<Vec<_>>(),
                    )
                }
                ruby_fast_lsp_extension_api::ReferenceTarget::Constant { namespace, name } => {
                    let mut parts = namespace
                        .iter()
                        .map(|part| RubyConstant::new(part).expect_invariant(
                            "extension reference constant namespace reached application without validation",
                            "guest patches must be validated before conflict resolution",
                            "keep validate_index_patch_payloads before emitted patch collection",
                        ))
                        .collect::<Vec<_>>();
                    parts.push(RubyConstant::new(name).expect_invariant(
                        "extension reference constant name reached application without validation",
                        "guest patches must be validated before conflict resolution",
                        "keep validate_index_patch_payloads before emitted patch collection",
                    ));
                    FullyQualifiedName::constant(parts)
                }
                ruby_fast_lsp_extension_api::ReferenceTarget::Method {
                    namespace,
                    owner_kind,
                    name,
                } => {
                    let owner = namespace
                        .iter()
                        .map(|part| RubyConstant::new(part).expect_invariant(
                            "extension method reference namespace reached application without validation",
                            "guest patches must be validated before conflict resolution",
                            "keep validate_index_patch_payloads before patch application",
                        ))
                        .collect::<Vec<_>>();
                    let method = RubyMethod::new(name).expect_invariant(
                        "extension method reference name reached application without validation",
                        "guest patches must be validated before conflict resolution",
                        "keep validate_index_patch_payloads before patch application",
                    );
                    let owner_kind = match owner_kind {
                        AbiNamespaceKind::Instance => NamespaceKind::Instance,
                        AbiNamespaceKind::Singleton => NamespaceKind::Singleton,
                    };
                    visitor.add_reference_candidate(ReferenceCandidate::method_target(
                        range,
                        owner,
                        owner_kind,
                        method,
                        None,
                    ));
                    return;
                }
            };
            visitor.add_reference_candidate(ReferenceCandidate::resolved(range, target, None));
        }
        IndexPatch::DefineMethod(method) => {
            let declared_return_type =
                analysis_ruby_type_from_extension(method.return_type.as_ref()).expect_invariant(
                    "extension return type reached application without validation",
                    "guest patches must be validated before conflict resolution",
                    "keep validate_index_patch_payloads before emitted patch collection",
                );
            let inferred_return_type = match method.return_type_source {
                Some(ruby_fast_lsp_extension_api::MethodReturnTypeSource::Block) => {
                    visitor.infer_call_block_return_type(call)
                }
                None => None,
            };
            let return_type = inferred_return_type.clone().or(declared_return_type);
            let (namespace, owner_kind) = resolved_patch_owner(
                method.owner_target.as_ref(),
                &method.namespace,
                method.owner_kind,
                &method.source.extension_id,
                visitor.document().uri.as_str(),
                visitor
                    .extension_project_context()
                    .map(|project| project.project_uri.as_str()),
                "method owner",
            );
            let ruby_method = RubyMethod::new(&method.name).expect_invariant(
                "extension method name reached application without validation",
                "guest patches must be validated before conflict resolution",
                "keep validate_index_patch_payloads before emitted patch collection",
            );
            let fqn = FullyQualifiedName::method(namespace, ruby_method);
            let range =
                crate::utils::lsp::text_range(visitor.document(), range_from_abi(method.location));
            visitor.direct_push_method_fact_with_visibility(
                fqn.namespace_parts(),
                owner_kind,
                ruby_method,
                range,
                match method.visibility {
                    ruby_fast_lsp_extension_api::MethodVisibility::Public => {
                        AnalysisMethodVisibility::Public
                    }
                    ruby_fast_lsp_extension_api::MethodVisibility::Protected => {
                        AnalysisMethodVisibility::Protected
                    }
                    ruby_fast_lsp_extension_api::MethodVisibility::Private => {
                        AnalysisMethodVisibility::Private
                    }
                },
            );
            if let Some(return_type) = return_type {
                let type_fact = TypeFact::new(
                    TypeSubject::MethodReturn(fqn),
                    return_type,
                    range,
                    TypeProvenance::Extension,
                );
                visitor.add_type_fact(type_fact.clone());
                if inferred_return_type.is_some() && !visitor.analysis().types.contains(&type_fact)
                {
                    visitor.add_direct_type_fact(type_fact);
                }
            }
        }
        IndexPatch::SetSuperclass(_) => {}
        IndexPatch::ApplyMixin(_) => {}
        IndexPatch::ConnectExecutionContext(_) => {}
    }
    visitor.record_extension_patch(patch);
}

fn resolved_patch_owner(
    target: Option<&ExecutionContextTarget>,
    fallback_namespace: &[String],
    fallback_kind: AbiNamespaceKind,
    extension_id: &str,
    source_identity: &str,
    project_identity: Option<&str>,
    label: &str,
) -> (Vec<RubyConstant>, NamespaceKind) {
    match target {
        None => (
            extension_ruby_constants(fallback_namespace, label),
            namespace_kind_from_abi(fallback_kind),
        ),
        Some(ExecutionContextTarget::Namespace {
            namespace,
            owner_kind,
        }) => (
            extension_ruby_constants(namespace, label),
            namespace_kind_from_abi(*owner_kind),
        ),
        Some(ExecutionContextTarget::GeneratedOwner {
            local_id,
            owner_kind,
        }) => {
            let owner = GeneratedOwnerId::new(extension_id, source_identity, local_id)
                .expect_invariant(
                    "invalid generated patch owner reached application",
                    "semantic patch owners must be validated before conversion",
                    "keep validate_patch_owner_target before apply_patch",
                );
            (
                vec![RubyConstant::generated_owner(owner)],
                owner_kind
                    .map(namespace_kind_from_abi)
                    .unwrap_or_else(|| namespace_kind_from_abi(fallback_kind)),
            )
        }
        Some(ExecutionContextTarget::ProjectGeneratedOwner {
            local_id,
            owner_kind,
        }) => {
            let identity = generated_owner_scope_identity(
                GeneratedOwnerScope::Project,
                source_identity,
                project_identity,
                label,
            );
            let owner = GeneratedOwnerId::new(extension_id, identity, local_id).expect_invariant(
                "invalid project-generated patch owner reached application",
                "semantic patch owners must be validated before conversion",
                "keep validation before apply_patch",
            );
            (
                vec![RubyConstant::generated_owner(owner)],
                owner_kind
                    .map(namespace_kind_from_abi)
                    .unwrap_or_else(|| namespace_kind_from_abi(fallback_kind)),
            )
        }
    }
}
