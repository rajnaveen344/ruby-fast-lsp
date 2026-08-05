use crate::core::{
    DiagnosticFact, DiagnosticSeverity, FullyQualifiedName, GeneratedOwnerId, GraphEdgeFact,
    GraphEdgeKind, GraphNodeFact, GraphNodeKind, InferenceEvidence, InferenceTelemetry,
    MethodCalleeResolution, MethodFact, MethodReturnEquation, NamespaceKind, ReferenceCandidate,
    RubyConstant, RubyMethod, RubyType, SymbolFact, SymbolKind, TypeFact, TypeInferenceOutcome,
    TypeProvenance, TypeSubject, UnknownReason, UnresolvedGraphEdgeFact,
};

use super::*;
use crate::engine::resolution::{
    method_lookup_chain, method_lookup_chain_for_reference_cached, MethodLookupChainCache,
};
use crate::engine::AnalysisQueryCache;
use crate::ConstantLookupRequest;

fn constant_subject(name: &str) -> TypeSubject {
    TypeSubject::Constant(FullyQualifiedName::constant(vec![
        RubyConstant::new(name).unwrap()
    ]))
}

#[test]
fn name_registry_interns_one_owned_identity_with_stable_ids() {
    let mut names = NameRegistry::default();
    let user = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);
    let account = FullyQualifiedName::namespace(vec![RubyConstant::new("Account").unwrap()]);

    let user_id = names.intern_fqn(user.clone());
    let account_id = names.intern_fqn(account.clone());

    assert_eq!(names.intern_fqn(user.clone()), user_id);
    assert_ne!(user_id, account_id);
    assert_eq!(names.fqn_id(&user), Some(user_id));
    assert_eq!(names.fqn(user_id), Some(&user));
    assert_eq!(names.fqn(account_id), Some(&account));
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
fn namespace_existence_tracks_graph_node_replacement() {
    let mut engine = AnalysisEngine::new();
    let file_id = register_project_file(&mut engine, "app/user.rb", "class User; end");
    let user = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);

    engine.replace_facts(
        file_id,
        FileFacts {
            graph_nodes: vec![GraphNodeFact::new(
                user.clone(),
                GraphNodeKind::Class,
                crate::core::TextRange::new(file_id, 0, 15),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert!(engine.query().namespace_exists(&user));
    assert!(!engine
        .query()
        .namespace_exists(&FullyQualifiedName::namespace(vec![RubyConstant::new(
            "Missing"
        )
        .unwrap()])));

    engine.replace_facts(file_id, FileFacts::default(), ResolveMode::Immediate);
    assert!(!engine.query().namespace_exists(&user));
}

#[test]
fn method_rename_rejects_external_definition_even_with_exact_name_range() {
    let mut engine = AnalysisEngine::new();
    let file_id = engine.register_file(SourceFileInput {
        path: "gems/user.rb".into(),
        content: "class User; def name; end; end".into(),
        kind: SourceKind::Gem,
    });
    let user = FullyQualifiedName::namespace_with_kind(
        vec![RubyConstant::new("User").unwrap()],
        crate::core::NamespaceKind::Instance,
    );
    let method = RubyMethod::new("name").unwrap();
    engine.replace_facts(
        file_id,
        FileFacts {
            methods: vec![MethodFact::new(
                FullyQualifiedName::method(user.namespace_parts(), method),
                user,
                crate::core::TextRange::new(file_id, 12, 25),
            )
            .with_name_range(crate::core::TextRange::new(file_id, 16, 20))],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert!(
        AnalysisQuery::new(&engine)
            .method_rename_target_at(file_id, 17)
            .is_none(),
        "dependency sources are navigation inputs, never editable rename truth"
    );
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
fn semantic_context_fingerprint_is_path_independent_but_kind_and_fact_sensitive() {
    fn engine_with(path: &str, kind: SourceKind, method_name: &str) -> AnalysisEngine {
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(SourceFileInput {
            path: PathBuf::from(path),
            content: "class Shared; end".to_string(),
            kind,
        });
        let owner = FullyQualifiedName::try_from("Shared").unwrap();
        let method = RubyMethod::new(method_name).unwrap();
        engine.replace_facts(
            file_id,
            FileFacts {
                methods: vec![MethodFact::new(
                    FullyQualifiedName::method(owner.namespace_parts(), method),
                    owner,
                    TextRange::new(file_id, 0, 12),
                )],
                ..Default::default()
            },
            ResolveMode::Deferred,
        );
        engine
    }

    let first = engine_with("/runtime/a/shared.rb", SourceKind::Stub, "call");
    let same_semantics_other_path = engine_with("/runtime/b/shared.rb", SourceKind::Stub, "call");
    let different_kind = engine_with("/runtime/a/shared.rb", SourceKind::Gem, "call");
    let different_fact = engine_with("/runtime/a/shared.rb", SourceKind::Stub, "other");

    assert_eq!(
        first.semantic_context_fingerprint(),
        same_semantics_other_path.semantic_context_fingerprint()
    );
    assert_ne!(
        first.semantic_context_fingerprint(),
        different_kind.semantic_context_fingerprint()
    );
    assert_ne!(
        first.semantic_context_fingerprint(),
        different_fact.semantic_context_fingerprint()
    );
}

#[test]
fn semantic_result_fingerprint_is_file_id_independent_and_reference_sensitive() {
    fn engine_with(target_name: &str, reverse_registration: bool) -> AnalysisEngine {
        let mut engine = AnalysisEngine::new();
        let register_definitions = |engine: &mut AnalysisEngine| {
            register_project_file(
                engine,
                "app/models.rb",
                "class Alpha; end\nclass Beta; end\n",
            )
        };
        let register_call =
            |engine: &mut AnalysisEngine| register_project_file(engine, "app/call.rb", "Alpha\n");
        let (definitions_file, call_file) = if reverse_registration {
            let call = register_call(&mut engine);
            let definitions = register_definitions(&mut engine);
            (definitions, call)
        } else {
            let definitions = register_definitions(&mut engine);
            let call = register_call(&mut engine);
            (definitions, call)
        };
        let alpha = FullyQualifiedName::constant(vec![RubyConstant::new("Alpha").unwrap()]);
        let beta = FullyQualifiedName::constant(vec![RubyConstant::new("Beta").unwrap()]);
        engine.replace_facts(
            definitions_file,
            FileFacts {
                symbols: vec![
                    SymbolFact::new(
                        alpha.clone(),
                        SymbolKind::Constant,
                        TextRange::new(definitions_file, 6, 11),
                    ),
                    SymbolFact::new(
                        beta.clone(),
                        SymbolKind::Constant,
                        TextRange::new(definitions_file, 23, 27),
                    ),
                ],
                ..Default::default()
            },
            ResolveMode::Deferred,
        );
        let target = match target_name {
            "Alpha" => alpha,
            "Beta" => beta,
            other => panic!("unexpected semantic fingerprint fixture target {other}"),
        };
        engine.replace_facts(
            call_file,
            FileFacts {
                reference_candidates: vec![ReferenceCandidate::resolved(
                    TextRange::new(call_file, 0, 5),
                    target,
                    None,
                )],
                types: vec![TypeFact::new(
                    TypeSubject::Expression(TextRange::new(call_file, 0, 5)),
                    RubyType::string(),
                    TextRange::new(call_file, 0, 5),
                    TypeProvenance::Inferred,
                )],
                diagnostics: vec![DiagnosticFact::new(
                    TextRange::new(call_file, 0, 5),
                    DiagnosticSeverity::Information,
                    "fixture",
                    "stable fixture diagnostic",
                )],
                ..Default::default()
            },
            ResolveMode::Immediate,
        );
        engine
    }

    let alpha = engine_with("Alpha", false);
    let alpha_reversed = engine_with("Alpha", true);
    let beta = engine_with("Beta", false);

    assert_eq!(
        alpha.semantic_result_fingerprint(),
        alpha_reversed.semantic_result_fingerprint(),
        "engine-local file IDs and insertion order must not change semantic evidence"
    );
    assert_ne!(
        alpha.semantic_result_fingerprint(),
        beta.semantic_result_fingerprint(),
        "a different resolved definition target must change semantic evidence"
    );
}

#[test]
fn semantic_context_fingerprint_is_cross_process_stable() {
    fn fingerprint() -> String {
        let mut engine = AnalysisEngine::new();
        let file_id = engine.register_file(SourceFileInput {
            path: "stubs/widget.rb".into(),
            content: "class Widget; def call(value); end; end".into(),
            kind: SourceKind::Stub,
        });
        let owner = FullyQualifiedName::namespace(vec![RubyConstant::new("Widget").unwrap()]);
        let method = RubyMethod::new("call").unwrap();
        engine.replace_facts(
            file_id,
            FileFacts {
                symbols: vec![SymbolFact::new(
                    owner.clone(),
                    SymbolKind::Class,
                    TextRange::new(file_id, 0, 40),
                )],
                methods: vec![MethodFact::with_params(
                    FullyQualifiedName::method(owner.namespace_parts(), method),
                    owner.clone(),
                    TextRange::new(file_id, 14, 34),
                    vec!["value".to_string()],
                )],
                types: vec![TypeFact::new(
                    TypeSubject::MethodReturn(FullyQualifiedName::method(
                        owner.namespace_parts(),
                        method,
                    )),
                    RubyType::string(),
                    TextRange::new(file_id, 14, 34),
                    TypeProvenance::Inferred,
                )],
                graph_nodes: vec![GraphNodeFact::new(
                    owner,
                    GraphNodeKind::Class,
                    TextRange::new(file_id, 0, 40),
                )],
                ..FileFacts::default()
            },
            ResolveMode::Deferred,
        );
        engine
            .semantic_context_fingerprint()
            .stable_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    const CHILD_ENV: &str = "RUBY_FAST_LSP_FINGERPRINT_CHILD";
    if std::env::var_os(CHILD_ENV).is_some() {
        for index in 0..512 {
            let _ = RubyConstant::new(&format!("FingerprintNoise{index}")).unwrap();
        }
        println!("RUBY_FAST_LSP_FINGERPRINT={}", fingerprint());
        return;
    }

    let expected = fingerprint();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "engine::state::tests::semantic_context_fingerprint_is_cross_process_stable",
            "--nocapture",
        ])
        .env(CHILD_ENV, "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "fingerprint child failed:\nstdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let child = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("RUBY_FAST_LSP_FINGERPRINT="))
        .unwrap()
        .to_string();
    assert_eq!(child, expected);
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
fn expression_query_preserves_an_exact_unknown_proof_barrier() {
    let mut engine = AnalysisEngine::new();
    let file_id = register_project_file(&mut engine, "app/user.rb", "@value");
    let range = engine.text_range(file_id, 0, 6);

    engine.replace_facts(
        file_id,
        FileFacts {
            types: vec![TypeFact::new(
                TypeSubject::Expression(range),
                RubyType::Unknown,
                range,
                TypeProvenance::Flow,
            )],
            inference: InferenceEvidence {
                expression_unknown_reasons: vec![(range, UnknownReason::NoReachingAssignment)],
                ..Default::default()
            },
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert_eq!(
        engine.query().expression_type_at(file_id, 2),
        Some(RubyType::Unknown),
        "an exact Unknown expression must stop adapters from borrowing another concrete type"
    );
    assert_eq!(
        engine.query().expression_unknown_reason(range),
        Some(UnknownReason::NoReachingAssignment)
    );
    assert_eq!(
        engine.query().expression_unknown_reason_at(file_id, 2),
        Some(UnknownReason::NoReachingAssignment)
    );

    engine.replace_facts(file_id, FileFacts::default(), ResolveMode::Immediate);
    assert_eq!(engine.query().expression_unknown_reason(range), None);
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
            symbols: vec![SymbolFact::new(
                user.clone(),
                SymbolKind::Class,
                TextRange::new(def_file, 0, 14),
            )],
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
fn resolve_pass_stats_record_cache_cardinality_after_full_resolve() {
    let mut engine = AnalysisEngine::new();
    let def_file = register_project_file(&mut engine, "app/user.rb", "class User; end");
    let first_ref = register_project_file(&mut engine, "app/first.rb", "User");
    let second_ref = register_project_file(&mut engine, "app/second.rb", "User");
    let user = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);

    engine.replace_facts(
        def_file,
        FileFacts {
            symbols: vec![SymbolFact::new(
                user.clone(),
                SymbolKind::Class,
                TextRange::new(def_file, 0, 14),
            )],
            graph_nodes: vec![GraphNodeFact::new(
                user.clone(),
                GraphNodeKind::Class,
                TextRange::new(def_file, 0, 14),
            )],
            ..Default::default()
        },
        ResolveMode::Deferred,
    );
    for file_id in [first_ref, second_ref] {
        engine.replace_facts(
            file_id,
            FileFacts {
                reference_candidates: vec![ReferenceCandidate::constant(
                    TextRange::new(file_id, 0, 4),
                    user.namespace_parts(),
                    Vec::new(),
                )],
                ..Default::default()
            },
            ResolveMode::Deferred,
        );
    }

    engine.resolve();

    let resolve_pass = engine.last_resolve_stats();
    assert_eq!(resolve_pass.constant_cache_misses, 1);
    assert_eq!(resolve_pass.constant_cache_hits, 1);
    assert_eq!(resolve_pass.constant_cache_unique_keys, 1);
    assert_eq!(engine.reference_facts_for(&user).len(), 2);
}

#[test]
fn resolve_files_materializes_only_selected_open_document_candidates() {
    let mut engine = AnalysisEngine::new();
    let first_ref = register_project_file(&mut engine, "app/first.rb", "User.new");
    let second_ref = register_project_file(&mut engine, "app/second.rb", "User.new");
    let def_file = register_project_file(&mut engine, "app/user.rb", "class User; end");
    let user = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);

    for file_id in [first_ref, second_ref] {
        engine.replace_facts(
            file_id,
            FileFacts {
                reference_candidates: vec![ReferenceCandidate::constant(
                    TextRange::new(file_id, 0, 4),
                    user.namespace_parts(),
                    Vec::new(),
                )],
                ..Default::default()
            },
            ResolveMode::Deferred,
        );
    }
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
        ResolveMode::Deferred,
    );

    engine.resolve_files(&[first_ref]);

    assert_eq!(
        AnalysisQuery::new(&engine)
            .references_in_file(first_ref)
            .len(),
        1
    );
    assert!(
        AnalysisQuery::new(&engine)
            .references_in_file(second_ref)
            .is_empty(),
        "closed-file candidates must remain deferred until the complete project resolution"
    );

    engine.resolve();
    assert_eq!(
        AnalysisQuery::new(&engine)
            .references_in_file(second_ref)
            .len(),
        1
    );
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
fn exact_method_reference_prefers_a_verified_declaration_and_falls_back_after_removal() {
    let mut engine = AnalysisEngine::new();
    let signature_file = engine.register_file(SourceFileInput {
        path: PathBuf::from("signatures/java/list.rb"),
        content: "def get(index); end\ndef get(key); end".to_string(),
        kind: SourceKind::Signature,
    });
    let implementation_file = engine.register_file(SourceFileInput {
        path: PathBuf::from("/external/java/util/List.java"),
        content: "Object get(int index) { return values[index]; }\nString get(String key) { return key; }"
            .to_string(),
        kind: SourceKind::External,
    });
    let source_file = register_project_file(
        &mut engine,
        "app/use_list.rb",
        "list.java_send(:get, [Java::int], 0)",
    );
    let owner = FullyQualifiedName::namespace(
        ["Java", "JavaUtil", "List"]
            .into_iter()
            .map(|part| RubyConstant::new(part).unwrap())
            .collect::<Vec<_>>(),
    );
    let method = RubyMethod::new("get").unwrap();
    let method_fqn = FullyQualifiedName::method(owner.namespace_parts(), method);
    let signature_range = TextRange::new(signature_file, 0, 19);
    let int_range = TextRange::new(implementation_file, 0, 47);
    let string_range = TextRange::new(implementation_file, 48, 87);
    engine.replace_facts(
        signature_file,
        FileFacts {
            graph_nodes: vec![GraphNodeFact::new(
                owner.clone(),
                GraphNodeKind::Class,
                signature_range,
            )],
            methods: vec![MethodFact::new(
                method_fqn.clone(),
                FullyQualifiedName::namespace_with_kind(
                    owner.namespace_parts(),
                    NamespaceKind::Instance,
                ),
                signature_range,
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    engine.replace_facts(
        implementation_file,
        FileFacts {
            graph_nodes: vec![GraphNodeFact::new(
                owner.clone(),
                GraphNodeKind::Class,
                TextRange::new(implementation_file, 0, 87),
            )],
            methods: vec![
                MethodFact::new(
                    method_fqn.clone(),
                    FullyQualifiedName::namespace_with_kind(
                        owner.namespace_parts(),
                        NamespaceKind::Instance,
                    ),
                    int_range,
                ),
                MethodFact::new(
                    method_fqn,
                    FullyQualifiedName::namespace_with_kind(
                        owner.namespace_parts(),
                        NamespaceKind::Instance,
                    ),
                    string_range,
                ),
            ],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    let reference_range = TextRange::new(source_file, 16, 20);
    engine.replace_facts(
        source_file,
        FileFacts {
            reference_candidates: vec![ReferenceCandidate::method(
                reference_range,
                crate::core::MethodReferenceCandidate {
                    owner: owner.namespace_parts(),
                    owner_kind: NamespaceKind::Instance,
                    method,
                    is_super: false,
                    access: crate::core::MethodReferenceAccess::VisibilityBypass,
                    caller: None,
                    call_expression_range: None,
                    preferred_definition_range: Some(int_range),
                    diagnostics: crate::core::MethodReferenceDiagnostics {
                        diagnostic_range: reference_range,
                        receiver_label: Some(owner.to_string()),
                        diagnose_unresolved: false,
                        allow_unindexed_owner: false,
                        signature: crate::core::MethodCallSignatureCandidate::default(),
                    },
                },
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert_eq!(
        AnalysisQuery::new(&engine).resolved_reference_definition_ranges_at(source_file, 17),
        vec![int_range],
        "a verified JVM overload range must outrank same-named methods and signatures"
    );

    engine.replace_facts(
        implementation_file,
        FileFacts::default(),
        ResolveMode::Immediate,
    );
    assert_eq!(
        AnalysisQuery::new(&engine).resolved_reference_definition_ranges_at(source_file, 17),
        vec![signature_range],
        "a removed preferred declaration must not leave a stale location and must fall back normally"
    );
}

#[test]
fn runtime_constant_alias_definition_prefers_the_external_proxy_declaration() {
    let mut engine = AnalysisEngine::new();
    let import_file = register_project_file(
        &mut engine,
        "lib/runtime.rb",
        "java_import java.util.concurrent.TimeUnit",
    );
    let implementation_file = engine.register_file(SourceFileInput {
        path: PathBuf::from("/external/java/util/concurrent/TimeUnit.java"),
        content: "public enum TimeUnit {}".to_string(),
        kind: SourceKind::External,
    });
    let alias = FullyQualifiedName::constant(vec![RubyConstant::new("TimeUnit").unwrap()]);
    let proxy = FullyQualifiedName::constant(
        ["Java", "JavaUtilConcurrent", "TimeUnit"]
            .into_iter()
            .map(|part| RubyConstant::new(part).unwrap())
            .collect::<Vec<_>>(),
    );
    let import_range = TextRange::new(import_file, 12, 41);
    let implementation_range = TextRange::new(implementation_file, 0, 23);
    engine.replace_facts(
        import_file,
        FileFacts {
            symbols: vec![SymbolFact::new(
                alias.clone(),
                SymbolKind::Constant,
                import_range,
            )],
            types: vec![crate::core::TypeFact::new(
                TypeSubject::Constant(alias),
                RubyType::ClassReference(proxy.clone()),
                import_range,
                TypeProvenance::Runtime,
            )],
            ..FileFacts::default()
        },
        ResolveMode::Immediate,
    );
    engine.replace_facts(
        implementation_file,
        FileFacts {
            symbols: vec![SymbolFact::new(
                proxy,
                SymbolKind::Class,
                implementation_range,
            )],
            ..FileFacts::default()
        },
        ResolveMode::Immediate,
    );

    assert_eq!(
        AnalysisQuery::new(&engine)
            .constant_definition_ranges(&[RubyConstant::new("TimeUnit").unwrap()], &[]),
        vec![implementation_range],
        "a runtime import alias must navigate to the implementation class instead of its import statement"
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
                    call_expression_range: None,
                    preferred_definition_range: None,
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
    assert_eq!(
        AnalysisQuery::new(&engine).resolved_reference_definition_ranges_at(ref_file, 6),
        vec![TextRange::new(def_file, 12, 20)],
        "ordinary diagnostics-bearing method candidates must navigate through their resolved reference fact"
    );
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
fn borrowed_source_registration_preserves_ascii_and_utf16_semantics() {
    let mut engine = AnalysisEngine::new();
    let ascii = String::from("class User\nend\n");
    let unicode = String::from("a😀b\n");

    let ascii_id = engine.register_file_borrowed("ascii.rb".into(), &ascii, SourceKind::Project);
    let unicode_id =
        engine.register_file_borrowed("unicode.rb".into(), &unicode, SourceKind::Project);
    drop(ascii);
    drop(unicode);

    assert!(engine.file_content_matches(ascii_id, "class User\nend\n"));
    assert!(engine.file_content_matches(unicode_id, "a😀b\n"));
    assert_eq!(
        engine
            .file(unicode_id)
            .expect("borrowed Unicode source should remain registered")
            .byte_offset_to_line_character(5),
        Some((0, 3))
    );
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
        engine.query().method_return_type_for_callee(&callees[0]),
        Some(RubyType::string()),
        "an already-resolved callee must expose its return type without another receiver lookup"
    );
    assert_eq!(
        engine
            .query()
            .constant_definition_ranges(&[RubyConstant::new("Widget").unwrap()], &[],),
        vec![TextRange::new(implementation_file, 0, 37)]
    );
}

#[test]
fn reopened_method_return_requires_every_definition_to_resolve() {
    let mut engine = AnalysisEngine::new();
    let known_file = register_project_file(
        &mut engine,
        "lib/known.rb",
        "class Service\n  def value = 'known'\nend\n",
    );
    let unresolved_file = register_project_file(
        &mut engine,
        "lib/unresolved.rb",
        "class Service\n  def value = dynamic_value\nend\n",
    );
    let owner = FullyQualifiedName::namespace(vec![RubyConstant::new("Service").unwrap()]);
    let method_name = RubyMethod::new("value").unwrap();
    let method = FullyQualifiedName::method(owner.namespace_parts(), method_name);
    let known_range = TextRange::new(known_file, 16, 35);
    let unresolved_range = TextRange::new(unresolved_file, 16, 42);

    engine.replace_facts(
        known_file,
        FileFacts {
            graph_nodes: vec![GraphNodeFact::new(
                owner.clone(),
                GraphNodeKind::Class,
                TextRange::new(known_file, 0, 40),
            )],
            methods: vec![MethodFact::new(method.clone(), owner.clone(), known_range)],
            types: vec![TypeFact::new(
                TypeSubject::MethodReturn(method.clone()),
                RubyType::string(),
                known_range,
                TypeProvenance::Inferred,
            )],
            ..Default::default()
        },
        ResolveMode::Deferred,
    );
    engine.replace_facts(
        unresolved_file,
        FileFacts {
            graph_nodes: vec![GraphNodeFact::new(
                owner.clone(),
                GraphNodeKind::Class,
                TextRange::new(unresolved_file, 0, 47),
            )],
            methods: vec![MethodFact::new(method, owner.clone(), unresolved_range)],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    let callees = engine
        .query()
        .resolve_method_callees(&owner, &method_name)
        .expect("reopened Service#value must resolve");
    assert_eq!(callees.len(), 1);
    assert_eq!(
        engine
            .query()
            .method_return_type_for_receiver(&owner, &method_name),
        None,
        "INVARIANT VIOLATED: receiver return inference discarded an unresolved reopened method \
         definition. This is a bug because every definition is a reachable static outcome. Fix: \
         return Unknown/None unless every matching definition proves a return type."
    );
    assert_eq!(
        engine.query().method_return_type_for_callee(&callees[0]),
        None,
        "INVARIANT VIOLATED: resolved-callee return inference discarded an unresolved reopened \
         method definition. This is a bug because chained calls would consume a partial concrete \
         type. Fix: require a return type for every resolved definition range."
    );
}

#[test]
fn cached_method_return_queries_invalidate_after_semantic_replacement() {
    let mut engine = AnalysisEngine::new();
    let file_id = register_project_file(
        &mut engine,
        "lib/widget.rb",
        "class Widget\n  def value = 'first'\nend\n",
    );
    let owner = FullyQualifiedName::namespace(vec![RubyConstant::new("Widget").unwrap()]);
    let method_name = RubyMethod::new("value").unwrap();
    let method = FullyQualifiedName::method(owner.namespace_parts(), method_name);
    let range = TextRange::new(file_id, 15, 34);
    let facts = |ruby_type| FileFacts {
        graph_nodes: vec![GraphNodeFact::new(
            owner.clone(),
            GraphNodeKind::Class,
            TextRange::new(file_id, 0, 41),
        )],
        methods: vec![MethodFact::new(method.clone(), owner.clone(), range)],
        types: vec![TypeFact::new(
            TypeSubject::MethodReturn(method.clone()),
            ruby_type,
            range,
            TypeProvenance::Inferred,
        )],
        ..Default::default()
    };
    engine.replace_facts(file_id, facts(RubyType::string()), ResolveMode::Immediate);
    let cache = AnalysisQueryCache::default();

    assert_eq!(
        engine
            .query()
            .method_return_type_for_receiver_cached(&owner, &method_name, &cache),
        Some(RubyType::string())
    );
    let cached_callees = engine
        .query()
        .resolve_method_callees_cached(&owner, &method_name, &cache)
        .expect("cached method owner must resolve");
    assert_eq!(cached_callees.len(), 1);
    assert_eq!(
        engine
            .query()
            .method_return_type_for_callee(&cached_callees[0]),
        Some(RubyType::string())
    );
    assert_eq!(cache.valid_entry_counts_for_test(), (1, 1));

    engine.replace_facts(file_id, facts(RubyType::integer()), ResolveMode::Immediate);
    assert_eq!(
        engine
            .query()
            .method_return_type_for_receiver_cached(&owner, &method_name, &cache),
        Some(RubyType::integer()),
        "a semantic replacement must invalidate cached return types"
    );
    let cached_callees = engine
        .query()
        .resolve_method_callees_cached(&owner, &method_name, &cache)
        .expect("replacement method owner must resolve");
    assert_eq!(
        engine
            .query()
            .method_return_type_for_callee(&cached_callees[0]),
        Some(RubyType::integer()),
        "semantic replacement must expose the resolved callee's new return type"
    );
    assert_eq!(
        cache.valid_entry_counts_for_test(),
        (1, 1),
        "replacement must discard both cache families before inserting new-revision entries"
    );
}

#[test]
fn resolved_method_callee_cache_is_bounded_per_source_collection() {
    let mut engine = AnalysisEngine::new();
    let file_id = register_project_file(&mut engine, "lib/widget.rb", "class Widget; end\n");
    let owner = FullyQualifiedName::namespace(vec![RubyConstant::new("Widget").unwrap()]);
    engine.replace_facts(
        file_id,
        FileFacts {
            graph_nodes: vec![GraphNodeFact::new(
                owner.clone(),
                GraphNodeKind::Class,
                TextRange::new(file_id, 0, 17),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    let cache = AnalysisQueryCache::default();

    for index in 0..300 {
        let method = RubyMethod::new(&format!("missing_{index}")).unwrap();
        assert!(engine
            .query()
            .resolve_method_callees_cached(&owner, &method, &cache)
            .is_some());
    }

    assert_eq!(
        cache.valid_entry_counts_for_test(),
        (0, 64),
        "one pathological source must not retain an unbounded method-callee catalog"
    );
}

#[test]
fn method_lookup_chain_cache_is_engine_local_and_invalidates_on_replacement() {
    let mut engine = AnalysisEngine::new();
    let file_id = register_project_file(
        &mut engine,
        "lib/child.rb",
        "class Parent\n  def value = 'ok'\nend\nclass Child < Parent\nend\n",
    );
    let parent = FullyQualifiedName::namespace(vec![RubyConstant::new("Parent").unwrap()]);
    let child = FullyQualifiedName::namespace(vec![RubyConstant::new("Child").unwrap()]);
    let method = RubyMethod::new("value").unwrap();
    engine.replace_facts(
        file_id,
        FileFacts {
            graph_nodes: vec![
                GraphNodeFact::new(
                    parent.clone(),
                    GraphNodeKind::Module,
                    crate::core::TextRange::new(file_id, 0, 38),
                ),
                GraphNodeFact::new(
                    child.clone(),
                    GraphNodeKind::Module,
                    crate::core::TextRange::new(file_id, 39, 63),
                ),
            ],
            graph_edges: vec![GraphEdgeFact::new(
                child.clone(),
                parent.clone(),
                GraphEdgeKind::Include,
                crate::core::TextRange::new(file_id, 39, 59),
            )],
            methods: vec![MethodFact::new(
                FullyQualifiedName::method(parent.namespace_parts(), method),
                parent.clone(),
                TextRange::new(file_id, 15, 31),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert!(engine
        .query()
        .resolve_method_callees(&child, &method)
        .is_some());
    assert_eq!(
        engine.valid_method_lookup_chain_cache_len_for_test(),
        1,
        "repeated method names on one receiver must reuse one MRO"
    );
    assert!(engine
        .query()
        .resolve_method_callees(&child, &RubyMethod::new("missing").unwrap())
        .is_some());
    assert_eq!(engine.valid_method_lookup_chain_cache_len_for_test(), 1);

    engine.replace_facts(file_id, FileFacts::default(), ResolveMode::Immediate);
    assert_eq!(
        engine.valid_method_lookup_chain_cache_len_for_test(),
        0,
        "semantic replacement must invalidate cached lookup chains"
    );
}

#[test]
fn method_reference_chain_cache_returns_the_stored_chain_by_borrow() {
    let mut engine = AnalysisEngine::new();
    let file_id = register_project_file(
        &mut engine,
        "lib/child.rb",
        "class Parent\n  def value = 'ok'\nend\nclass Child < Parent\nend\n",
    );
    let parent = FullyQualifiedName::namespace(vec![RubyConstant::new("Parent").unwrap()]);
    let child = FullyQualifiedName::namespace(vec![RubyConstant::new("Child").unwrap()]);
    engine.replace_facts(
        file_id,
        FileFacts {
            graph_nodes: vec![
                GraphNodeFact::new(
                    parent.clone(),
                    GraphNodeKind::Class,
                    TextRange::new(file_id, 0, 38),
                ),
                GraphNodeFact::new(
                    child.clone(),
                    GraphNodeKind::Class,
                    TextRange::new(file_id, 39, 63),
                ),
            ],
            graph_edges: vec![GraphEdgeFact::new(
                child.clone(),
                parent,
                GraphEdgeKind::Superclass,
                TextRange::new(file_id, 39, 59),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    let mut cache = MethodLookupChainCache::new();
    let (first_pointer, first_length) = {
        let chain = method_lookup_chain_for_reference_cached(&engine, &child, &mut cache);
        (chain.as_ptr(), chain.len())
    };
    let (second_pointer, second_length) = {
        let chain = method_lookup_chain_for_reference_cached(&engine, &child, &mut cache);
        (chain.as_ptr(), chain.len())
    };

    assert_eq!(first_length, second_length);
    assert_eq!(first_pointer, second_pointer);
    assert_eq!(cache.len(), 1);
}

#[test]
fn method_reference_chain_cache_reuses_interned_owner_ids() {
    let mut engine = AnalysisEngine::new();
    let file_id = register_project_file(
        &mut engine,
        "lib/child.rb",
        "class Parent\n  def first = 1\n  def second = 2\nend\nclass Child < Parent\nend\n",
    );
    let parent = FullyQualifiedName::namespace(vec![RubyConstant::new("Parent").unwrap()]);
    let child = FullyQualifiedName::namespace(vec![RubyConstant::new("Child").unwrap()]);
    let first = RubyMethod::new("first").unwrap();
    let second = RubyMethod::new("second").unwrap();
    engine.replace_facts(
        file_id,
        FileFacts {
            graph_nodes: vec![
                GraphNodeFact::new(
                    parent.clone(),
                    GraphNodeKind::Class,
                    TextRange::new(file_id, 0, 52),
                ),
                GraphNodeFact::new(
                    child.clone(),
                    GraphNodeKind::Class,
                    TextRange::new(file_id, 53, 77),
                ),
            ],
            graph_edges: vec![GraphEdgeFact::new(
                child.clone(),
                parent.clone(),
                GraphEdgeKind::Superclass,
                TextRange::new(file_id, 53, 73),
            )],
            methods: vec![
                MethodFact::new(
                    FullyQualifiedName::method(parent.namespace_parts(), first),
                    parent.clone(),
                    TextRange::new(file_id, 15, 28),
                ),
                MethodFact::new(
                    FullyQualifiedName::method(parent.namespace_parts(), second),
                    parent,
                    TextRange::new(file_id, 31, 45),
                ),
            ],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    let mut cache = MethodLookupChainCache::new();
    assert!(matches!(
        engine
            .query()
            .resolve_method_reference_with_chain_cache(&child, &first, &mut cache),
        crate::engine::resolution::MethodLookupResult::Unique(_)
    ));

    engine.names.reset_fqn_lookup_count_for_test();
    assert!(matches!(
        engine
            .query()
            .resolve_method_reference_with_chain_cache(&child, &second, &mut cache),
        crate::engine::resolution::MethodLookupResult::Unique(_)
    ));
    assert_eq!(
        engine.names.fqn_lookup_count_for_test(),
        1,
        "a cached method chain must reuse each member's interned owner id; only the namespace existence check should probe the interner"
    );
}

#[test]
fn edge_only_graph_entries_do_not_promote_missing_namespaces() {
    let mut engine = AnalysisEngine::new();
    let file_id = register_project_file(&mut engine, "lib/edge.rb", "class Parent\nend\n");
    let parent = FullyQualifiedName::namespace(vec![RubyConstant::new("Parent").unwrap()]);
    let missing = FullyQualifiedName::namespace(vec![RubyConstant::new("Missing").unwrap()]);
    let root = FullyQualifiedName::namespace_with_kind(Vec::new(), NamespaceKind::Instance);
    engine.replace_facts(
        file_id,
        FileFacts {
            graph_nodes: vec![GraphNodeFact::new(
                parent.clone(),
                GraphNodeKind::Class,
                TextRange::new(file_id, 0, 16),
            )],
            graph_edges: vec![GraphEdgeFact::new(
                missing.clone(),
                parent,
                GraphEdgeKind::Superclass,
                TextRange::new(file_id, 0, 16),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert_eq!(method_lookup_chain(&engine, &missing), vec![missing, root]);
}

#[test]
fn generated_owners_use_normal_mro_but_isolate_siblings_and_replace_per_file() {
    let mut engine = AnalysisEngine::new();
    let file_id = register_project_file(
        &mut engine,
        "spec/user_spec.rb",
        "RSpec.describe User do; context 'nested' do; end; end",
    );
    let generated = |local_identity: &str| {
        FullyQualifiedName::namespace(vec![RubyConstant::generated_owner(
            GeneratedOwnerId::new(
                "rspec-ruby",
                "file:///workspace/spec/user_spec.rb",
                local_identity,
            )
            .expect("test generated owner identity must be valid"),
        )])
    };
    let parent = generated("group:0:0");
    let child = generated("group:0:24");
    let sibling = generated("group:1:0");
    let helper = RubyMethod::new("helper").expect("test method must be valid");
    let helper_fqn = FullyQualifiedName::method(parent.namespace_parts(), helper);
    let helper_range = TextRange::new(file_id, 1, 7);
    engine.replace_facts(
        file_id,
        FileFacts {
            symbols: vec![SymbolFact::new(
                parent.clone(),
                SymbolKind::Class,
                TextRange::new(file_id, 0, 10),
            )],
            graph_nodes: vec![
                GraphNodeFact::new(
                    parent.clone(),
                    GraphNodeKind::Class,
                    TextRange::new(file_id, 0, 10),
                ),
                GraphNodeFact::new(
                    child.clone(),
                    GraphNodeKind::Class,
                    TextRange::new(file_id, 20, 30),
                ),
                GraphNodeFact::new(
                    sibling.clone(),
                    GraphNodeKind::Class,
                    TextRange::new(file_id, 31, 40),
                ),
            ],
            graph_edges: vec![GraphEdgeFact::new(
                child.clone(),
                parent.clone(),
                GraphEdgeKind::Superclass,
                TextRange::new(file_id, 20, 30),
            )],
            methods: vec![MethodFact::new(helper_fqn, parent.clone(), helper_range)],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    let query = engine.query();
    assert_eq!(
        query
            .resolve_method_callees(&parent, &helper)
            .expect("parent helper must resolve")[0]
            .definition_ranges,
        vec![helper_range]
    );
    assert_eq!(
        query
            .resolve_method_callees(&child, &helper)
            .expect("nested generated owner must inherit its parent helper")[0]
            .definition_ranges,
        vec![helper_range]
    );
    let sibling_callees = query
        .resolve_method_callees(&sibling, &helper)
        .expect("known sibling owner must produce a conservative receiver-only result");
    assert_eq!(sibling_callees.len(), 1);
    assert_eq!(sibling_callees[0].owner, sibling);
    assert_eq!(
        sibling_callees[0].resolution,
        MethodCalleeResolution::ReceiverOnly
    );
    assert!(sibling_callees[0].definition_ranges.is_empty());
    assert!(query
        .constant_matches(&ConstantLookupRequest::new("", 100))
        .is_empty());
    assert!(query
        .constant_rename_target(&parent.namespace_parts(), &[])
        .is_none());

    engine.replace_facts(file_id, FileFacts::default(), ResolveMode::Immediate);
    assert!(engine
        .query()
        .resolve_method_callees(&parent, &helper)
        .is_none());
    assert!(engine
        .query()
        .resolve_method_callees(&child, &helper)
        .is_none());
}

#[test]
fn execution_context_applications_resolve_independently_and_replace_per_file() {
    let mut engine = AnalysisEngine::new();
    let template_file = register_project_file(
        &mut engine,
        "spec/support/shared_examples.rb",
        "shared_helper\nconsumer_helper",
    );
    let applications_file = register_project_file(
        &mut engine,
        "spec/shared_examples_spec.rb",
        "consumer_helper\nconsumer_helper",
    );
    let namespace = |name: &str| {
        FullyQualifiedName::namespace(vec![
            RubyConstant::new(name).expect("test execution owner name must be valid")
        ])
    };
    let template = namespace("SharedTemplate");
    let first = namespace("FirstApplication");
    let second = namespace("SecondApplication");
    let shared = RubyMethod::new("shared_helper").expect("test method must be valid");
    let consumer = RubyMethod::new("consumer_helper").expect("test method must be valid");
    let first_only = RubyMethod::new("first_only").expect("test method must be valid");
    let second_only = RubyMethod::new("second_only").expect("test method must be valid");
    let shared_range = TextRange::new(template_file, 0, 13);
    let first_range = TextRange::new(applications_file, 0, 15);
    let second_range = TextRange::new(applications_file, 16, 31);
    engine.replace_facts(
        template_file,
        FileFacts {
            graph_nodes: vec![GraphNodeFact::new(
                template.clone(),
                GraphNodeKind::Class,
                shared_range,
            )],
            methods: vec![MethodFact::new(
                FullyQualifiedName::method(template.namespace_parts(), shared),
                template.clone(),
                shared_range,
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    let application_facts = |include_second: bool| {
        let mut nodes = vec![GraphNodeFact::new(
            first.clone(),
            GraphNodeKind::Class,
            first_range,
        )];
        let mut edges = vec![GraphEdgeFact::new(
            template.clone(),
            first.clone(),
            GraphEdgeKind::ExecutionContextApplication,
            first_range,
        )];
        let first_consumer_fqn = FullyQualifiedName::method(first.namespace_parts(), consumer);
        let mut methods = vec![
            MethodFact::new(first_consumer_fqn.clone(), first.clone(), first_range),
            MethodFact::new(
                FullyQualifiedName::method(first.namespace_parts(), first_only),
                first.clone(),
                first_range,
            ),
        ];
        let mut types = vec![TypeFact::new(
            TypeSubject::MethodReturn(first_consumer_fqn),
            RubyType::string(),
            first_range,
            TypeProvenance::Extension,
        )];
        if include_second {
            nodes.push(GraphNodeFact::new(
                second.clone(),
                GraphNodeKind::Class,
                second_range,
            ));
            edges.push(GraphEdgeFact::new(
                template.clone(),
                second.clone(),
                GraphEdgeKind::ExecutionContextApplication,
                second_range,
            ));
            let second_consumer_fqn =
                FullyQualifiedName::method(second.namespace_parts(), consumer);
            methods.extend([
                MethodFact::new(second_consumer_fqn.clone(), second.clone(), second_range),
                MethodFact::new(
                    FullyQualifiedName::method(second.namespace_parts(), second_only),
                    second.clone(),
                    second_range,
                ),
            ]);
            types.push(TypeFact::new(
                TypeSubject::MethodReturn(second_consumer_fqn),
                RubyType::integer(),
                second_range,
                TypeProvenance::Extension,
            ));
        }
        FileFacts {
            graph_nodes: nodes,
            graph_edges: edges,
            methods,
            types,
            ..Default::default()
        }
    };
    engine.replace_facts(
        applications_file,
        application_facts(true),
        ResolveMode::Immediate,
    );

    let query = engine.query();
    let shared_callees = query
        .resolve_method_callees(&template, &shared)
        .expect("template-local helper must resolve");
    assert_eq!(shared_callees.len(), 1);
    assert_eq!(shared_callees[0].definition_ranges, vec![shared_range]);
    let application_callees = query
        .resolve_method_callees(&template, &consumer)
        .expect("application helpers must resolve through the template");
    assert_eq!(application_callees.len(), 2);
    assert_eq!(application_callees[0].definition_ranges, vec![first_range]);
    assert_eq!(application_callees[1].definition_ranges, vec![second_range]);
    assert_eq!(
        query.method_return_type_for_receiver(&template, &consumer),
        Some(RubyType::union(vec![
            RubyType::integer(),
            RubyType::string()
        ])),
    );
    let completion_names = query
        .method_facts_matching(&template, "")
        .into_iter()
        .map(|fact| fact.fqn.name())
        .collect::<Vec<_>>();
    assert!(completion_names.contains(&"shared_helper".to_string()));
    assert!(completion_names.contains(&"consumer_helper".to_string()));
    assert!(completion_names.contains(&"first_only".to_string()));
    assert!(completion_names.contains(&"second_only".to_string()));
    assert!(matches!(
        query.resolve_method_reference(&template, &consumer),
        crate::MethodLookupResult::Ambiguous { .. }
    ));
    drop(query);

    engine.replace_facts(
        applications_file,
        application_facts(false),
        ResolveMode::Immediate,
    );
    let one = engine
        .query()
        .resolve_method_callees(&template, &consumer)
        .expect("remaining application helper must resolve");
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].definition_ranges, vec![first_range]);

    engine.replace_facts(
        applications_file,
        FileFacts::default(),
        ResolveMode::Immediate,
    );
    let removed = engine
        .query()
        .resolve_method_callees(&template, &consumer)
        .expect("known template must retain receiver-only fallback");
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].resolution, MethodCalleeResolution::ReceiverOnly);
    assert!(removed[0].definition_ranges.is_empty());
}

#[test]
fn execution_context_query_selects_innermost_range_and_replaces_per_file() {
    use crate::core::{ExecutionContextFact, ExecutionScopeMode};

    let mut engine = AnalysisEngine::new();
    let file_id = register_project_file(
        &mut engine,
        "spec/nested_spec.rb",
        "describe do\n  context do\n    helper\n  end\nend\n",
    );
    let owner = |local: &str| {
        FullyQualifiedName::namespace(vec![RubyConstant::generated_owner(
            GeneratedOwnerId::new("rspec-ruby", "file:///workspace/spec/nested_spec.rb", local)
                .unwrap(),
        )])
    };
    let outer = ExecutionContextFact {
        range: TextRange::new(file_id, 9, 45),
        lexical_namespace: FullyQualifiedName::namespace(Vec::new()),
        implicit_receiver: owner("outer"),
        method_definition_owner: owner("outer"),
        lexical_scope: ExecutionScopeMode::Preserve,
        local_scope: ExecutionScopeMode::Preserve,
        extension_id: "rspec-ruby".to_string(),
    };
    let inner = ExecutionContextFact {
        range: TextRange::new(file_id, 22, 39),
        lexical_namespace: FullyQualifiedName::namespace(Vec::new()),
        implicit_receiver: owner("inner"),
        method_definition_owner: owner("inner"),
        lexical_scope: ExecutionScopeMode::Preserve,
        local_scope: ExecutionScopeMode::Preserve,
        extension_id: "rspec-ruby".to_string(),
    };
    engine.replace_facts(
        file_id,
        FileFacts {
            execution_contexts: vec![outer.clone(), inner.clone()],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert_eq!(
        engine.query().execution_context_at(file_id, 30),
        Some(&inner)
    );
    assert_eq!(
        engine.query().execution_context_at(file_id, 12),
        Some(&outer)
    );
    assert_eq!(engine.query().execution_context_at(file_id, 5), None);

    engine.replace_facts(file_id, FileFacts::default(), ResolveMode::Immediate);
    assert_eq!(engine.query().execution_context_at(file_id, 30), None);
}

#[test]
fn inference_telemetry_replaces_with_its_owning_file() {
    let mut engine = AnalysisEngine::new();
    let file_id = register_project_file(&mut engine, "types.rb", "class Types; end\n");
    let method = FullyQualifiedName::method(
        vec![RubyConstant::new("Types").unwrap()],
        RubyMethod::new("value").unwrap(),
    );
    let unknown = TypeInferenceOutcome::unknown(UnknownReason::UnprovenRecursiveCycle);
    let mut recursive = InferenceTelemetry::default();
    recursive.observe_method_return(&unknown);
    recursive.recursive_components = 1;
    recursive.recursive_methods = 1;
    recursive.solver_iterations = 1;

    engine.replace_facts(
        file_id,
        FileFacts {
            inference: InferenceEvidence {
                method_return_outcomes: [(method.clone(), unknown)].into_iter().collect(),
                method_return_equations: vec![MethodReturnEquation::from_ruby_type(
                    method.clone(),
                    RubyType::Unknown,
                    UnknownReason::UnprovenRecursiveCycle,
                )],
                telemetry: recursive,
                ..Default::default()
            },
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    assert_eq!(engine.inference_telemetry().unknown_method_returns, 1);
    assert_eq!(
        engine
            .method_return_outcomes_in_file(file_id)
            .and_then(|outcomes| outcomes.get(&method))
            .and_then(TypeInferenceOutcome::unknown_reason),
        Some(UnknownReason::UnprovenRecursiveCycle)
    );

    let concrete = TypeInferenceOutcome::proven(RubyType::string());
    let mut proven = InferenceTelemetry::default();
    proven.observe_method_return(&concrete);
    engine.replace_facts(
        file_id,
        FileFacts {
            inference: InferenceEvidence {
                method_return_outcomes: [(method.clone(), concrete)].into_iter().collect(),
                method_return_equations: vec![MethodReturnEquation::proven(
                    method.clone(),
                    RubyType::string(),
                )],
                telemetry: proven,
                ..Default::default()
            },
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    let current = engine.inference_telemetry();
    assert_eq!(current.method_return_outcomes, 1);
    assert_eq!(current.proven_method_returns, 1);
    assert_eq!(current.unknown_method_returns, 0);
    assert!(current.unknown_reasons.is_empty());
    assert_eq!(current.recursive_components, 0);
    assert_eq!(
        engine
            .method_return_outcomes_in_file(file_id)
            .and_then(|outcomes| outcomes.get(&method))
            .and_then(TypeInferenceOutcome::proven_type),
        Some(&RubyType::string())
    );
}
