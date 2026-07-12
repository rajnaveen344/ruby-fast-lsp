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
fn semantic_export_fingerprint_distinguishes_body_and_api_edits() {
    let mut engine = AnalysisEngine::new();
    let file_id = register_project_file(&mut engine, "app/user.rb", "def name; 'A'; end");
    let owner = FullyQualifiedName::try_from("Object").unwrap();
    let method_fqn =
        FullyQualifiedName::method(owner.namespace_parts(), RubyMethod::new("name").unwrap());
    let facts = |params: Vec<String>, start_byte: u32| FileFacts {
        methods: vec![MethodFact::with_params(
            method_fqn.clone(),
            owner.clone(),
            crate::core::TextRange::new(file_id, start_byte, start_byte + 4),
            params,
        )],
        ..Default::default()
    };

    assert_eq!(
        engine.replace_facts(file_id, facts(Vec::new(), 0), ResolveMode::Immediate),
        SemanticChange::InitialIndex
    );

    register_project_file(&mut engine, "app/user.rb", "\n\ndef name; 'B'; end");
    assert_eq!(
        engine.replace_facts(file_id, facts(Vec::new(), 2), ResolveMode::Immediate),
        SemanticChange::BodyOnly
    );

    assert_eq!(
        engine.replace_facts(
            file_id,
            facts(vec!["prefix".to_string()], 2),
            ResolveMode::Immediate,
        ),
        SemanticChange::ExportsChanged
    );
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
fn resolved_reference_definition_query_requires_one_exact_target() {
    let mut engine = AnalysisEngine::new();
    let source_file = register_project_file(&mut engine, "app/model.rb", "field :user");
    let user_file = register_project_file(&mut engine, "app/user.rb", "class User; end");
    let account_file = register_project_file(&mut engine, "app/account.rb", "class Account; end");
    let user = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);
    let account = FullyQualifiedName::namespace(vec![RubyConstant::new("Account").unwrap()]);
    let user_range = TextRange::new(user_file, 0, 10);
    let account_range = TextRange::new(account_file, 0, 13);

    engine.replace_facts(
        user_file,
        FileFacts {
            symbols: vec![SymbolFact::new(user.clone(), SymbolKind::Class, user_range)],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    engine.replace_facts(
        account_file,
        FileFacts {
            symbols: vec![SymbolFact::new(
                account.clone(),
                SymbolKind::Class,
                account_range,
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    let reference_range = TextRange::new(source_file, 7, 12);
    engine.replace_facts(
        source_file,
        FileFacts {
            reference_candidates: vec![ReferenceCandidate::resolved(
                reference_range,
                user.clone(),
                None,
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert_eq!(
        AnalysisQuery::new(&engine).resolved_reference_definition_ranges_at(source_file, 8),
        vec![user_range]
    );

    engine.replace_facts(
        source_file,
        FileFacts {
            reference_candidates: vec![
                ReferenceCandidate::resolved(reference_range, user, None),
                ReferenceCandidate::resolved(reference_range, account, None),
            ],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    assert!(
        AnalysisQuery::new(&engine)
            .resolved_reference_definition_ranges_at(source_file, 8)
            .is_empty(),
        "ambiguous resolved reference targets must not guess a definition"
    );
}

#[test]
fn exact_method_reference_uses_engine_resolution_and_lifecycle() {
    let mut engine = AnalysisEngine::new();
    let model_file = register_project_file(
        &mut engine,
        "app/models/user.rb",
        "class User; private; def normalize_account; end; end",
    );
    let callback_file = register_project_file(
        &mut engine,
        "app/models/callback.rb",
        "before_save :normalize_account",
    );
    let user_name = RubyConstant::new("User").unwrap();
    let user = FullyQualifiedName::namespace(vec![user_name]);
    let method = RubyMethod::new("normalize_account").unwrap();
    let method_range = TextRange::new(model_file, 20, 49);
    let reference_range = TextRange::new(callback_file, 13, 31);

    engine.replace_facts(
        model_file,
        FileFacts {
            graph_nodes: vec![GraphNodeFact::new(
                user.clone(),
                GraphNodeKind::Class,
                TextRange::new(model_file, 0, 10),
            )],
            methods: vec![MethodFact::new(
                FullyQualifiedName::method(user.namespace_parts(), method),
                user.clone(),
                method_range,
            )
            .with_visibility(crate::method_store::MethodVisibility::Private)],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    engine.replace_facts(
        callback_file,
        FileFacts {
            reference_candidates: vec![ReferenceCandidate::method_target(
                reference_range,
                user.namespace_parts(),
                crate::core::NamespaceKind::Instance,
                method,
                None,
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert_eq!(
        AnalysisQuery::new(&engine).resolved_reference_definition_ranges_at(callback_file, 15),
        vec![method_range],
        "exact callback target must use normal engine method lookup, including private methods"
    );
    assert_eq!(
        engine
            .reference_facts_for(&FullyQualifiedName::method(user.namespace_parts(), method))
            .len(),
        1,
        "exact callback target must participate in ordinary method references"
    );

    engine.replace_facts(callback_file, FileFacts::default(), ResolveMode::Immediate);
    assert!(
        AnalysisQuery::new(&engine)
            .resolved_reference_definition_ranges_at(callback_file, 15)
            .is_empty(),
        "removing callback facts must remove exact method navigation"
    );
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

#[test]
fn source_positions_use_utf16_code_units() {
    let mut engine = AnalysisEngine::new();
    let file_id = register_project_file(&mut engine, "unicode.rb", "a😀b\n");
    let file = engine
        .file(file_id)
        .expect("registered source should exist");

    assert_eq!(file.byte_offset_to_line_character(1), Some((0, 1)));
    assert_eq!(file.byte_offset_to_line_character(5), Some((0, 3)));
}

#[test]
fn constant_rename_rejects_external_only_definition() {
    let mut engine = AnalysisEngine::new();
    let file_id = engine.register_file(SourceFileInput {
        path: "gem/user.rb".into(),
        content: "class User\nend\n".to_string(),
        kind: SourceKind::Gem,
    });
    let user = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);
    engine.replace_facts(
        file_id,
        FileFacts {
            symbols: vec![
                SymbolFact::new(user, SymbolKind::Class, TextRange::new(file_id, 0, 14))
                    .with_name_range(TextRange::new(file_id, 6, 10)),
            ],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert!(engine
        .query()
        .constant_rename_target(&[RubyConstant::new("User").unwrap()], &[])
        .is_none());
}

#[test]
fn method_navigation_prefers_implementation_over_matching_rbs_declaration() {
    let mut engine = AnalysisEngine::new();
    let signature_file = engine.register_file(SourceFileInput {
        path: "sig/widget.rbs".into(),
        content: "class Widget\n  def encode: () -> String\nend\n".to_string(),
        kind: SourceKind::Signature,
    });
    let implementation_file = register_project_file(
        &mut engine,
        "lib/widget.rb",
        "class Widget\n  def encode = 'ok'\nend\n",
    );
    let owner = FullyQualifiedName::namespace(vec![RubyConstant::new("Widget").unwrap()]);
    let method_name = RubyMethod::new("encode").unwrap();
    let method = FullyQualifiedName::method(owner.namespace_parts(), method_name);
    let signature_range = TextRange::new(signature_file, 15, 39);
    let implementation_range = TextRange::new(implementation_file, 15, 32);

    engine.replace_facts(
        signature_file,
        FileFacts {
            symbols: vec![SymbolFact::new(
                owner.clone(),
                SymbolKind::Class,
                TextRange::new(signature_file, 0, 47),
            )],
            graph_nodes: vec![GraphNodeFact::new(
                owner.clone(),
                GraphNodeKind::Class,
                TextRange::new(signature_file, 0, 47),
            )],
            methods: vec![
                MethodFact::new(method.clone(), owner.clone(), signature_range)
                    .with_signature_metadata(None, Some("String".to_string())),
            ],
            types: vec![TypeFact::new(
                TypeSubject::MethodReturn(method.clone()),
                RubyType::string(),
                signature_range,
                TypeProvenance::Rbs,
            )],
            ..Default::default()
        },
        ResolveMode::Deferred,
    );
    engine.replace_facts(
        implementation_file,
        FileFacts {
            symbols: vec![SymbolFact::new(
                owner.clone(),
                SymbolKind::Class,
                TextRange::new(implementation_file, 0, 37),
            )],
            graph_nodes: vec![GraphNodeFact::new(
                owner.clone(),
                GraphNodeKind::Class,
                TextRange::new(implementation_file, 0, 37),
            )],
            methods: vec![MethodFact::new(method, owner.clone(), implementation_range)],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    let callees = engine
        .query()
        .resolve_method_callees(&owner, &method_name)
        .expect("method owner must resolve");
    assert_eq!(callees.len(), 1);
    assert_eq!(callees[0].definition_ranges, vec![implementation_range]);
    let signatures = engine
        .query()
        .resolve_method_signature_facts(&owner, &method_name);
    assert_eq!(signatures.len(), 1);
    assert_eq!(signatures[0].range, signature_range);
    assert_eq!(signatures[0].return_type_label.as_deref(), Some("String"));
    assert_eq!(
        engine
            .query()
            .method_return_type_for_receiver(&owner, &method_name),
        Some(RubyType::string())
    );
    assert_eq!(
        engine
            .query()
            .constant_definition_ranges(&[RubyConstant::new("Widget").unwrap()], &[],),
        vec![TextRange::new(implementation_file, 0, 37)]
    );
}
