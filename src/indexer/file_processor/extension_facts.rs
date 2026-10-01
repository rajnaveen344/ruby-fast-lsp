//! Conversion of extension-produced facts into analysis facts.

use crate::environment::extensions::analysis_ruby_type_from_extension;
use crate::invariant::ExpectInvariant;
use ruby_analysis::core::MethodVisibility as AnalysisMethodVisibility;
use ruby_analysis::core::{
    FullyQualifiedName, GeneratedOwnerId, GraphEdgeFact, GraphEdgeKind, GraphNodeFact,
    GraphNodeKind, MethodFact, MethodParamFact, MethodParamKind as AnalysisMethodParamKind,
    NamespaceKind as AnalysisNamespaceKind, RubyConstant, RubyMethod, SymbolFact,
    SymbolKind as AnalysisSymbolKind, TextRange, TypeFact, TypeProvenance, TypeSubject,
    UnresolvedGraphEdgeFact,
};
use ruby_analysis::engine::{AnalysisEngine, AnalysisQuery};
use ruby_analysis::indexer::RubyDocument;
use ruby_fast_lsp_extension_api::{
    IndexPatch, MixinKind, NamespaceDeclarationKind, ProjectContext, SourceRange,
};
use std::collections::HashSet;
use std::sync::Arc;

struct ExtensionGraphEdge<'a> {
    source: FullyQualifiedName,
    target_parts: &'a [RubyConstant],
    absolute: bool,
    context: FullyQualifiedName,
    kind: GraphEdgeKind,
    range: TextRange,
}

pub(super) fn add_extension_analysis_facts(
    analysis_engine: &Arc<parking_lot::RwLock<AnalysisEngine>>,
    document: &RubyDocument,
    patches: &[IndexPatch],
    project: Option<&ProjectContext>,
    facts: &mut ruby_analysis::core::FileAnalysis,
) {
    if patches.is_empty() {
        return;
    }

    let mut known_namespaces = {
        let engine = analysis_engine.read();
        AnalysisQuery::new(&engine).known_namespace_fqns()
    };
    for node in &facts.graph_nodes {
        if let Some(namespace) = node.fqn.to_instance_namespace() {
            known_namespaces.insert(namespace);
        }
    }

    for patch in patches {
        match patch {
            IndexPatch::DefineNamespace(namespace) => {
                let parts = ruby_constants(&namespace.namespace, "DefineNamespace namespace");
                let fqn = FullyQualifiedName::namespace(parts);
                let range = text_range_from_source_range(document, namespace.location, "namespace");
                let kind = match namespace.kind {
                    NamespaceDeclarationKind::Class => GraphNodeKind::Class,
                    NamespaceDeclarationKind::Module => GraphNodeKind::Module,
                };
                if !facts
                    .symbols
                    .iter()
                    .any(|fact| fact.fqn == fqn && fact.range == range)
                {
                    facts.symbols.push(SymbolFact::new(
                        fqn.clone(),
                        match kind {
                            GraphNodeKind::Class => AnalysisSymbolKind::Class,
                            GraphNodeKind::Module => AnalysisSymbolKind::Module,
                        },
                        range,
                    ));
                }
                if !facts
                    .graph_nodes
                    .iter()
                    .any(|fact| fact.fqn == fqn && fact.kind == kind && fact.range == range)
                {
                    facts
                        .graph_nodes
                        .push(GraphNodeFact::new(fqn.clone(), kind, range));
                    facts.graph_nodes.push(GraphNodeFact::new(
                        fqn.to_singleton_namespace().expect_invariant(
                            "extension namespace could not convert to singleton",
                            "validated namespace declarations must produce namespace FQNs",
                            "construct DefineNamespace facts from FullyQualifiedName::namespace",
                        ),
                        kind,
                        range,
                    ));
                }
                let namespace_type = TypeFact::new(
                    TypeSubject::Constant(FullyQualifiedName::constant(fqn.namespace_parts())),
                    match kind {
                        GraphNodeKind::Class => {
                            ruby_analysis::core::RubyType::ClassReference(fqn.clone())
                        }
                        GraphNodeKind::Module => {
                            ruby_analysis::core::RubyType::ModuleReference(fqn.clone())
                        }
                    },
                    range,
                    TypeProvenance::Extension,
                );
                if !facts.types.contains(&namespace_type) {
                    facts.types.push(namespace_type);
                }
                known_namespaces.insert(fqn);
            }
            IndexPatch::DefineConstant(constant) => {
                let mut parts = ruby_constants(&constant.namespace, "DefineConstant namespace");
                parts.push(RubyConstant::new(&constant.name).unwrap_or_else(|err| {
                    unreachable_invariant!(
                        what = "extension emitted invalid constant `{}`: {}",
                        why = "constant patches must be validated before fact conversion",
                        fix = "reject invalid DefineConstant patches at the extension boundary",
                        constant.name,
                        err,
                    )
                }));
                let fqn = FullyQualifiedName::constant(parts);
                let range = text_range_from_source_range(document, constant.location, "constant");
                if !facts
                    .symbols
                    .iter()
                    .any(|fact| fact.fqn == fqn && fact.range == range)
                {
                    facts.symbols.push(SymbolFact::new(
                        fqn.clone(),
                        AnalysisSymbolKind::Constant,
                        range,
                    ));
                }
                if let Some(ruby_type) =
                    analysis_ruby_type_from_extension(constant.ruby_type.as_ref()).expect_invariant(
                        "extension constant type reached fact conversion without validation",
                        "guest patches must be validated before collection",
                        "keep extension payload validation before patch application",
                    )
                {
                    let type_fact = TypeFact::new(
                        TypeSubject::Constant(fqn),
                        ruby_type,
                        range,
                        TypeProvenance::Extension,
                    );
                    if !facts.types.contains(&type_fact) {
                        facts.types.push(type_fact);
                    }
                }
            }
            IndexPatch::AddReference(_) => {
                // Resolved reference candidates are applied to FactCollector during
                // extension call traversal and flow through FileAnalysis separately
                // from direct parser/index facts.
            }
            IndexPatch::DefineMethod(method) => {
                let (namespace, owner_kind) = analysis_patch_owner(
                    document,
                    project,
                    method.owner_target.as_ref(),
                    &method.namespace,
                    method.owner_kind,
                    &method.source.extension_id,
                    "DefineMethod owner",
                );
                let ruby_method = RubyMethod::new(&method.name).unwrap_or_else(|err| {
                    unreachable_invariant!(
                        what = "extension emitted invalid analysis method `{}`: {}",
                        why = "extension method patches must be validated before fact conversion",
                        fix = "reject invalid DefineMethod patches at the extension boundary",
                        method.name,
                        err,
                    )
                });
                let fqn = FullyQualifiedName::method(namespace.clone(), ruby_method);
                let owner = FullyQualifiedName::namespace_with_kind(namespace, owner_kind);
                let range = text_range_from_source_range(document, method.location, "method");
                if !facts
                    .symbols
                    .iter()
                    .any(|fact| fact.fqn == fqn && fact.range == range)
                {
                    facts.symbols.push(SymbolFact::new(
                        fqn.clone(),
                        AnalysisSymbolKind::Method,
                        range,
                    ));
                }
                let return_type = analysis_ruby_type_from_extension(method.return_type.as_ref())
                    .expect_invariant(
                        "extension return type reached fact conversion without validation",
                        "guest patches must be validated before collection",
                        "keep extension payload validation before patch application",
                    );
                let return_type_label = return_type.as_ref().map(ToString::to_string);
                let method_fact = MethodFact::with_param_facts(
                    fqn.clone(),
                    owner.clone(),
                    range,
                    analysis_method_params_from_extension(&method.params),
                )
                .with_visibility(analysis_method_visibility(method.visibility))
                .with_signature_metadata(None, return_type_label);
                if let Some(existing) = facts
                    .methods
                    .iter_mut()
                    .find(|fact| fact.fqn == fqn && fact.owner == owner && fact.range == range)
                {
                    *existing = method_fact;
                } else {
                    facts.methods.push(method_fact);
                }
                if let Some(return_type) = return_type {
                    facts.types.push(TypeFact::new(
                        TypeSubject::MethodReturn(fqn),
                        return_type,
                        range,
                        TypeProvenance::Extension,
                    ));
                }
            }
            IndexPatch::SetSuperclass(superclass) => {
                let source_parts = ruby_constants(&superclass.namespace, "SetSuperclass namespace");
                let source = FullyQualifiedName::namespace(source_parts.clone());
                let target_parts = ruby_constants(&superclass.superclass, "SetSuperclass target");
                let range =
                    text_range_from_source_range(document, superclass.location, "superclass");
                let context = FullyQualifiedName::namespace(source_parts.clone());
                if let Some(target) = resolve_extension_namespace(
                    &known_namespaces,
                    &target_parts,
                    superclass.absolute,
                    &context,
                ) {
                    let source_singleton = source.to_singleton_namespace().expect_invariant(
                        "generated class namespace could not convert to singleton",
                        "validated class declarations must support Ruby singleton inheritance",
                        "construct SetSuperclass sources from FullyQualifiedName::namespace",
                    );
                    if let Some(target_singleton) = target.to_singleton_namespace() {
                        facts.graph_edges.push(GraphEdgeFact::new(
                            source_singleton,
                            target_singleton,
                            GraphEdgeKind::Superclass,
                            range,
                        ));
                    }
                }
                push_extension_graph_edge(
                    facts,
                    &known_namespaces,
                    ExtensionGraphEdge {
                        source,
                        target_parts: &target_parts,
                        absolute: superclass.absolute,
                        context,
                        kind: GraphEdgeKind::Superclass,
                        range,
                    },
                );
            }
            IndexPatch::ApplyMixin(mixin) => {
                let (mut source_parts, source_kind) = analysis_patch_owner(
                    document,
                    project,
                    mixin.owner_target.as_ref(),
                    &mixin.namespace,
                    mixin.target_kind,
                    &mixin.source.extension_id,
                    "ApplyMixin owner",
                );
                if source_parts.is_empty() && mixin.owner_target.is_none() {
                    source_parts.push(RubyConstant::new("Object").expect_invariant(
                        "Object is not a valid Ruby constant",
                        "root mixin patches normalize to Object",
                        "keep RubyConstant validation compatible with Ruby class names",
                    ));
                    let object = FullyQualifiedName::namespace(source_parts.clone());
                    let range = text_range_from_source_range(document, mixin.location, "mixin");
                    facts.graph_nodes.push(GraphNodeFact::new(
                        object.clone(),
                        GraphNodeKind::Class,
                        range,
                    ));
                    facts.graph_nodes.push(GraphNodeFact::new(
                        object.to_singleton_namespace().expect_invariant(
                            "Object namespace could not convert to singleton",
                            "namespace graph nodes must support singleton variants",
                            "update FullyQualifiedName singleton conversion",
                        ),
                        GraphNodeKind::Class,
                        range,
                    ));
                    known_namespaces.insert(object);
                }

                let source =
                    FullyQualifiedName::namespace_with_kind(source_parts.clone(), source_kind);
                let kind = analysis_mixin_kind(mixin.kind);
                let range = text_range_from_source_range(document, mixin.location, "mixin");
                if let Some(target) = mixin.mixin_target.as_ref() {
                    let (target_parts, target_kind) = analysis_patch_owner(
                        document,
                        project,
                        Some(target),
                        &[],
                        ruby_fast_lsp_extension_api::NamespaceKind::Instance,
                        &mixin.source.extension_id,
                        "ApplyMixin semantic target",
                    );
                    let target = FullyQualifiedName::namespace_with_kind(target_parts, target_kind);
                    facts.graph_edges.push(GraphEdgeFact::new(
                        source.clone(),
                        target.clone(),
                        kind,
                        range,
                    ));
                    if mixin.kind == MixinKind::Extend {
                        if let Some(singleton_source) = source.to_singleton_namespace() {
                            facts.graph_edges.push(GraphEdgeFact::new(
                                singleton_source,
                                target,
                                GraphEdgeKind::Include,
                                range,
                            ));
                        }
                    }
                    continue;
                }

                let target_parts = ruby_constants(&mixin.mixin, "ApplyMixin target");
                push_extension_graph_edge(
                    facts,
                    &known_namespaces,
                    ExtensionGraphEdge {
                        source: source.clone(),
                        target_parts: &target_parts,
                        absolute: mixin.absolute,
                        context: FullyQualifiedName::namespace(source_parts.clone()),
                        kind,
                        range,
                    },
                );
                if mixin.kind == MixinKind::Extend {
                    if let Some(singleton_source) = source.to_singleton_namespace() {
                        push_extension_graph_edge(
                            facts,
                            &known_namespaces,
                            ExtensionGraphEdge {
                                source: singleton_source,
                                target_parts: &target_parts,
                                absolute: mixin.absolute,
                                context: FullyQualifiedName::namespace(source_parts),
                                kind: GraphEdgeKind::Include,
                                range,
                            },
                        );
                    }
                }
            }
            IndexPatch::ConnectExecutionContext(connection) => {
                let (template_parts, template_kind) = analysis_patch_owner(
                    document,
                    project,
                    Some(&connection.template),
                    &[],
                    ruby_fast_lsp_extension_api::NamespaceKind::Instance,
                    &connection.source.extension_id,
                    "ConnectExecutionContext template",
                );
                let (application_parts, application_kind) = analysis_patch_owner(
                    document,
                    project,
                    Some(&connection.application),
                    &[],
                    ruby_fast_lsp_extension_api::NamespaceKind::Instance,
                    &connection.source.extension_id,
                    "ConnectExecutionContext application",
                );
                facts.graph_edges.push(GraphEdgeFact::new(
                    FullyQualifiedName::namespace_with_kind(template_parts, template_kind),
                    FullyQualifiedName::namespace_with_kind(application_parts, application_kind),
                    GraphEdgeKind::ExecutionContextApplication,
                    text_range_from_source_range(
                        document,
                        connection.location,
                        "execution context application",
                    ),
                ));
            }
        }
    }
}

fn analysis_patch_owner(
    document: &RubyDocument,
    project: Option<&ProjectContext>,
    target: Option<&ruby_fast_lsp_extension_api::ExecutionContextTarget>,
    fallback_namespace: &[String],
    fallback_kind: ruby_fast_lsp_extension_api::NamespaceKind,
    extension_id: &str,
    label: &str,
) -> (Vec<RubyConstant>, AnalysisNamespaceKind) {
    match target {
        None => (
            ruby_constants(fallback_namespace, label),
            analysis_namespace_kind(fallback_kind),
        ),
        Some(ruby_fast_lsp_extension_api::ExecutionContextTarget::Namespace {
            namespace,
            owner_kind,
        }) => (
            ruby_constants(namespace, label),
            analysis_namespace_kind(*owner_kind),
        ),
        Some(ruby_fast_lsp_extension_api::ExecutionContextTarget::GeneratedOwner {
            local_id,
            owner_kind,
        }) => {
            let owner = GeneratedOwnerId::new(extension_id, document.uri.as_str(), local_id)
                .expect_invariant(
                    "invalid generated patch owner reached fact conversion",
                    "extension owner targets must be validated before collection",
                    "keep validate_patch_owner_target before add_extension_analysis_facts",
                );
            (
                vec![RubyConstant::generated_owner(owner)],
                owner_kind
                    .map(analysis_namespace_kind)
                    .unwrap_or_else(|| analysis_namespace_kind(fallback_kind)),
            )
        }
        Some(ruby_fast_lsp_extension_api::ExecutionContextTarget::ProjectGeneratedOwner {
            local_id,
            owner_kind,
        }) => {
            let project_uri = project
                .map(|project| project.project_uri.as_str())
                .expect_invariant(
                    "project-generated patch owner reached fact conversion without ProjectContext",
                    "project-scoped targets must be rejected before collection",
                    "preserve the owning project context through extension fact conversion",
                );
            let owner = GeneratedOwnerId::new(extension_id, project_uri, local_id)
                .expect_invariant(
                    "invalid project-generated patch owner reached fact conversion",
                    "extension owner targets must be validated before collection",
                    "keep validate_patch_owner_target before add_extension_analysis_facts",
                );
            (
                vec![RubyConstant::generated_owner(owner)],
                owner_kind
                    .map(analysis_namespace_kind)
                    .unwrap_or_else(|| analysis_namespace_kind(fallback_kind)),
            )
        }
    }
}

fn push_extension_graph_edge(
    facts: &mut ruby_analysis::core::FileAnalysis,
    known_namespaces: &HashSet<FullyQualifiedName>,
    edge: ExtensionGraphEdge<'_>,
) {
    let Some(target) = resolve_extension_namespace(
        known_namespaces,
        edge.target_parts,
        edge.absolute,
        &edge.context,
    ) else {
        facts
            .unresolved_graph_edges
            .push(UnresolvedGraphEdgeFact::new(
                edge.source,
                edge.target_parts.to_vec(),
                edge.absolute,
                edge.context,
                edge.kind,
                edge.range,
            ));
        return;
    };
    facts.graph_edges.push(GraphEdgeFact::new(
        edge.source,
        target,
        edge.kind,
        edge.range,
    ));
}

fn resolve_extension_namespace(
    known_namespaces: &HashSet<FullyQualifiedName>,
    parts: &[RubyConstant],
    absolute: bool,
    context: &FullyQualifiedName,
) -> Option<FullyQualifiedName> {
    let mut search = if absolute {
        Vec::new()
    } else {
        context.namespace_parts()
    };

    loop {
        let mut probe = search.clone();
        probe.extend(parts.iter().cloned());
        let fqn = FullyQualifiedName::namespace(probe);
        if known_namespaces.contains(&fqn) {
            return Some(fqn);
        }
        if absolute || search.is_empty() {
            break;
        }
        search.pop();
    }

    let fqn = FullyQualifiedName::namespace(parts.to_vec());
    known_namespaces.contains(&fqn).then_some(fqn)
}

fn ruby_constants(parts: &[String], label: &str) -> Vec<RubyConstant> {
    parts
        .iter()
        .map(|part| {
            RubyConstant::new(part).unwrap_or_else(|err| {
                unreachable_invariant!(
                    what = "extension emitted invalid {label} constant `{}`: {}",
                    why = "extension constant patches must be valid Ruby constants",
                    fix = "validate constants before emitting extension index patches",
                    part,
                    err,
                    label = label,
                )
            })
        })
        .collect()
}

fn analysis_namespace_kind(
    kind: ruby_fast_lsp_extension_api::NamespaceKind,
) -> AnalysisNamespaceKind {
    match kind {
        ruby_fast_lsp_extension_api::NamespaceKind::Instance => AnalysisNamespaceKind::Instance,
        ruby_fast_lsp_extension_api::NamespaceKind::Singleton => AnalysisNamespaceKind::Singleton,
    }
}

fn analysis_method_visibility(
    visibility: ruby_fast_lsp_extension_api::MethodVisibility,
) -> AnalysisMethodVisibility {
    match visibility {
        ruby_fast_lsp_extension_api::MethodVisibility::Public => AnalysisMethodVisibility::Public,
        ruby_fast_lsp_extension_api::MethodVisibility::Protected => {
            AnalysisMethodVisibility::Protected
        }
        ruby_fast_lsp_extension_api::MethodVisibility::Private => AnalysisMethodVisibility::Private,
    }
}

fn analysis_mixin_kind(kind: MixinKind) -> GraphEdgeKind {
    match kind {
        MixinKind::Include => GraphEdgeKind::Include,
        MixinKind::Prepend => GraphEdgeKind::Prepend,
        MixinKind::Extend => GraphEdgeKind::Extend,
    }
}

fn analysis_method_params_from_extension(
    params: &[ruby_fast_lsp_extension_api::MethodParamPatch],
) -> Vec<MethodParamFact> {
    params
        .iter()
        .map(|param| {
            MethodParamFact::new(param.name.clone(), analysis_method_param_kind(param.kind))
        })
        .collect()
}

fn analysis_method_param_kind(
    kind: ruby_fast_lsp_extension_api::MethodParamKind,
) -> AnalysisMethodParamKind {
    match kind {
        ruby_fast_lsp_extension_api::MethodParamKind::Required => AnalysisMethodParamKind::Required,
        ruby_fast_lsp_extension_api::MethodParamKind::Optional => AnalysisMethodParamKind::Optional,
        ruby_fast_lsp_extension_api::MethodParamKind::Rest => AnalysisMethodParamKind::Rest,
        ruby_fast_lsp_extension_api::MethodParamKind::RequiredKeyword => {
            AnalysisMethodParamKind::RequiredKeyword
        }
        ruby_fast_lsp_extension_api::MethodParamKind::OptionalKeyword => {
            AnalysisMethodParamKind::OptionalKeyword
        }
        ruby_fast_lsp_extension_api::MethodParamKind::KeywordRest => {
            AnalysisMethodParamKind::KeywordRest
        }
        ruby_fast_lsp_extension_api::MethodParamKind::Block => AnalysisMethodParamKind::Block,
    }
}

fn text_range_from_source_range(
    document: &RubyDocument,
    range: SourceRange,
    kind: &str,
) -> TextRange {
    let start = ruby_analysis::core::SourcePosition {
        line: range.start.line,
        character: range.start.character,
    };
    let end = ruby_analysis::core::SourcePosition {
        line: range.end.line,
        character: range.end.character,
    };
    TextRange::new(
        document.analysis_file_id(),
        byte_offset_u32(
            document.position_to_offset(start),
            &format!("extension {kind} start offset exceeded u32"),
        ),
        byte_offset_u32(
            document.position_to_offset(end),
            &format!("extension {kind} end offset exceeded u32"),
        ),
    )
}

fn byte_offset_u32(byte_offset: usize, message: &str) -> u32 {
    u32::try_from(byte_offset).unwrap_or_else(|_| {
        unreachable_invariant!(
            what = "{message}",
            why = "ruby-analysis::core TextRange currently stores u32 offsets",
            fix = "widen TextRange offsets before indexing files larger than u32::MAX bytes",
            message = message,
        )
    })
}
