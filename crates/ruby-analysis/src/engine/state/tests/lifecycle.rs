//! Source registration, revision checks, and file-owned fact replacement.

use super::*;

#[test]
fn name_registry_interns_one_owned_identity_with_stable_ids() {
    let mut names = Names::default();
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

#[test]
fn file_ids_are_stable_across_updates() {
    let mut engine = Project::new();

    let first = register_project_file(&mut engine, "app/user.rb", "A = 1");
    let second = register_project_file(&mut engine, "app/user.rb", "A = 2");

    assert_eq!(first, second);
    assert_eq!(engine.view().file_count(), 1);
    let file = engine.view().file(first).unwrap();
    assert_eq!(file.line_index.len(), "A = 2".len());
    assert!(file.source_text().is_none());
}

#[test]
fn source_revisions_change_only_for_distinct_registered_snapshots() {
    let mut engine = Project::new();
    let path = std::path::PathBuf::from("app/utility.rb");
    let first = register_project_file(&mut engine, path.clone(), "module Utility; end");
    let first_snapshot = engine.view().source_snapshot_for_path(&path).unwrap();

    let identical = register_project_file(&mut engine, path.clone(), "module Utility; end");
    assert_eq!(identical, first);
    assert_eq!(
        engine.view().source_snapshot_for_path(&path),
        Some(first_snapshot),
        "byte-identical registration must retain the source snapshot identity"
    );

    register_project_file(
        &mut engine,
        path.clone(),
        "module Utility; def self.lookup; end; end",
    );
    assert_ne!(
        engine.view().source_snapshot_for_path(&path),
        Some(first_snapshot),
        "a content change must create a new source snapshot identity"
    );
}

#[test]
fn stale_source_revision_cannot_replace_newer_file_facts() {
    let mut engine = Project::new();
    let path = std::path::PathBuf::from("app/utility.rb");
    let file_id = register_project_file(&mut engine, path.clone(), "module Utility; end");
    let stale_snapshot = engine.view().source_snapshot_for_path(&path).unwrap();
    register_project_file(
        &mut engine,
        path,
        "module Utility; def self.lookup; end; end",
    );

    let utility = FullyQualifiedName::namespace(vec![RubyConstant::new("Utility").unwrap()]);
    engine.update(
        file_id,
        FileAnalysis {
            graph_nodes: vec![GraphNodeFact::new(
                utility.clone(),
                GraphNodeKind::Module,
                crate::core::TextRange::new(file_id, 0, 14),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert_eq!(
        engine.update_if_snapshot(
            stale_snapshot,
            FileAnalysis::default(),
            ResolveMode::Immediate,
        ),
        None,
        "facts collected from a superseded source snapshot must be discarded"
    );
    assert!(engine.view().namespace_exists(&utility));
}

#[test]
fn source_kind_updates_with_file() {
    let mut engine = Project::new();

    let file_id = engine.register_file(SourceFileInput {
        path: "gems/foo.rb".into(),
        content: "module Foo; end".into(),
        kind: SourceKind::Gem,
    });

    assert_eq!(engine.view().file(file_id).unwrap().kind, SourceKind::Gem);
}

#[test]
fn namespace_existence_tracks_graph_node_replacement() {
    let mut engine = Project::new();
    let file_id = register_project_file(&mut engine, "app/user.rb", "class User; end");
    let user = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);

    engine.update(
        file_id,
        FileAnalysis {
            graph_nodes: vec![GraphNodeFact::new(
                user.clone(),
                GraphNodeKind::Class,
                crate::core::TextRange::new(file_id, 0, 15),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert!(engine.view().namespace_exists(&user));
    assert!(!engine
        .view()
        .namespace_exists(&FullyQualifiedName::namespace(vec![RubyConstant::new(
            "Missing"
        )
        .unwrap()])));

    engine.update(file_id, FileAnalysis::default(), ResolveMode::Immediate);
    assert!(!engine.view().namespace_exists(&user));
}

#[test]
fn replace_facts_removes_stale_type_facts() {
    let mut engine = Project::new();
    let file_id = register_project_file(&mut engine, "app/user.rb", "A = 1");
    let subject = constant_subject("A");

    engine.update(
        file_id,
        FileAnalysis {
            types: vec![TypeFact::new(
                subject.clone(),
                RubyType::integer(),
                engine.view().text_range(file_id, 0, 5),
                TypeProvenance::Assignment,
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    engine.update(
        file_id,
        FileAnalysis {
            types: vec![TypeFact::new(
                subject.clone(),
                RubyType::string(),
                engine.view().text_range(file_id, 10, 15),
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
    let mut engine = Project::new();
    let file_id = register_project_file(&mut engine, "app/user.rb", "class User; end");
    let fqn = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);

    engine.update(
        file_id,
        FileAnalysis {
            symbols: vec![SymbolFact::new(
                fqn.clone(),
                SymbolKind::Class,
                engine.view().text_range(file_id, 0, 10),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    engine.update(
        file_id,
        FileAnalysis {
            symbols: vec![SymbolFact::new(
                fqn.clone(),
                SymbolKind::Class,
                engine.view().text_range(file_id, 20, 30),
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
#[should_panic(expected = "file analysis references unknown source file id")]
fn rejects_type_fact_for_unknown_file() {
    let mut engine = Project::new();
    let subject = constant_subject("A");

    engine.update(
        SourceFileId(99),
        FileAnalysis {
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
    let mut engine = Project::new();
    let file_id = register_project_file(&mut engine, "unicode.rb", "a😀b\n");
    let file = engine
        .view()
        .file(file_id)
        .expect("registered source should exist");

    assert_eq!(file.byte_offset_to_line_character(1), Some((0, 1)));
    assert_eq!(file.byte_offset_to_line_character(5), Some((0, 3)));
}

#[test]
fn borrowed_source_registration_preserves_ascii_and_utf16_semantics() {
    let mut engine = Project::new();
    let ascii = String::from("class User\nend\n");
    let unicode = String::from("a😀b\n");

    let ascii_id = engine.register_file_borrowed("ascii.rb".into(), &ascii, SourceKind::Project);
    let unicode_id =
        engine.register_file_borrowed("unicode.rb".into(), &unicode, SourceKind::Project);
    drop(ascii);
    drop(unicode);

    assert!(engine
        .view()
        .file_content_matches(ascii_id, "class User\nend\n"));
    assert!(engine.view().file_content_matches(unicode_id, "a😀b\n"));
    assert_eq!(
        engine
            .view()
            .file(unicode_id)
            .expect("borrowed Unicode source should remain registered")
            .byte_offset_to_line_character(5),
        Some((0, 3))
    );
}

#[test]
fn inference_telemetry_replaces_with_its_owning_file() {
    let mut engine = Project::new();
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

    engine.update(
        file_id,
        FileAnalysis {
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
    engine.update(
        file_id,
        FileAnalysis {
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

fn declaration_side_facts(file_id: SourceFileId, index: usize) -> FileAnalysis {
    use crate::core::{
        ExecutionContextFact, ExecutionScopeMode, MethodVisibility, MethodVisibilityOverrideFact,
    };

    let owner = FullyQualifiedName::namespace(vec![RubyConstant::generated_owner(
        GeneratedOwnerId::new(
            "sample-extension",
            &format!("file:///workspace/spec/sample_{index}_spec.rb"),
            "block",
        )
        .unwrap(),
    )]);
    let widget = FullyQualifiedName::namespace(vec![RubyConstant::new("Widget").unwrap()]);
    FileAnalysis {
        method_visibility_overrides: vec![MethodVisibilityOverrideFact::new(
            widget,
            RubyMethod::new("render").unwrap(),
            MethodVisibility::Private,
            TextRange::new(file_id, 0, 6),
        )],
        execution_contexts: vec![ExecutionContextFact {
            range: TextRange::new(file_id, 0, 12),
            lexical_namespace: FullyQualifiedName::namespace(Vec::new()),
            implicit_receiver: owner.clone(),
            method_definition_owner: owner,
            lexical_scope: ExecutionScopeMode::Preserve,
            local_scope: ExecutionScopeMode::Preserve,
            extension_id: "sample-extension".to_string(),
        }],
        ..Default::default()
    }
}

#[test]
fn memory_stats_count_visibility_overrides_and_execution_contexts() {
    let source = "describe do\nend\n";
    let mut with_facts = Project::new();
    let mut without_facts = Project::new();
    let with_id = register_project_file(&mut with_facts, "spec/sample_0_spec.rb", source);
    let without_id = register_project_file(&mut without_facts, "spec/sample_0_spec.rb", source);
    with_facts.update(
        with_id,
        declaration_side_facts(with_id, 0),
        ResolveMode::Immediate,
    );
    without_facts.update(without_id, FileAnalysis::default(), ResolveMode::Immediate);

    assert!(
        with_facts.estimated_memory_stats().total()
            > without_facts.estimated_memory_stats().total(),
        "visibility overrides and execution contexts must count toward the engine heap"
    );
}

#[test]
fn shrink_to_fit_compacts_visibility_overrides_and_execution_contexts() {
    let source = "describe do\nend\n";
    let mut engine = Project::new();
    let file_ids = (0..64)
        .map(|index| {
            let path = format!("spec/sample_{index}_spec.rb");
            let file_id = register_project_file(&mut engine, path, source);
            let facts = declaration_side_facts(file_id, index);
            engine.update(file_id, facts, ResolveMode::Deferred);
            file_id
        })
        .collect::<Vec<_>>();
    for file_id in &file_ids[1..] {
        engine.update(*file_id, FileAnalysis::default(), ResolveMode::Deferred);
    }
    engine.resolve();
    engine.shrink_to_fit();

    let mut fresh = Project::new();
    let fresh_id = register_project_file(&mut fresh, "spec/sample_0_spec.rb", source);
    fresh.update(
        fresh_id,
        declaration_side_facts(fresh_id, 0),
        ResolveMode::Immediate,
    );

    assert!(
        engine.decls.method_visibility_overrides_heap_bytes()
            <= fresh.decls.method_visibility_overrides_heap_bytes(),
        "compaction must release emptied visibility-override capacity"
    );
    assert!(
        engine.decls.execution_contexts_heap_bytes() <= fresh.decls.execution_contexts_heap_bytes(),
        "compaction must release emptied execution-context capacity"
    );
}
