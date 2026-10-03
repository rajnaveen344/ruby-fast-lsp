//! File removal: every component forgets the file, other files observe its
//! absence, and identities are never reissued.

use super::*;

const CHILD_SOURCE: &str = "class Child < Parent; include Mixin; end\nParent.new.greet\n";
const PARENT_SOURCE: &str = "class Parent; def greet; end; end\nmodule Mixin; end\n";

fn constant(name: &str) -> RubyConstant {
    RubyConstant::new(name).unwrap()
}

fn namespace(name: &str) -> FullyQualifiedName {
    FullyQualifiedName::namespace(vec![constant(name)])
}

fn greet() -> RubyMethod {
    RubyMethod::new("greet").unwrap()
}

fn greet_fqn() -> FullyQualifiedName {
    FullyQualifiedName::method(vec![constant("Parent")], greet())
}

/// A child class whose superclass, include, constant read, and method call
/// all target declarations in another file.
fn child_facts(file_id: SourceFileId) -> FileAnalysis {
    let child = namespace("Child");
    FileAnalysis {
        symbols: vec![SymbolFact::new(
            child.clone(),
            SymbolKind::Class,
            TextRange::new(file_id, 0, 40),
        )],
        graph_nodes: vec![
            GraphNodeFact::new(
                child.clone(),
                GraphNodeKind::Class,
                TextRange::new(file_id, 0, 40),
            ),
            GraphNodeFact::new(
                child.to_singleton_namespace().unwrap(),
                GraphNodeKind::Class,
                TextRange::new(file_id, 0, 40),
            ),
        ],
        unresolved_graph_edges: vec![
            UnresolvedGraphEdgeFact::new(
                child.clone(),
                vec![constant("Parent")],
                false,
                child.clone(),
                GraphEdgeKind::Superclass,
                TextRange::new(file_id, 14, 20),
            ),
            UnresolvedGraphEdgeFact::new(
                child.clone(),
                vec![constant("Mixin")],
                false,
                child,
                GraphEdgeKind::Include,
                TextRange::new(file_id, 22, 35),
            ),
        ],
        reference_candidates: vec![
            ReferenceCandidate::constant(
                TextRange::new(file_id, 14, 20),
                vec![constant("Parent")],
                Vec::new(),
            ),
            explicit_method_call_candidate(
                TextRange::new(file_id, 52, 57),
                TextRange::new(file_id, 41, 57),
                vec![constant("Parent")],
                NamespaceKind::Instance,
                greet(),
                None,
            ),
        ],
        ..Default::default()
    }
}

/// Facts covering every component the parent file can own.
fn parent_facts(file_id: SourceFileId) -> FileAnalysis {
    let parent = namespace("Parent");
    let mixin = namespace("Mixin");
    let greet_range = TextRange::new(file_id, 14, 28);
    FileAnalysis {
        symbols: vec![
            SymbolFact::new(
                parent.clone(),
                SymbolKind::Class,
                TextRange::new(file_id, 0, 33),
            ),
            SymbolFact::new(
                mixin.clone(),
                SymbolKind::Module,
                TextRange::new(file_id, 34, 51),
            ),
        ],
        graph_nodes: vec![
            GraphNodeFact::new(
                parent.clone(),
                GraphNodeKind::Class,
                TextRange::new(file_id, 0, 33),
            ),
            GraphNodeFact::new(
                parent.to_singleton_namespace().unwrap(),
                GraphNodeKind::Class,
                TextRange::new(file_id, 0, 33),
            ),
            GraphNodeFact::new(
                mixin,
                GraphNodeKind::Module,
                TextRange::new(file_id, 34, 51),
            ),
        ],
        methods: vec![MethodFact::new(greet_fqn(), parent, greet_range)],
        types: vec![TypeFact::new(
            TypeSubject::MethodReturn(greet_fqn()),
            RubyType::string(),
            greet_range,
            TypeProvenance::Rbs,
        )],
        diagnostics: vec![DiagnosticFact {
            range: TextRange::new(file_id, 0, 5),
            severity: DiagnosticSeverity::Warning,
            code: "indexer-note".to_string(),
            message: "indexer diagnostic owned by the parent file".to_string(),
        }],
        ..Default::default()
    }
}

/// A project with the child file only, and one that also indexed and then
/// removed the parent file.
fn child_only_and_removed_parent() -> (Project, Project, SourceFileId, SourceFileId) {
    let mut never_added = Project::new();
    let child_file = register_project_file(&mut never_added, "child.rb", CHILD_SOURCE);
    never_added.update(child_file, child_facts(child_file), ResolveMode::Immediate);

    let mut removed = Project::new();
    let removed_child = register_project_file(&mut removed, "child.rb", CHILD_SOURCE);
    let parent_file = register_project_file(&mut removed, "parent.rb", PARENT_SOURCE);
    assert_eq!(removed_child, child_file);
    removed.update(
        removed_child,
        child_facts(removed_child),
        ResolveMode::Deferred,
    );
    removed.update(
        parent_file,
        parent_facts(parent_file),
        ResolveMode::Immediate,
    );
    assert!(removed.remove(parent_file, ResolveMode::Immediate));
    (never_added, removed, child_file, parent_file)
}

#[test]
fn removed_file_leaves_the_registered_sources() {
    let (_, engine, child_file, parent_file) = child_only_and_removed_parent();

    assert_eq!(engine.view().file_count(), 1);
    assert_eq!(
        engine
            .view()
            .files()
            .map(|file| file.id)
            .collect::<Vec<_>>(),
        vec![child_file]
    );
    assert_eq!(engine.view().file_id("parent.rb"), None);
    assert!(engine.view().file(parent_file).is_none());
    assert!(engine
        .view()
        .semantic_export_fingerprint(parent_file)
        .is_none());
    assert!(engine
        .view()
        .source_snapshot_for_path("parent.rb")
        .is_none());
}

#[test]
fn removal_matches_a_project_that_never_added_the_file() {
    let (never_added, removed, _, _) = child_only_and_removed_parent();

    assert_eq!(
        removed.view().semantic_result_fingerprint(),
        never_added.view().semantic_result_fingerprint()
    );
    assert_eq!(
        removed.view().semantic_context_fingerprint(),
        never_added.view().semantic_context_fingerprint()
    );
    assert_eq!(removed.view().stats(), never_added.view().stats());
    assert_eq!(
        removed.view().unresolved_graph_edges().len(),
        never_added.view().unresolved_graph_edges().len()
    );
}

#[test]
fn edges_into_the_removed_file_become_unresolved_again() {
    let mut engine = Project::new();
    let child_file = register_project_file(&mut engine, "child.rb", CHILD_SOURCE);
    let parent_file = register_project_file(&mut engine, "parent.rb", PARENT_SOURCE);
    engine.update(child_file, child_facts(child_file), ResolveMode::Deferred);
    engine.update(
        parent_file,
        parent_facts(parent_file),
        ResolveMode::Immediate,
    );
    let child = namespace("Child");
    let unresolved_constants = |engine: &Project| {
        engine
            .view()
            .diagnostic_facts_in_file(child_file)
            .into_iter()
            .filter(|fact| fact.code == "unresolved-constant")
            .count()
    };
    assert!(engine.view().unresolved_graph_edges().is_empty());
    assert_eq!(unresolved_constants(&engine), 0);
    assert!(engine
        .view()
        .graph_edges_from(&child)
        .iter()
        .any(|edge| edge.kind == GraphEdgeKind::Superclass && edge.target == namespace("Parent")));

    let revision = engine.query_cache_identity().1;
    assert!(engine.remove(parent_file, ResolveMode::Immediate));
    assert!(engine.query_cache_identity().1 > revision);

    assert_eq!(engine.view().unresolved_graph_edges().len(), 2);
    assert!(!engine
        .view()
        .graph_edges_from(&child)
        .iter()
        .any(|edge| { edge.target == namespace("Parent") || edge.target == namespace("Mixin") }));
    assert!(!engine
        .view()
        .graph_edges_from(&child.to_singleton_namespace().unwrap())
        .iter()
        .any(|edge| edge.kind == GraphEdgeKind::Superclass));
    assert_eq!(
        unresolved_constants(&engine),
        1,
        "the superclass read must be reported again once its target is removed"
    );
}

#[test]
fn references_and_callees_no_longer_reach_removed_methods() {
    let mut engine = Project::new();
    let child_file = register_project_file(&mut engine, "child.rb", CHILD_SOURCE);
    let parent_file = register_project_file(&mut engine, "parent.rb", PARENT_SOURCE);
    engine.update(child_file, child_facts(child_file), ResolveMode::Deferred);
    engine.update(
        parent_file,
        parent_facts(parent_file),
        ResolveMode::Immediate,
    );
    let parent = namespace("Parent");
    assert_eq!(engine.view().reference_facts_for(&greet_fqn()).len(), 1);
    assert_eq!(engine.view().reference_facts_for(&parent).len(), 1);
    assert_eq!(
        ask_any(&engine.view(), &parent, &greet(), MethodWant::Callees)
            .into_callees()
            .map(|callees| callees.len()),
        Some(1)
    );

    assert!(engine.remove(parent_file, ResolveMode::Immediate));

    assert!(engine.view().reference_facts_for(&greet_fqn()).is_empty());
    assert!(engine.view().reference_facts_for(&parent).is_empty());
    assert!(engine.view().references_in_file(child_file).is_empty());
    assert!(
        ask_any(&engine.view(), &parent, &greet(), MethodWant::Callees)
            .into_callees()
            .unwrap_or_default()
            .iter()
            .all(|callee| callee.definition_ranges.is_empty())
    );
    assert!(engine.view().method_facts_in_file(parent_file).is_empty());
    assert!(engine
        .view()
        .diagnostic_facts_in_file(parent_file)
        .is_empty());
}

#[test]
fn stale_snapshots_are_rejected_after_removal() {
    let mut engine = Project::new();
    let parent_file = register_project_file(&mut engine, "parent.rb", PARENT_SOURCE);
    let snapshot = engine.view().source_snapshot_for_path("parent.rb").unwrap();
    engine.update(
        parent_file,
        parent_facts(parent_file),
        ResolveMode::Immediate,
    );

    assert!(engine.remove_if_snapshot(snapshot, ResolveMode::Immediate));
    assert!(!engine.remove_if_snapshot(snapshot, ResolveMode::Immediate));
    assert_eq!(
        engine.update_if_snapshot(snapshot, parent_facts(parent_file), ResolveMode::Immediate),
        None
    );
    assert_eq!(
        engine.register_file_borrowed_if_snapshot(
            PathBuf::from("parent.rb"),
            PARENT_SOURCE,
            SourceKind::Project,
            Some(snapshot),
        ),
        None
    );
    assert_eq!(engine.view().file_count(), 0);
    assert!(engine.view().reference_facts_for(&greet_fqn()).is_empty());
    assert!(engine.view().method_facts_in_file(parent_file).is_empty());
}

#[test]
fn remove_if_snapshot_keeps_a_file_edited_after_the_snapshot() {
    let mut engine = Project::new();
    let parent_file = register_project_file(&mut engine, "parent.rb", PARENT_SOURCE);
    let snapshot = engine.view().source_snapshot_for_path("parent.rb").unwrap();
    register_project_file(&mut engine, "parent.rb", "class Parent; end\n");

    assert!(!engine.remove_if_snapshot(snapshot, ResolveMode::Immediate));
    assert_eq!(engine.view().file_id("parent.rb"), Some(parent_file));
}

#[test]
fn reregistering_a_removed_path_starts_empty_with_a_new_id() {
    let mut engine = Project::new();
    let parent_file = register_project_file(&mut engine, "parent.rb", PARENT_SOURCE);
    engine.update(
        parent_file,
        parent_facts(parent_file),
        ResolveMode::Immediate,
    );
    assert!(engine.remove(parent_file, ResolveMode::Immediate));

    let reregistered = register_project_file(&mut engine, "parent.rb", PARENT_SOURCE);

    assert_ne!(reregistered, parent_file);
    assert_eq!(engine.view().file_id("parent.rb"), Some(reregistered));
    assert!(engine.view().method_facts_in_file(reregistered).is_empty());
    assert!(engine
        .view()
        .diagnostic_facts_in_file(reregistered)
        .is_empty());
    assert!(engine.view().references_in_file(reregistered).is_empty());
    assert!(!engine
        .hierarchy
        .has_node(&engine.names, &namespace("Parent")));
    assert!(engine
        .view()
        .semantic_export_fingerprint(reregistered)
        .is_none());
    assert_eq!(
        engine.update(
            reregistered,
            FileAnalysis::default(),
            ResolveMode::Immediate
        ),
        SemanticChange::InitialIndex
    );
}

#[test]
fn removing_an_unknown_file_is_a_no_op() {
    let mut engine = Project::new();
    let parent_file = register_project_file(&mut engine, "parent.rb", PARENT_SOURCE);
    engine.update(
        parent_file,
        parent_facts(parent_file),
        ResolveMode::Immediate,
    );
    let identity = engine.query_cache_identity();
    let fingerprint = engine.view().semantic_result_fingerprint();

    assert!(!engine.remove(SourceFileId(parent_file.0 + 1), ResolveMode::Immediate));
    assert!(engine.remove(parent_file, ResolveMode::Deferred));
    let after_removal = engine.query_cache_identity();
    assert!(!engine.remove(parent_file, ResolveMode::Immediate));

    assert_eq!(engine.query_cache_identity(), after_removal);
    assert!(after_removal.1 > identity.1);
    assert_ne!(engine.view().semantic_result_fingerprint(), fingerprint);
}
