use crate::core::names::fqn_id::FqnId;
use crate::core::{SourceFileId, TextRange};

use super::*;

fn file() -> SourceFileId {
    SourceFileId(1)
}

#[test]
fn replace_file_removes_stale_graph_facts_for_same_file_only() {
    let source = FqnId(1);
    let target = FqnId(2);
    let mut store = SemanticGraph::default();
    store.add_node(StoredGraphNodeFact::new(
        source,
        GraphNodeKind::Class,
        TextRange::new(file(), 0, 10),
    ));
    store.add_edge(StoredGraphEdgeFact::new(
        source,
        target,
        GraphEdgeKind::Superclass,
        TextRange::new(file(), 0, 10),
    ));

    store.replace_file(
        file(),
        [StoredGraphNodeFact::new(
            target,
            GraphNodeKind::Class,
            TextRange::new(file(), 20, 30),
        )],
        [],
        [],
    );

    assert!(store.nodes_for(source).is_empty());
    assert!(store.edges_from(source).is_empty());
    assert_eq!(store.nodes_for(target).len(), 1);
}

#[test]
fn superclass_candidates_survive_independent_file_lifecycles() {
    let first_file = SourceFileId(1);
    let second_file = SourceFileId(2);
    let source = FqnId(1);
    let first_target = FqnId(2);
    let second_target = FqnId(3);
    let mut store = SemanticGraph::default();
    store.add_edge(StoredGraphEdgeFact::new(
        source,
        first_target,
        GraphEdgeKind::Superclass,
        TextRange::new(first_file, 0, 10),
    ));
    store.add_edge(StoredGraphEdgeFact::new(
        source,
        second_target,
        GraphEdgeKind::Superclass,
        TextRange::new(second_file, 0, 10),
    ));

    let candidates = store.edges_from(source);
    assert_eq!(candidates.len(), 2);
    assert!(candidates.iter().any(|edge| edge.target == first_target));
    assert!(candidates.iter().any(|edge| edge.target == second_target));

    store.replace_file(second_file, [], [], []);
    assert_eq!(
        store.edges_from(source),
        vec![StoredGraphEdgeFact::new(
            source,
            first_target,
            GraphEdgeKind::Superclass,
            TextRange::new(first_file, 0, 10),
        )]
    );
}

#[test]
fn superclass_resolution_is_proof_first_and_ignores_only_implicit_object() {
    let source = FqnId(1);
    let object = FqnId(2);
    let parent = FqnId(3);
    let alternative = FqnId(4);
    let mut store = SemanticGraph::default();
    store.add_edge(
        StoredGraphEdgeFact::new(
            source,
            object,
            GraphEdgeKind::Superclass,
            TextRange::new(SourceFileId(1), 0, 10),
        )
        .with_provenance(GraphEdgeProvenance::ImplicitObject),
    );
    let parent_fact = StoredGraphEdgeFact::new(
        source,
        parent,
        GraphEdgeKind::Superclass,
        TextRange::new(SourceFileId(2), 4, 10),
    );
    store.add_edge(parent_fact);
    assert_eq!(
        store.superclass_resolution(source),
        StoredSuperclassResolution::Unique(parent_fact)
    );

    store.add_edge(StoredGraphEdgeFact::new(
        source,
        alternative,
        GraphEdgeKind::Superclass,
        TextRange::new(SourceFileId(3), 4, 10),
    ));
    assert_eq!(
        store.superclass_resolution(source),
        StoredSuperclassResolution::Ambiguous
    );
}

#[test]
fn unresolved_explicit_superclass_source_index_tracks_take_and_reinsert() {
    let file_id = SourceFileId(1);
    let source = FqnId(1);
    let unresolved = StoredUnresolvedGraphEdgeFact::new(
        source,
        ConstLookupId(1),
        GraphEdgeKind::Superclass,
        TextRange::new(file_id, 0, 10),
    );
    let mut store = SemanticGraph::default();
    store.add_unresolved_edge(unresolved);
    assert!(store.has_unresolved_explicit_superclass(source));

    assert_eq!(store.take_unresolved_edges(), vec![unresolved]);
    assert!(!store.has_unresolved_explicit_superclass(source));

    store.add_unresolved_edge(unresolved);
    store.replace_file(file_id, [], [], []);
    assert!(!store.has_unresolved_explicit_superclass(source));
}

#[test]
fn unresolved_explicit_edge_source_index_covers_include_and_ignores_implicit() {
    let file_id = SourceFileId(1);
    let source = FqnId(1);
    let include_edge = StoredUnresolvedGraphEdgeFact::new(
        source,
        ConstLookupId(1),
        GraphEdgeKind::Include,
        TextRange::new(file_id, 0, 10),
    );
    let implicit_superclass = StoredUnresolvedGraphEdgeFact::new(
        FqnId(2),
        ConstLookupId(2),
        GraphEdgeKind::Superclass,
        TextRange::new(file_id, 10, 20),
    )
    .with_provenance(GraphEdgeProvenance::ImplicitObject);
    let mut store = SemanticGraph::default();
    store.add_unresolved_edge(include_edge);
    store.add_unresolved_edge(implicit_superclass);

    assert!(store.has_explicit_unresolved_edge_from(source));
    assert!(!store.has_unresolved_explicit_superclass(source));
    assert!(!store.has_explicit_unresolved_edge_from(FqnId(2)));

    store.replace_file(file_id, [], [], []);
    assert!(!store.has_explicit_unresolved_edge_from(source));
}

#[test]
fn node_definition_file_index_tracks_only_files_that_require_node_cleanup() {
    let node_file = SourceFileId(1);
    let edge_only_file = SourceFileId(2);
    let source = FqnId(1);
    let target = FqnId(2);
    let mut store = SemanticGraph::default();

    store.add_node(StoredGraphNodeFact::new(
        source,
        GraphNodeKind::Class,
        TextRange::new(node_file, 0, 10),
    ));
    store.add_edge(StoredGraphEdgeFact::new(
        source,
        target,
        GraphEdgeKind::Include,
        TextRange::new(edge_only_file, 0, 10),
    ));

    assert!(store.node_definition_files.contains(&node_file));
    assert!(
        !store.node_definition_files.contains(&edge_only_file),
        "edge endpoint nodes must not trigger a global definition cleanup scan"
    );

    store.replace_file(node_file, [], [], []);
    assert!(!store.node_definition_files.contains(&node_file));
    assert!(store.nodes_for(source).is_empty());
}

#[test]
fn edge_queries_are_deterministic_independent_of_insertion_order() {
    let source = FqnId(1);
    let first_target = FqnId(2);
    let second_target = FqnId(3);
    let first = StoredGraphEdgeFact::new(
        source,
        first_target,
        GraphEdgeKind::Include,
        TextRange::new(SourceFileId(2), 4, 8),
    );
    let second = StoredGraphEdgeFact::new(
        source,
        second_target,
        GraphEdgeKind::Include,
        TextRange::new(SourceFileId(1), 12, 16),
    );

    let mut forward = SemanticGraph::default();
    forward.add_edge(first);
    forward.add_edge(second);
    let mut reverse = SemanticGraph::default();
    reverse.add_edge(second);
    reverse.add_edge(first);

    assert_eq!(forward.edges_from(source), reverse.edges_from(source));
    assert_eq!(
        forward.edges_to(first_target),
        reverse.edges_to(first_target)
    );
    assert_eq!(
        forward.edges_to(second_target),
        reverse.edges_to(second_target)
    );
}

#[test]
fn node_definition_queries_do_not_treat_edge_only_entries_as_namespaces() {
    let source = FqnId(1);
    let target = FqnId(2);
    let mut store = SemanticGraph::default();
    store.add_node(StoredGraphNodeFact::new(
        source,
        GraphNodeKind::Class,
        TextRange::new(file(), 0, 10),
    ));
    store.add_edge(StoredGraphEdgeFact::new(
        source,
        target,
        GraphEdgeKind::Superclass,
        TextRange::new(file(), 0, 10),
    ));

    assert!(store.has_node_definition(source));
    assert!(!store.has_node_definition(target));
    assert_eq!(store.first_node_kind(source), Some(GraphNodeKind::Class));
    assert_eq!(store.latest_node_kind(source), Some(GraphNodeKind::Class));
    assert_eq!(store.first_node_kind(target), None);
    assert_eq!(store.latest_node_kind(target), None);
    assert!(store.has_node_kind(source, GraphNodeKind::Class));
    assert!(!store.has_node_kind(source, GraphNodeKind::Module));
    assert!(!store.has_node_kind(target, GraphNodeKind::Class));
    assert_eq!(
        store.first_node_definition(source),
        Some((GraphNodeKind::Class, TextRange::new(file(), 0, 10)))
    );
    assert_eq!(store.first_node_definition(target), None);
}

#[test]
fn edges_from_kind_matches_filtered_edges_from_in_source_order() {
    let source = FqnId(1);
    let parent = FqnId(2);
    let early = FqnId(3);
    let late = FqnId(4);
    let template = FqnId(5);
    let mut store = SemanticGraph::default();
    store.add_node(StoredGraphNodeFact::new(
        source,
        GraphNodeKind::Class,
        TextRange::new(file(), 0, 80),
    ));
    store.add_edge(StoredGraphEdgeFact::new(
        source,
        late,
        GraphEdgeKind::Include,
        TextRange::new(file(), 60, 72),
    ));
    store.add_edge(StoredGraphEdgeFact::new(
        source,
        parent,
        GraphEdgeKind::Superclass,
        TextRange::new(file(), 0, 10),
    ));
    store.add_edge(StoredGraphEdgeFact::new(
        source,
        early,
        GraphEdgeKind::Include,
        TextRange::new(file(), 20, 32),
    ));
    store.add_edge(StoredGraphEdgeFact::new(
        source,
        template,
        GraphEdgeKind::ExecutionContextApplication,
        TextRange::new(file(), 40, 50),
    ));

    let includes = store.edges_from_kind(source, GraphEdgeKind::Include);
    assert_eq!(
        includes,
        vec![
            StoredGraphEdgeFact::new(
                source,
                early,
                GraphEdgeKind::Include,
                TextRange::new(file(), 20, 32),
            ),
            StoredGraphEdgeFact::new(
                source,
                late,
                GraphEdgeKind::Include,
                TextRange::new(file(), 60, 72),
            ),
        ],
        "kind-specific outgoing edges must sort by source range, not insertion order"
    );
    assert_eq!(
        includes,
        store
            .edges_from(source)
            .into_iter()
            .filter(|edge| edge.kind == GraphEdgeKind::Include)
            .collect::<Vec<_>>(),
        "edges_from_kind must match filtering the concatenated outgoing list"
    );
    assert_eq!(
        store
            .edges_from_kind(source, GraphEdgeKind::Superclass)
            .len(),
        1
    );
    assert!(store
        .edges_from_kind(source, GraphEdgeKind::Prepend)
        .is_empty());
    assert_eq!(
        store.ancestry_edges_from(source),
        store
            .edges_from(source)
            .into_iter()
            .filter(|edge| matches!(
                edge.kind,
                GraphEdgeKind::Superclass
                    | GraphEdgeKind::Include
                    | GraphEdgeKind::Prepend
                    | GraphEdgeKind::Extend
            ))
            .collect::<Vec<_>>(),
        "ancestry edges must omit execution-context applications while keeping source order"
    );
}

#[test]
fn latest_node_kind_follows_sorted_definition_order() {
    let fqn = FqnId(1);
    let earlier = SourceFileId(1);
    let later = SourceFileId(2);
    let mut store = SemanticGraph::default();
    store.add_node(StoredGraphNodeFact::new(
        fqn,
        GraphNodeKind::Module,
        TextRange::new(later, 0, 20),
    ));
    store.add_node(StoredGraphNodeFact::new(
        fqn,
        GraphNodeKind::Class,
        TextRange::new(earlier, 0, 10),
    ));

    assert_eq!(store.first_node_kind(fqn), Some(GraphNodeKind::Class));
    assert_eq!(store.latest_node_kind(fqn), Some(GraphNodeKind::Module));
    assert!(store.has_node_kind(fqn, GraphNodeKind::Class));
    assert!(store.has_node_kind(fqn, GraphNodeKind::Module));
    assert_eq!(
        store.first_node_definition(fqn),
        Some((GraphNodeKind::Class, TextRange::new(earlier, 0, 10)))
    );
}
