
use crate::core::{
    FullyQualifiedName, GraphEdgeKind, GraphNodeFact, GraphNodeKind, MethodFact,
    ReferenceCandidate, RubyConstant, RubyMethod, RubyType, SymbolFact, SymbolKind, TypeProvenance,
    TypeSubject, UnresolvedGraphEdgeFact,
};

use super::*;

fn constant_subject(name: &str) -> TypeSubject {
    TypeSubject::Constant(FullyQualifiedName::Constant(vec![
        RubyConstant::new(name).unwrap()
    ]))
}

#[test]
fn file_ids_are_stable_across_updates() {
    let mut engine = AnalysisEngine::new();

    let first = engine.open_or_update_file("app/user.rb", "A = 1");
    let second = engine.open_or_update_file("app/user.rb", "A = 2");

    assert_eq!(first, second);
    assert_eq!(engine.file_count(), 1);
    assert_eq!(engine.file(first).unwrap().source, "A = 2");
}

#[test]
fn source_kind_updates_with_file() {
    let mut engine = AnalysisEngine::new();

    let file_id =
        engine.open_or_update_file_with_kind("gems/foo.rb", "module Foo; end", SourceKind::Gem);

    assert_eq!(engine.file(file_id).unwrap().kind, SourceKind::Gem);
}

#[test]
fn type_at_reads_engine_owned_store() {
    let mut engine = AnalysisEngine::new();
    let file_id = engine.open_or_update_file("app/user.rb", "A = 1");
    let subject = constant_subject("A");

    engine.add_type_fact(TypeFact::new(
        subject.clone(),
        RubyType::integer(),
        engine.text_range(file_id, 0, 5),
        TypeProvenance::Assignment,
    ));

    match engine.type_at(&subject, file_id, 4) {
        TypeResolution::Resolved(fact) => assert_eq!(fact.ruby_type, RubyType::integer()),
        other => panic!("expected resolved type fact, got {other:?}"),
    }
}

#[test]
fn replace_type_facts_for_file_removes_stale_engine_facts() {
    let mut engine = AnalysisEngine::new();
    let file_id = engine.open_or_update_file("app/user.rb", "A = 1");
    let subject = constant_subject("A");

    engine.add_type_fact(TypeFact::new(
        subject.clone(),
        RubyType::integer(),
        engine.text_range(file_id, 0, 5),
        TypeProvenance::Assignment,
    ));
    engine.replace_type_facts_for_file(
        file_id,
        [TypeFact::new(
            subject.clone(),
            RubyType::string(),
            engine.text_range(file_id, 10, 15),
            TypeProvenance::Assignment,
        )],
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
fn replace_symbol_facts_for_file_removes_stale_engine_facts() {
    let mut engine = AnalysisEngine::new();
    let file_id = engine.open_or_update_file("app/user.rb", "class User; end");
    let fqn = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);

    engine.add_symbol_fact(SymbolFact::new(
        fqn.clone(),
        SymbolKind::Class,
        engine.text_range(file_id, 0, 10),
    ));
    engine.replace_symbol_facts_for_file(
        file_id,
        [SymbolFact::new(
            fqn.clone(),
            SymbolKind::Class,
            engine.text_range(file_id, 20, 30),
        )],
    );

    let facts = engine.symbol_facts_for(&fqn);
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].range.start_byte, 20);
}

#[test]
fn reference_candidate_resolves_when_definition_arrives_later() {
    let mut engine = AnalysisEngine::new();
    let ref_file = engine.open_or_update_file("app/use_user.rb", "User.new");
    let def_file = engine.open_or_update_file("app/user.rb", "class User; end");
    let user_name = RubyConstant::new("User").unwrap();
    let user = FullyQualifiedName::namespace(vec![user_name]);

    engine.replace_reference_candidates_for_file(
        ref_file,
        [ReferenceCandidate::constant(
            TextRange::new(ref_file, 0, 4),
            user.namespace_parts(),
            Vec::new(),
            "User",
        )],
    );

    assert!(engine.reference_facts_for(&user).is_empty());
    assert!(engine
        .diagnostic_facts_in_file(ref_file)
        .iter()
        .any(|fact| fact.code == "unresolved-constant"));

    engine.replace_graph_update_for_file(
        def_file,
        [GraphNodeFact::new(
            user.clone(),
            GraphNodeKind::Class,
            TextRange::new(def_file, 0, 14),
        )],
        [],
        [],
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
    let ref_file = engine.open_or_update_file("app/use_user.rb", "user.name");
    let def_file = engine.open_or_update_file("app/user.rb", "class User; def name; end; end");
    let user_name = RubyConstant::new("User").unwrap();
    let user = FullyQualifiedName::namespace(vec![user_name]);
    let method = RubyMethod::new("name").unwrap();
    let method_fqn = FullyQualifiedName::method(user.namespace_parts(), method);

    engine.replace_graph_update_for_file(
        def_file,
        [GraphNodeFact::new(
            user.clone(),
            GraphNodeKind::Class,
            TextRange::new(def_file, 0, 10),
        )],
        [],
        [],
    );
    engine.replace_reference_candidates_for_file(
        ref_file,
        [ReferenceCandidate::method(
            TextRange::new(ref_file, 5, 9),
            user.namespace_parts(),
            crate::core::NamespaceKind::Instance,
            method,
            None,
            TextRange::new(ref_file, 5, 9),
            Some("User".to_string()),
            true,
            crate::core::MethodCallSignatureCandidate::default(),
        )],
    );

    assert_eq!(engine.reference_facts_for(&method_fqn).len(), 1);
    assert!(engine
        .diagnostic_facts_in_file(ref_file)
        .iter()
        .any(|fact| fact.code == "unresolved-method"));

    engine.replace_method_facts_for_file(
        def_file,
        [MethodFact::new(
            method_fqn.clone(),
            FullyQualifiedName::namespace_with_kind(
                user.namespace_parts(),
                crate::core::NamespaceKind::Instance,
            ),
            TextRange::new(def_file, 12, 20),
        )],
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
    let user_file = engine.open_or_update_file("user.rb", "class User; include Auth; end");
    let auth_file = engine.open_or_update_file("auth.rb", "module Auth; end");

    let user = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);
    let auth = FullyQualifiedName::namespace(vec![RubyConstant::new("Auth").unwrap()]);
    engine.replace_graph_update_for_file(
        user_file,
        [GraphNodeFact::new(
            user.clone(),
            GraphNodeKind::Class,
            TextRange::new(user_file, 0, 10),
        )],
        [],
        [UnresolvedGraphEdgeFact::new(
            user.clone(),
            vec![RubyConstant::new("Auth").unwrap()],
            false,
            user.clone(),
            GraphEdgeKind::Include,
            TextRange::new(user_file, 12, 24),
        )],
    );
    assert_eq!(engine.unresolved_graph_edges().len(), 1);

    engine.replace_graph_update_for_file(
        auth_file,
        [GraphNodeFact::new(
            auth.clone(),
            GraphNodeKind::Module,
            TextRange::new(auth_file, 0, 11),
        )],
        [],
        [],
    );

    assert!(engine.unresolved_graph_edges().is_empty());
    assert!(engine
        .graph_edges_from(&user)
        .iter()
        .any(|edge| edge.target == auth && edge.kind == GraphEdgeKind::Include));
}

#[test]
#[should_panic(expected = "type fact references unknown source file id")]
fn rejects_type_fact_for_unknown_file() {
    let mut engine = AnalysisEngine::new();
    let subject = constant_subject("A");

    engine.add_type_fact(TypeFact::new(
        subject,
        RubyType::integer(),
        TextRange::new(SourceFileId(99), 0, 5),
        TypeProvenance::Assignment,
    ));
}
