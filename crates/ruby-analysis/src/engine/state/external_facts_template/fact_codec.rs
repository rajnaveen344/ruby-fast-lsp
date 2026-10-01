//! Snapshot encoding and validated restoration of declaration facts.

use super::callable_codec::{
    restore_callable_signature, restore_constant_callable_body, snapshot_callable_signature,
    snapshot_constant_callable_body,
};
use super::type_codec::{
    restore_fqn, restore_parts, restore_provenance, restore_range, restore_ruby_type,
    restore_type_subject, snapshot_fqn, snapshot_parts, snapshot_provenance, snapshot_range,
    snapshot_ruby_type, snapshot_type_subject,
};
use super::{
    validate_contained_range, ProjectNeutralFileFactsSnapshot, SnapshotDirectYieldCall,
    SnapshotForwardedBlockCall, SnapshotGraphEdgeFact, SnapshotGraphEdgeKind,
    SnapshotGraphEdgeProvenance, SnapshotGraphNodeFact, SnapshotGraphNodeKind,
    SnapshotMethodAvailability, SnapshotMethodFact, SnapshotMethodParamFact,
    SnapshotMethodParamKind, SnapshotMethodVisibility, SnapshotMethodVisibilityOverrideFact,
    SnapshotSymbolFact, SnapshotSymbolKind, SnapshotTypeFact, SnapshotUnresolvedGraphEdgeFact,
};
use crate::core::MethodVisibility;
use crate::core::{
    GraphEdgeFact, GraphEdgeKind, GraphNodeFact, GraphNodeKind, MethodAvailability, MethodFact,
    MethodParamFact, MethodParamKind, MethodVisibilityOverrideFact, RubyMethod, SourceFileId,
    SymbolFact, SymbolKind, TypeFact, UnresolvedGraphEdgeFact,
};
use crate::engine::FileFacts;

pub(super) fn snapshot_declaration_facts(
    facts: &FileFacts,
) -> Result<ProjectNeutralFileFactsSnapshot, String> {
    Ok(ProjectNeutralFileFactsSnapshot {
        symbols: facts
            .symbols
            .iter()
            .map(snapshot_symbol)
            .collect::<Result<_, _>>()?,
        methods: facts
            .methods
            .iter()
            .map(snapshot_method)
            .collect::<Result<_, _>>()?,
        method_visibility_overrides: facts
            .method_visibility_overrides
            .iter()
            .map(snapshot_method_visibility_override)
            .collect::<Result<_, _>>()?,
        types: facts
            .types
            .iter()
            .map(snapshot_type_fact)
            .collect::<Result<_, _>>()?,
        graph_nodes: facts
            .graph_nodes
            .iter()
            .map(snapshot_graph_node)
            .collect::<Result<_, _>>()?,
        graph_edges: facts
            .graph_edges
            .iter()
            .map(snapshot_graph_edge)
            .collect::<Result<_, _>>()?,
        unresolved_graph_edges: facts
            .unresolved_graph_edges
            .iter()
            .map(snapshot_unresolved_graph_edge)
            .collect::<Result<_, _>>()?,
        constant_callable_bodies: facts
            .inference
            .constant_callable_bodies
            .iter()
            .map(snapshot_constant_callable_body)
            .collect::<Result<_, _>>()?,
    })
}

pub(super) fn restore_declaration_facts(
    snapshot: ProjectNeutralFileFactsSnapshot,
    source_file_id: SourceFileId,
) -> Result<FileFacts, String> {
    let mut facts = FileFacts {
        symbols: snapshot
            .symbols
            .into_iter()
            .map(|fact| restore_symbol(fact, source_file_id))
            .collect::<Result<_, _>>()?,
        methods: snapshot
            .methods
            .into_iter()
            .map(|fact| restore_method(fact, source_file_id))
            .collect::<Result<_, _>>()?,
        method_visibility_overrides: snapshot
            .method_visibility_overrides
            .into_iter()
            .map(|fact| restore_method_visibility_override(fact, source_file_id))
            .collect::<Result<_, _>>()?,
        types: snapshot
            .types
            .into_iter()
            .map(|fact| restore_type_fact(fact, source_file_id))
            .collect::<Result<_, _>>()?,
        graph_nodes: snapshot
            .graph_nodes
            .into_iter()
            .map(|fact| restore_graph_node(fact, source_file_id))
            .collect::<Result<_, _>>()?,
        graph_edges: snapshot
            .graph_edges
            .into_iter()
            .map(|fact| restore_graph_edge(fact, source_file_id))
            .collect::<Result<_, _>>()?,
        unresolved_graph_edges: snapshot
            .unresolved_graph_edges
            .into_iter()
            .map(|fact| restore_unresolved_graph_edge(fact, source_file_id))
            .collect::<Result<_, _>>()?,
        ..FileFacts::default()
    };
    facts.inference.constant_callable_bodies = snapshot
        .constant_callable_bodies
        .into_iter()
        .map(|fact| restore_constant_callable_body(fact, source_file_id))
        .collect::<Result<_, _>>()?;
    Ok(facts)
}

fn snapshot_symbol_kind(kind: SymbolKind) -> SnapshotSymbolKind {
    match kind {
        SymbolKind::Class => SnapshotSymbolKind::Class,
        SymbolKind::Module => SnapshotSymbolKind::Module,
        SymbolKind::Method => SnapshotSymbolKind::Method,
        SymbolKind::Constant => SnapshotSymbolKind::Constant,
        SymbolKind::LocalVariable => SnapshotSymbolKind::LocalVariable,
        SymbolKind::InstanceVariable => SnapshotSymbolKind::InstanceVariable,
        SymbolKind::ClassVariable => SnapshotSymbolKind::ClassVariable,
        SymbolKind::GlobalVariable => SnapshotSymbolKind::GlobalVariable,
    }
}

fn restore_symbol_kind(kind: SnapshotSymbolKind) -> SymbolKind {
    match kind {
        SnapshotSymbolKind::Class => SymbolKind::Class,
        SnapshotSymbolKind::Module => SymbolKind::Module,
        SnapshotSymbolKind::Method => SymbolKind::Method,
        SnapshotSymbolKind::Constant => SymbolKind::Constant,
        SnapshotSymbolKind::LocalVariable => SymbolKind::LocalVariable,
        SnapshotSymbolKind::InstanceVariable => SymbolKind::InstanceVariable,
        SnapshotSymbolKind::ClassVariable => SymbolKind::ClassVariable,
        SnapshotSymbolKind::GlobalVariable => SymbolKind::GlobalVariable,
    }
}

fn snapshot_symbol(fact: &SymbolFact) -> Result<SnapshotSymbolFact, String> {
    Ok(SnapshotSymbolFact {
        fqn: snapshot_fqn(&fact.fqn)?,
        kind: snapshot_symbol_kind(fact.kind),
        name_range: snapshot_range(fact.name_range),
        range: snapshot_range(fact.range),
    })
}

fn restore_symbol(fact: SnapshotSymbolFact, file_id: SourceFileId) -> Result<SymbolFact, String> {
    let range = restore_range(fact.range, file_id)?;
    let name_range = restore_range(fact.name_range, file_id)?;
    validate_contained_range(name_range, range, "symbol name")?;
    Ok(SymbolFact {
        fqn: restore_fqn(fact.fqn)?,
        kind: restore_symbol_kind(fact.kind),
        name_range,
        range,
    })
}

pub(super) fn snapshot_param_kind(kind: MethodParamKind) -> SnapshotMethodParamKind {
    match kind {
        MethodParamKind::Required => SnapshotMethodParamKind::Required,
        MethodParamKind::Optional => SnapshotMethodParamKind::Optional,
        MethodParamKind::Rest => SnapshotMethodParamKind::Rest,
        MethodParamKind::RequiredKeyword => SnapshotMethodParamKind::RequiredKeyword,
        MethodParamKind::OptionalKeyword => SnapshotMethodParamKind::OptionalKeyword,
        MethodParamKind::KeywordRest => SnapshotMethodParamKind::KeywordRest,
        MethodParamKind::Block => SnapshotMethodParamKind::Block,
        MethodParamKind::Forwarding => SnapshotMethodParamKind::Forwarding,
        MethodParamKind::AnonymousRest => SnapshotMethodParamKind::AnonymousRest,
        MethodParamKind::AnonymousKeywordRest => SnapshotMethodParamKind::AnonymousKeywordRest,
    }
}

pub(super) fn restore_param_kind(kind: SnapshotMethodParamKind) -> MethodParamKind {
    match kind {
        SnapshotMethodParamKind::Required => MethodParamKind::Required,
        SnapshotMethodParamKind::Optional => MethodParamKind::Optional,
        SnapshotMethodParamKind::Rest => MethodParamKind::Rest,
        SnapshotMethodParamKind::RequiredKeyword => MethodParamKind::RequiredKeyword,
        SnapshotMethodParamKind::OptionalKeyword => MethodParamKind::OptionalKeyword,
        SnapshotMethodParamKind::KeywordRest => MethodParamKind::KeywordRest,
        SnapshotMethodParamKind::Block => MethodParamKind::Block,
        SnapshotMethodParamKind::Forwarding => MethodParamKind::Forwarding,
        SnapshotMethodParamKind::AnonymousRest => MethodParamKind::AnonymousRest,
        SnapshotMethodParamKind::AnonymousKeywordRest => MethodParamKind::AnonymousKeywordRest,
    }
}

fn snapshot_visibility(visibility: MethodVisibility) -> SnapshotMethodVisibility {
    match visibility {
        MethodVisibility::Public => SnapshotMethodVisibility::Public,
        MethodVisibility::Protected => SnapshotMethodVisibility::Protected,
        MethodVisibility::Private => SnapshotMethodVisibility::Private,
    }
}

fn restore_visibility(visibility: SnapshotMethodVisibility) -> MethodVisibility {
    match visibility {
        SnapshotMethodVisibility::Public => MethodVisibility::Public,
        SnapshotMethodVisibility::Protected => MethodVisibility::Protected,
        SnapshotMethodVisibility::Private => MethodVisibility::Private,
    }
}

fn snapshot_availability(availability: &MethodAvailability) -> SnapshotMethodAvailability {
    match availability {
        MethodAvailability::Available => SnapshotMethodAvailability::Available,
        MethodAvailability::Unavailable { reason } => SnapshotMethodAvailability::Unavailable {
            reason: reason.clone(),
        },
        MethodAvailability::Absent { reason } => SnapshotMethodAvailability::Absent {
            reason: reason.clone(),
        },
    }
}

fn restore_availability(
    availability: SnapshotMethodAvailability,
) -> Result<MethodAvailability, String> {
    match availability {
        SnapshotMethodAvailability::Available => Ok(MethodAvailability::Available),
        SnapshotMethodAvailability::Unavailable { reason } => {
            validate_reason(&reason)?;
            Ok(MethodAvailability::Unavailable { reason })
        }
        SnapshotMethodAvailability::Absent { reason } => {
            validate_reason(&reason)?;
            Ok(MethodAvailability::Absent { reason })
        }
    }
}

fn validate_reason(reason: &str) -> Result<(), String> {
    if reason.trim().is_empty() {
        return Err("persistent unavailable method reason is empty".to_string());
    }
    Ok(())
}

fn snapshot_method(fact: &MethodFact) -> Result<SnapshotMethodFact, String> {
    Ok(SnapshotMethodFact {
        fqn: snapshot_fqn(&fact.fqn)?,
        owner: snapshot_fqn(&fact.owner)?,
        range: snapshot_range(fact.range),
        name_range: snapshot_range(fact.name_range),
        params: fact.params.clone(),
        param_facts: fact
            .param_facts
            .iter()
            .map(|parameter| SnapshotMethodParamFact {
                name: parameter.name.clone(),
                kind: snapshot_param_kind(parameter.kind),
                type_label: parameter.type_label.clone(),
                documentation: parameter.documentation.clone(),
            })
            .collect(),
        parameter_shape_complete: fact.parameter_shape_complete,
        delegate_receiver: fact
            .delegate_receiver
            .as_ref()
            .map(|method| method.as_str().to_string()),
        visibility: snapshot_visibility(fact.visibility),
        availability: snapshot_availability(&fact.availability),
        documentation: fact.documentation.clone(),
        return_type_label: fact.return_type_label.clone(),
        callable_signatures: fact
            .callable_signatures()
            .iter()
            .map(snapshot_callable_signature)
            .collect::<Result<Vec<_>, _>>()?,
        forwarded_block_call: fact.forwarded_block_call().map(|forwarded| {
            SnapshotForwardedBlockCall {
                receiver_parameter: forwarded.receiver_parameter.clone(),
                method: forwarded.method.as_str().to_string(),
            }
        }),
        direct_yield_call: fact
            .direct_yield_call()
            .map(|direct| SnapshotDirectYieldCall {
                parameter_names: direct.parameter_names.clone(),
            }),
    })
}

fn restore_method(fact: SnapshotMethodFact, file_id: SourceFileId) -> Result<MethodFact, String> {
    let range = restore_range(fact.range, file_id)?;
    let name_range = restore_range(fact.name_range, file_id)?;
    validate_contained_range(name_range, range, "method name")?;
    let param_facts = fact
        .param_facts
        .into_iter()
        .map(|parameter| {
            Ok(
                MethodParamFact::new(parameter.name, restore_param_kind(parameter.kind))
                    .with_signature_metadata(parameter.type_label, parameter.documentation),
            )
        })
        .collect::<Result<Vec<_>, String>>()?;
    let delegate_receiver = fact
        .delegate_receiver
        .map(|name| {
            RubyMethod::new(&name)
                .map_err(|error| format!("invalid persistent delegate method `{name}`: {error}"))
        })
        .transpose()?;
    let callable_signatures = fact
        .callable_signatures
        .into_iter()
        .map(restore_callable_signature)
        .collect::<Result<Vec<_>, _>>()?;
    let forwarded_block_call = fact
        .forwarded_block_call
        .map(|forwarded| {
            Ok::<crate::core::callables::callable_signature::ForwardedBlockCall, String>(
                crate::core::callables::callable_signature::ForwardedBlockCall {
                    receiver_parameter: forwarded.receiver_parameter,
                    method: RubyMethod::new(&forwarded.method).map_err(|error| {
                        format!(
                            "invalid persistent forwarded block method `{}`: {error}",
                            forwarded.method
                        )
                    })?,
                },
            )
        })
        .transpose()?;
    let direct_yield_call = fact.direct_yield_call.map(|direct| {
        crate::core::callables::callable_signature::DirectYieldCall {
            parameter_names: direct.parameter_names,
        }
    });
    Ok(MethodFact {
        fqn: restore_fqn(fact.fqn)?,
        owner: restore_fqn(fact.owner)?,
        range,
        name_range,
        params: fact.params,
        param_facts,
        parameter_shape_complete: fact.parameter_shape_complete,
        delegate_receiver,
        visibility: restore_visibility(fact.visibility),
        availability: restore_availability(fact.availability)?,
        documentation: fact.documentation,
        return_type_label: fact.return_type_label,
        higher_order: None,
    }
    .with_callable_signatures(callable_signatures)
    .with_forwarded_block_call(forwarded_block_call)
    .with_direct_yield_call(direct_yield_call))
}

fn snapshot_method_visibility_override(
    fact: &MethodVisibilityOverrideFact,
) -> Result<SnapshotMethodVisibilityOverrideFact, String> {
    Ok(SnapshotMethodVisibilityOverrideFact {
        owner: snapshot_fqn(&fact.owner)?,
        method: fact.method.as_str().to_string(),
        visibility: snapshot_visibility(fact.visibility),
        range: snapshot_range(fact.range),
    })
}

fn restore_method_visibility_override(
    fact: SnapshotMethodVisibilityOverrideFact,
    file_id: SourceFileId,
) -> Result<MethodVisibilityOverrideFact, String> {
    Ok(MethodVisibilityOverrideFact::new(
        restore_fqn(fact.owner)?,
        RubyMethod::new(&fact.method).map_err(|error| {
            format!(
                "invalid persistent visibility method `{}`: {error}",
                fact.method
            )
        })?,
        restore_visibility(fact.visibility),
        restore_range(fact.range, file_id)?,
    ))
}

fn snapshot_type_fact(fact: &TypeFact) -> Result<SnapshotTypeFact, String> {
    Ok(SnapshotTypeFact {
        subject: snapshot_type_subject(&fact.subject)?,
        ruby_type: snapshot_ruby_type(&fact.ruby_type)?,
        range: snapshot_range(fact.range),
        provenance: snapshot_provenance(fact.provenance),
    })
}

fn restore_type_fact(fact: SnapshotTypeFact, file_id: SourceFileId) -> Result<TypeFact, String> {
    Ok(TypeFact::new(
        restore_type_subject(fact.subject, file_id)?,
        restore_ruby_type(fact.ruby_type, 0)?,
        restore_range(fact.range, file_id)?,
        restore_provenance(fact.provenance),
    ))
}

fn snapshot_graph_node_kind(kind: GraphNodeKind) -> SnapshotGraphNodeKind {
    match kind {
        GraphNodeKind::Class => SnapshotGraphNodeKind::Class,
        GraphNodeKind::Module => SnapshotGraphNodeKind::Module,
    }
}

fn restore_graph_node_kind(kind: SnapshotGraphNodeKind) -> GraphNodeKind {
    match kind {
        SnapshotGraphNodeKind::Class => GraphNodeKind::Class,
        SnapshotGraphNodeKind::Module => GraphNodeKind::Module,
    }
}

fn snapshot_graph_edge_kind(kind: GraphEdgeKind) -> SnapshotGraphEdgeKind {
    match kind {
        GraphEdgeKind::Superclass => SnapshotGraphEdgeKind::Superclass,
        GraphEdgeKind::Include => SnapshotGraphEdgeKind::Include,
        GraphEdgeKind::Prepend => SnapshotGraphEdgeKind::Prepend,
        GraphEdgeKind::Extend => SnapshotGraphEdgeKind::Extend,
        GraphEdgeKind::ExecutionContextApplication => {
            SnapshotGraphEdgeKind::ExecutionContextApplication
        }
    }
}

fn restore_graph_edge_kind(kind: SnapshotGraphEdgeKind) -> GraphEdgeKind {
    match kind {
        SnapshotGraphEdgeKind::Superclass => GraphEdgeKind::Superclass,
        SnapshotGraphEdgeKind::Include => GraphEdgeKind::Include,
        SnapshotGraphEdgeKind::Prepend => GraphEdgeKind::Prepend,
        SnapshotGraphEdgeKind::Extend => GraphEdgeKind::Extend,
        SnapshotGraphEdgeKind::ExecutionContextApplication => {
            GraphEdgeKind::ExecutionContextApplication
        }
    }
}

fn snapshot_graph_edge_provenance(
    provenance: crate::core::GraphEdgeProvenance,
) -> SnapshotGraphEdgeProvenance {
    match provenance {
        crate::core::GraphEdgeProvenance::Explicit => SnapshotGraphEdgeProvenance::Explicit,
        crate::core::GraphEdgeProvenance::ImplicitObject => {
            SnapshotGraphEdgeProvenance::ImplicitObject
        }
    }
}

fn restore_graph_edge_provenance(
    provenance: SnapshotGraphEdgeProvenance,
) -> crate::core::GraphEdgeProvenance {
    match provenance {
        SnapshotGraphEdgeProvenance::Explicit => crate::core::GraphEdgeProvenance::Explicit,
        SnapshotGraphEdgeProvenance::ImplicitObject => {
            crate::core::GraphEdgeProvenance::ImplicitObject
        }
    }
}

fn snapshot_graph_node(fact: &GraphNodeFact) -> Result<SnapshotGraphNodeFact, String> {
    Ok(SnapshotGraphNodeFact {
        fqn: snapshot_fqn(&fact.fqn)?,
        kind: snapshot_graph_node_kind(fact.kind),
        range: snapshot_range(fact.range),
    })
}

fn restore_graph_node(
    fact: SnapshotGraphNodeFact,
    file_id: SourceFileId,
) -> Result<GraphNodeFact, String> {
    Ok(GraphNodeFact::new(
        restore_fqn(fact.fqn)?,
        restore_graph_node_kind(fact.kind),
        restore_range(fact.range, file_id)?,
    ))
}

fn snapshot_graph_edge(fact: &GraphEdgeFact) -> Result<SnapshotGraphEdgeFact, String> {
    Ok(SnapshotGraphEdgeFact {
        source: snapshot_fqn(&fact.source)?,
        target: snapshot_fqn(&fact.target)?,
        kind: snapshot_graph_edge_kind(fact.kind),
        provenance: snapshot_graph_edge_provenance(fact.provenance),
        range: snapshot_range(fact.range),
    })
}

fn restore_graph_edge(
    fact: SnapshotGraphEdgeFact,
    file_id: SourceFileId,
) -> Result<GraphEdgeFact, String> {
    Ok(GraphEdgeFact::new(
        restore_fqn(fact.source)?,
        restore_fqn(fact.target)?,
        restore_graph_edge_kind(fact.kind),
        restore_range(fact.range, file_id)?,
    )
    .with_provenance(restore_graph_edge_provenance(fact.provenance)))
}

fn snapshot_unresolved_graph_edge(
    fact: &UnresolvedGraphEdgeFact,
) -> Result<SnapshotUnresolvedGraphEdgeFact, String> {
    Ok(SnapshotUnresolvedGraphEdgeFact {
        source: snapshot_fqn(&fact.source)?,
        target_parts: snapshot_parts(&fact.target_parts)?,
        absolute: fact.absolute,
        context: snapshot_fqn(&fact.context)?,
        kind: snapshot_graph_edge_kind(fact.kind),
        provenance: snapshot_graph_edge_provenance(fact.provenance),
        range: snapshot_range(fact.range),
    })
}

fn restore_unresolved_graph_edge(
    fact: SnapshotUnresolvedGraphEdgeFact,
    file_id: SourceFileId,
) -> Result<UnresolvedGraphEdgeFact, String> {
    Ok(UnresolvedGraphEdgeFact::new(
        restore_fqn(fact.source)?,
        restore_parts(fact.target_parts)?,
        fact.absolute,
        restore_fqn(fact.context)?,
        restore_graph_edge_kind(fact.kind),
        restore_range(fact.range, file_id)?,
    )
    .with_provenance(restore_graph_edge_provenance(fact.provenance)))
}
