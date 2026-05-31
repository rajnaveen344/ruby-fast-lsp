use crate::core::{
    FullyQualifiedName, GraphEdgeKind, GraphNodeFact, GraphNodeKind, MethodFact,
    ReferenceCandidate, RubyConstant, RubyMethod, RubyType, SymbolFact, SymbolKind, TypeProvenance,
    TypeSubject, UnresolvedGraphEdgeFact,
};

use super::*;

fn constant_subject(name: &str) -> TypeSubject {
    TypeSubject::Constant(FullyQualifiedName::constant(vec![
        RubyConstant::new(name).unwrap()
    ]))
}

fn register_project_file(
    engine: &mut AnalysisEngine,
    path: impl Into<std::path::PathBuf>,
    source: impl Into<String>,
) -> SourceFileId {
    engine.register_file(SourceFileInput {
        path: path.into(),
        content: source.into(),
        kind: SourceKind::Project,
    })
}

#[test]
fn file_ids_are_stable_across_updates() {
    let mut engine = AnalysisEngine::new();

    let first = register_project_file(&mut engine, "app/user.rb", "A = 1");
    let second = register_project_file(&mut engine, "app/user.rb", "A = 2");

    assert_eq!(first, second);
    assert_eq!(engine.file_count(), 1);
    let file = engine.file(first).unwrap();
    assert_eq!(file.line_index.len(), "A = 2".len());
    assert!(file.source_text().is_none());
}

#[test]
fn source_kind_updates_with_file() {
    let mut engine = AnalysisEngine::new();

    let file_id = engine.register_file(SourceFileInput {
        path: "gems/foo.rb".into(),
        content: "module Foo; end".into(),
        kind: SourceKind::Gem,
    });

    assert_eq!(engine.file(file_id).unwrap().kind, SourceKind::Gem);
}

#[test]
fn type_at_reads_engine_owned_store() {
    let mut engine = AnalysisEngine::new();
    let file_id = register_project_file(&mut engine, "app/user.rb", "A = 1");
    let subject = constant_subject("A");

    engine.replace_facts(
        file_id,
        FileFacts {
            types: vec![TypeFact::new(
                subject.clone(),
                RubyType::integer(),
                engine.text_range(file_id, 0, 5),
                TypeProvenance::Assignment,
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    match engine.type_at(&subject, file_id, 4) {
        TypeResolution::Resolved(fact) => assert_eq!(fact.ruby_type, RubyType::integer()),
        other => panic!("expected resolved type fact, got {other:?}"),
    }
}

#[test]
fn replace_facts_removes_stale_type_facts() {
    let mut engine = AnalysisEngine::new();
    let file_id = register_project_file(&mut engine, "app/user.rb", "A = 1");
    let subject = constant_subject("A");

    engine.replace_facts(
        file_id,
        FileFacts {
            types: vec![TypeFact::new(
                subject.clone(),
                RubyType::integer(),
                engine.text_range(file_id, 0, 5),
                TypeProvenance::Assignment,
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    engine.replace_facts(
        file_id,
        FileFacts {
            types: vec![TypeFact::new(
                subject.clone(),
                RubyType::string(),
                engine.text_range(file_id, 10, 15),
                TypeProvenance::Assignment,
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert_eq!(
        engine.type_at(&subject, file_id, 4),
        TypeResolution::Unresolved
    );
    match engine.type_at(&subject, file_id, 12) {
        TypeResolution::Resolved(fact) => assert_eq!(fact.ruby_type, RubyType::string()),
        other => panic!("expected replacement fact, got {other:?}"),
    }
}

#[test]
fn replace_facts_removes_stale_symbol_facts() {
    let mut engine = AnalysisEngine::new();
    let file_id = register_project_file(&mut engine, "app/user.rb", "class User; end");
    let fqn = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);

    engine.replace_facts(
        file_id,
        FileFacts {
            symbols: vec![SymbolFact::new(
                fqn.clone(),
                SymbolKind::Class,
                engine.text_range(file_id, 0, 10),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    engine.replace_facts(
        file_id,
        FileFacts {
            symbols: vec![SymbolFact::new(
                fqn.clone(),
                SymbolKind::Class,
                engine.text_range(file_id, 20, 30),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    let facts = engine.symbol_facts_for(&fqn);
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].range.start_byte, 20);
}

#[test]
fn reference_candidate_resolves_when_definition_arrives_later() {
    let mut engine = AnalysisEngine::new();
    let ref_file = register_project_file(&mut engine, "app/use_user.rb", "User.new");
    let def_file = register_project_file(&mut engine, "app/user.rb", "class User; end");
    let user_name = RubyConstant::new("User").unwrap();
    let user = FullyQualifiedName::namespace(vec![user_name]);

    engine.replace_facts(
        ref_file,
        FileFacts {
            reference_candidates: vec![ReferenceCandidate::constant(
                TextRange::new(ref_file, 0, 4),
                user.namespace_parts(),
                Vec::new(),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert!(engine.reference_facts_for(&user).is_empty());
    assert!(engine
        .diagnostic_facts_in_file(ref_file)
        .iter()
        .any(|fact| fact.code == "unresolved-constant"));

    engine.replace_facts(
        def_file,
        FileFacts {
            graph_nodes: vec![GraphNodeFact::new(
                user.clone(),
                GraphNodeKind::Class,
                TextRange::new(def_file, 0, 14),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert_eq!(engine.reference_facts_for(&user).len(), 1);
    assert!(engine
        .diagnostic_facts_in_file(ref_file)
        .iter()
        .all(|fact| fact.code != "unresolved-constant"));
}

#[test]
fn method_candidate_resolves_when_method_definition_arrives_later() {
    let mut engine = AnalysisEngine::new();
    let ref_file = register_project_file(&mut engine, "app/use_user.rb", "user.name");
    let def_file =
        register_project_file(&mut engine, "app/user.rb", "class User; def name; end; end");
    let user_name = RubyConstant::new("User").unwrap();
    let user = FullyQualifiedName::namespace(vec![user_name]);
    let method = RubyMethod::new("name").unwrap();
    let method_fqn = FullyQualifiedName::method(user.namespace_parts(), method);

    engine.replace_facts(
        def_file,
        FileFacts {
            graph_nodes: vec![GraphNodeFact::new(
                user.clone(),
                GraphNodeKind::Class,
                TextRange::new(def_file, 0, 10),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    engine.replace_facts(
        ref_file,
        FileFacts {
            reference_candidates: vec![ReferenceCandidate::method(
                TextRange::new(ref_file, 5, 9),
                crate::core::MethodReferenceCandidate {
                    owner: user.namespace_parts(),
                    owner_kind: crate::core::NamespaceKind::Instance,
                    method,
                    is_super: false,
                    access: crate::core::MethodReferenceAccess::ExplicitReceiver,
                    caller: None,
                    diagnostics: crate::core::MethodReferenceDiagnostics {
                        diagnostic_range: TextRange::new(ref_file, 5, 9),
                        receiver_label: Some("User".to_string()),
                        diagnose_unresolved: true,
                        allow_unindexed_owner: false,
                        signature: crate::core::MethodCallSignatureCandidate::default(),
                    },
                },
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert_eq!(engine.reference_facts_for(&method_fqn).len(), 1);
    assert!(engine
        .diagnostic_facts_in_file(ref_file)
        .iter()
        .any(|fact| fact.code == "unresolved-method"));

    engine.replace_facts(
        def_file,
        FileFacts {
            graph_nodes: vec![GraphNodeFact::new(
                user.clone(),
                GraphNodeKind::Class,
                TextRange::new(def_file, 0, 10),
            )],
            methods: vec![MethodFact::new(
                method_fqn.clone(),
                FullyQualifiedName::namespace_with_kind(
                    user.namespace_parts(),
                    crate::core::NamespaceKind::Instance,
                ),
                TextRange::new(def_file, 12, 20),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert_eq!(engine.reference_facts_for(&method_fqn).len(), 1);
    assert!(engine
        .diagnostic_facts_in_file(ref_file)
        .iter()
        .all(|fact| fact.code != "unresolved-method"));
}

#[test]
fn graph_update_retries_unresolved_edges_when_target_arrives() {
    let mut engine = AnalysisEngine::new();
    let user_file = register_project_file(&mut engine, "user.rb", "class User; include Auth; end");
    let auth_file = register_project_file(&mut engine, "auth.rb", "module Auth; end");

    let user = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);
    let auth = FullyQualifiedName::namespace(vec![RubyConstant::new("Auth").unwrap()]);
    engine.replace_facts(
        user_file,
        FileFacts {
            graph_nodes: vec![GraphNodeFact::new(
                user.clone(),
                GraphNodeKind::Class,
                TextRange::new(user_file, 0, 10),
            )],
            unresolved_graph_edges: vec![UnresolvedGraphEdgeFact::new(
                user.clone(),
                vec![RubyConstant::new("Auth").unwrap()],
                false,
                user.clone(),
                GraphEdgeKind::Include,
                TextRange::new(user_file, 12, 24),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    assert_eq!(engine.unresolved_graph_edges().len(), 1);

    engine.replace_facts(
        auth_file,
        FileFacts {
            graph_nodes: vec![GraphNodeFact::new(
                auth.clone(),
                GraphNodeKind::Module,
                TextRange::new(auth_file, 0, 11),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert!(engine.unresolved_graph_edges().is_empty());
    assert!(engine
        .graph_edges_from(&user)
        .iter()
        .any(|edge| edge.target == auth && edge.kind == GraphEdgeKind::Include));
}

#[test]
#[should_panic(expected = "file analysis references unknown source file id")]
fn rejects_type_fact_for_unknown_file() {
    let mut engine = AnalysisEngine::new();
    let subject = constant_subject("A");

    engine.replace_facts(
        SourceFileId(99),
        FileFacts {
            types: vec![TypeFact::new(
                subject,
                RubyType::integer(),
                TextRange::new(SourceFileId(99), 0, 5),
                TypeProvenance::Assignment,
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
}
