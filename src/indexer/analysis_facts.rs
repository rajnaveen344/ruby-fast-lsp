use ruby_analysis_core::{
    FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphNodeFact, GraphNodeKind, MethodFact,
    ReferenceFact, SourceFileId, SymbolFact, SymbolKind, TextRange,
};
use tower_lsp::lsp_types::{Location, Range, Url};

use crate::analyzer_prism::utils;
use crate::indexer::entry::{EntryKind, MixinRef};
use crate::indexer::index::RubyIndex;
use crate::types::ruby_document::RubyDocument;

pub fn collect_symbol_facts_for_file(
    index: &RubyIndex,
    document: &RubyDocument,
    uri: &Url,
    file_id: SourceFileId,
) -> Vec<SymbolFact> {
    index
        .file_entries(uri)
        .into_iter()
        .filter_map(|entry| {
            let kind = symbol_kind_for_entry(&entry.kind)?;
            let fqn = index.get_fqn(entry.fqn_id).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: index entry references missing FQN id. \
                     This is a bug because RubyIndex::add_entry must intern every FQN before storing entries. \
                     Fix: build entries through EntryBuilder or intern the FQN before insertion."
                )
            });
            Some(SymbolFact::new(
                fqn.clone(),
                kind,
                text_range_from_lsp_range(document, file_id, entry.location.range, "symbol"),
            ))
        })
        .collect()
}

pub fn collect_reference_facts_from_locations<'a>(
    document: &RubyDocument,
    file_id: SourceFileId,
    references: impl Iterator<Item = &'a (FullyQualifiedName, Location, Option<FullyQualifiedName>)>,
) -> Vec<ReferenceFact> {
    references
        .map(|(target, location, caller)| {
            ReferenceFact::new(
                target.clone(),
                text_range_from_lsp_range(document, file_id, location.range, "reference"),
                caller.clone(),
            )
        })
        .collect()
}

pub fn collect_method_facts_for_file(
    index: &RubyIndex,
    document: &RubyDocument,
    uri: &Url,
    file_id: SourceFileId,
) -> Vec<MethodFact> {
    index
        .file_entries(uri)
        .into_iter()
        .filter_map(|entry| {
            let EntryKind::Method(data) = &entry.kind else {
                return None;
            };
            let fqn = index.get_fqn(entry.fqn_id).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: method index entry references missing FQN id. \
                     This is a bug because RubyIndex::add_entry must intern every FQN before storing entries. \
                     Fix: build entries through EntryBuilder or intern the FQN before insertion."
                )
            });
            Some(MethodFact::new(
                fqn.clone(),
                data.owner.clone(),
                text_range_from_lsp_range(document, file_id, entry.location.range, "method"),
            ))
        })
        .collect()
}

pub fn collect_graph_facts_for_file(
    index: &RubyIndex,
    document: &RubyDocument,
    uri: &Url,
    file_id: SourceFileId,
) -> (Vec<GraphNodeFact>, Vec<GraphEdgeFact>) {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    for entry in index.file_entries(uri) {
        let source = index.get_fqn(entry.fqn_id).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: index entry references missing FQN id. \
                 This is a bug because RubyIndex::add_entry must intern every FQN before storing entries. \
                 Fix: build entries through EntryBuilder or intern the FQN before insertion."
            )
        });
        match &entry.kind {
            EntryKind::Class(data) => {
                let range = text_range_from_lsp_range(
                    document,
                    file_id,
                    entry.location.range,
                    "graph node",
                );
                nodes.push(GraphNodeFact::new(
                    source.clone(),
                    GraphNodeKind::Class,
                    range,
                ));
                if let Some(singleton_source) = source.to_singleton_namespace() {
                    nodes.push(GraphNodeFact::new(
                        singleton_source,
                        GraphNodeKind::Class,
                        range,
                    ));
                }
                if let Some(superclass) = &data.superclass {
                    push_graph_edge(
                        index,
                        document,
                        file_id,
                        source,
                        superclass,
                        GraphEdgeKind::Superclass,
                        &mut edges,
                    );
                    push_singleton_superclass_edge(
                        index, document, file_id, source, superclass, &mut edges,
                    );
                }
                push_graph_edges(
                    index,
                    document,
                    file_id,
                    source,
                    &data.includes,
                    GraphEdgeKind::Include,
                    &mut edges,
                );
                push_graph_edges(
                    index,
                    document,
                    file_id,
                    source,
                    &data.prepends,
                    GraphEdgeKind::Prepend,
                    &mut edges,
                );
                push_graph_edges(
                    index,
                    document,
                    file_id,
                    source,
                    &data.extends,
                    GraphEdgeKind::Extend,
                    &mut edges,
                );
                push_normalized_extend_edges(
                    index,
                    document,
                    file_id,
                    source,
                    &data.extends,
                    &mut edges,
                );
            }
            EntryKind::Module(data) => {
                let range = text_range_from_lsp_range(
                    document,
                    file_id,
                    entry.location.range,
                    "graph node",
                );
                nodes.push(GraphNodeFact::new(
                    source.clone(),
                    GraphNodeKind::Module,
                    range,
                ));
                if let Some(singleton_source) = source.to_singleton_namespace() {
                    nodes.push(GraphNodeFact::new(
                        singleton_source,
                        GraphNodeKind::Module,
                        range,
                    ));
                }
                push_graph_edges(
                    index,
                    document,
                    file_id,
                    source,
                    &data.includes,
                    GraphEdgeKind::Include,
                    &mut edges,
                );
                push_graph_edges(
                    index,
                    document,
                    file_id,
                    source,
                    &data.prepends,
                    GraphEdgeKind::Prepend,
                    &mut edges,
                );
                push_graph_edges(
                    index,
                    document,
                    file_id,
                    source,
                    &data.extends,
                    GraphEdgeKind::Extend,
                    &mut edges,
                );
                push_normalized_extend_edges(
                    index,
                    document,
                    file_id,
                    source,
                    &data.extends,
                    &mut edges,
                );
            }
            EntryKind::Method(_)
            | EntryKind::Constant(_)
            | EntryKind::LocalVariable(_)
            | EntryKind::InstanceVariable(_)
            | EntryKind::ClassVariable(_)
            | EntryKind::GlobalVariable(_)
            | EntryKind::Reference(_) => {}
        }
    }

    (nodes, edges)
}

fn push_singleton_superclass_edge(
    index: &RubyIndex,
    document: &RubyDocument,
    file_id: SourceFileId,
    source: &FullyQualifiedName,
    superclass_ref: &MixinRef,
    edges: &mut Vec<GraphEdgeFact>,
) {
    let Some(singleton_source) = source.to_singleton_namespace() else {
        return;
    };
    let Some(superclass_target) = utils::resolve_constant_fqn_from_parts(
        index,
        &superclass_ref.parts,
        superclass_ref.absolute,
        source,
    ) else {
        return;
    };
    let Some(singleton_target) = superclass_target.to_singleton_namespace() else {
        return;
    };

    edges.push(GraphEdgeFact::new(
        singleton_source,
        singleton_target,
        GraphEdgeKind::Superclass,
        text_range_from_lsp_range(
            document,
            file_id,
            superclass_ref.location.range,
            "graph singleton superclass edge",
        ),
    ));
}

fn push_normalized_extend_edges(
    index: &RubyIndex,
    document: &RubyDocument,
    file_id: SourceFileId,
    source: &FullyQualifiedName,
    mixin_refs: &[MixinRef],
    edges: &mut Vec<GraphEdgeFact>,
) {
    let Some(singleton_source) = source.to_singleton_namespace() else {
        return;
    };

    for mixin_ref in mixin_refs {
        if let Some(target) = utils::resolve_constant_fqn_from_parts(
            index,
            &mixin_ref.parts,
            mixin_ref.absolute,
            source,
        ) {
            edges.push(GraphEdgeFact::new(
                singleton_source.clone(),
                target,
                GraphEdgeKind::Include,
                text_range_from_lsp_range(
                    document,
                    file_id,
                    mixin_ref.location.range,
                    "graph normalized extend edge",
                ),
            ));
        }
    }
}

fn push_graph_edges(
    index: &RubyIndex,
    document: &RubyDocument,
    file_id: SourceFileId,
    source: &FullyQualifiedName,
    mixin_refs: &[MixinRef],
    kind: GraphEdgeKind,
    edges: &mut Vec<GraphEdgeFact>,
) {
    for mixin_ref in mixin_refs {
        push_graph_edge(index, document, file_id, source, mixin_ref, kind, edges);
    }
}

fn push_graph_edge(
    index: &RubyIndex,
    document: &RubyDocument,
    file_id: SourceFileId,
    source: &FullyQualifiedName,
    mixin_ref: &MixinRef,
    kind: GraphEdgeKind,
    edges: &mut Vec<GraphEdgeFact>,
) {
    if let Some(target) =
        utils::resolve_constant_fqn_from_parts(index, &mixin_ref.parts, mixin_ref.absolute, source)
    {
        edges.push(GraphEdgeFact::new(
            source.clone(),
            target,
            kind,
            text_range_from_lsp_range(document, file_id, mixin_ref.location.range, "graph edge"),
        ));
    }
}

fn symbol_kind_for_entry(entry_kind: &EntryKind) -> Option<SymbolKind> {
    match entry_kind {
        EntryKind::Class(_) => Some(SymbolKind::Class),
        EntryKind::Module(_) => Some(SymbolKind::Module),
        EntryKind::Method(_) => Some(SymbolKind::Method),
        EntryKind::Constant(_) => Some(SymbolKind::Constant),
        EntryKind::LocalVariable(_) => Some(SymbolKind::LocalVariable),
        EntryKind::InstanceVariable(_) => Some(SymbolKind::InstanceVariable),
        EntryKind::ClassVariable(_) => Some(SymbolKind::ClassVariable),
        EntryKind::GlobalVariable(_) => Some(SymbolKind::GlobalVariable),
        EntryKind::Reference(_) => None,
    }
}

fn byte_offset_u32(byte_offset: usize, message: &str) -> u32 {
    u32::try_from(byte_offset).unwrap_or_else(|_| {
        panic!(
            "INVARIANT VIOLATED: {message}. \
             This is a bug because ruby-analysis-core TextRange currently stores u32 offsets. \
             Fix: widen TextRange offsets before indexing files larger than u32::MAX bytes."
        )
    })
}

fn text_range_from_lsp_range(
    document: &RubyDocument,
    file_id: SourceFileId,
    range: Range,
    kind: &str,
) -> TextRange {
    let start_byte = byte_offset_u32(
        document.position_to_offset(range.start),
        &format!("{kind} start offset exceeded u32"),
    );
    let end_byte = byte_offset_u32(
        document.position_to_offset(range.end),
        &format!("{kind} end offset exceeded u32"),
    );
    TextRange::new(file_id, start_byte, end_byte)
}
