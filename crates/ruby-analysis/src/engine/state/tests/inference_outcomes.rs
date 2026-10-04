//! Expression outcomes, local reads, method returns, and resolve-pass outcome caches.

use super::*;
use crate::engine::ResolveStat;

#[test]
fn type_at_reads_engine_owned_store() {
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

    match engine.view().type_at(&subject, file_id, 4) {
        TypeResolution::Resolved(fact) => assert_eq!(fact.ruby_type, RubyType::integer()),
        other => panic!("expected resolved type fact, got {other:?}"),
    }
}

#[test]
fn expression_query_preserves_an_exact_unknown_proof_barrier() {
    let mut engine = Project::new();
    let file_id = register_project_file(&mut engine, "app/user.rb", "@value");
    let range = engine.view().text_range(file_id, 0, 6);

    engine.update(
        file_id,
        FileAnalysis {
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
        engine.view().expression_type_at(file_id, 2),
        Some(RubyType::Unknown),
        "an exact Unknown expression must stop adapters from borrowing another concrete type"
    );
    assert_eq!(
        engine.view().expression_unknown_reason(range),
        Some(UnknownReason::NoReachingAssignment)
    );
    assert_eq!(
        engine.view().expression_unknown_reason_at(file_id, 2),
        Some(UnknownReason::NoReachingAssignment)
    );

    engine.update(file_id, FileAnalysis::default(), ResolveMode::Immediate);
    assert_eq!(engine.view().expression_unknown_reason(range), None);
}

#[test]
fn compact_expression_unknown_reason_does_not_require_a_type_store_fact() {
    let mut engine = Project::new();
    let file_id = register_project_file(&mut engine, "app/user.rb", "value");
    let range = engine.view().text_range(file_id, 0, 5);

    engine.update(
        file_id,
        FileAnalysis {
            inference: InferenceEvidence {
                expression_unknown_reasons: vec![(range, UnknownReason::UnresolvedAssignmentValue)],
                ..Default::default()
            },
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert_eq!(engine.view().expression_type_at(file_id, 2), None);
    assert_eq!(
        engine.view().expression_unknown_reason_at(file_id, 2),
        Some(UnknownReason::UnresolvedAssignmentValue),
        "compact local-flow evidence must remain queryable without entering the general type store"
    );
    assert_eq!(
        engine.view().expression_unknown_reasons_in_file(file_id),
        Some(&[(range, UnknownReason::UnresolvedAssignmentValue)][..])
    );
}

#[test]
fn compact_local_read_type_is_queryable_and_replaced_without_a_type_store_fact() {
    let mut engine = Project::new();
    let file_id = register_project_file(&mut engine, "app/user.rb", "value");
    let range = engine.view().text_range(file_id, 0, 5);
    let empty_fingerprint = engine.view().semantic_result_fingerprint();

    engine.update(
        file_id,
        FileAnalysis {
            local_read_types: vec![(range, RubyType::string())].into_boxed_slice(),
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert_eq!(
        engine.view().expression_type_at(file_id, 2),
        Some(RubyType::string())
    );
    assert_eq!(
        engine.view().local_read_type_at(file_id, 2),
        Some(RubyType::string())
    );
    assert_eq!(
        engine.view().local_read_types_in_file(file_id),
        Some(vec![(range, RubyType::string())])
    );
    assert_ne!(
        engine.view().semantic_result_fingerprint(),
        empty_fingerprint
    );

    engine.update(file_id, FileAnalysis::default(), ResolveMode::Immediate);
    assert_eq!(engine.view().expression_type_at(file_id, 2), None);
    assert_eq!(
        engine.view().local_read_types_in_file(file_id),
        Some(Vec::new())
    );
}

#[test]
fn resolve_pass_stats_record_cache_cardinality_after_full_resolve() {
    let mut engine = Project::new();
    let def_file = register_project_file(&mut engine, "app/user.rb", "class User; end");
    let first_ref = register_project_file(&mut engine, "app/first.rb", "User");
    let second_ref = register_project_file(&mut engine, "app/second.rb", "User");
    let user = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);

    engine.update(
        def_file,
        FileAnalysis {
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
        engine.update(
            file_id,
            FileAnalysis {
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

    let resolve_pass = engine.view().last_resolve_stats();
    assert_eq!(resolve_pass.get(ResolveStat::ConstantCacheMisses), 1);
    assert_eq!(resolve_pass.get(ResolveStat::ConstantCacheHits), 1);
    assert_eq!(resolve_pass.get(ResolveStat::ConstantCacheUniqueKeys), 1);
    assert_eq!(engine.view().reference_facts_for(&user).len(), 2);
}

#[test]
fn resolve_local_call_outcome_caches_reuse_one_exact_method_proof() {
    let mut engine = Project::new();
    let def_file = register_project_file(
        &mut engine,
        "app/user.rb",
        "class User; def name = 'Ada'; end",
    );
    let ref_file =
        register_project_file(&mut engine, "app/use_user.rb", "first.name\nsecond.name\n");
    let user = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);
    let instance_user =
        FullyQualifiedName::namespace_with_kind(user.namespace_parts(), NamespaceKind::Instance);
    let method = RubyMethod::new("name").unwrap();
    let method_fqn = FullyQualifiedName::method(user.namespace_parts(), method);
    let method_range = TextRange::new(def_file, 12, 28);

    engine.update(
        def_file,
        FileAnalysis {
            graph_nodes: vec![GraphNodeFact::new(
                user.clone(),
                GraphNodeKind::Class,
                TextRange::new(def_file, 0, 34),
            )],
            methods: vec![MethodFact::new(
                method_fqn.clone(),
                instance_user,
                method_range,
            )],
            types: vec![TypeFact::new(
                TypeSubject::MethodReturn(method_fqn),
                RubyType::string(),
                method_range,
                TypeProvenance::Inferred,
            )],
            ..Default::default()
        },
        ResolveMode::Deferred,
    );

    let first_call = TextRange::new(ref_file, 0, 10);
    let second_call = TextRange::new(ref_file, 11, 22);
    let candidates = [first_call, second_call]
        .into_iter()
        .map(|call_range| {
            let method_range =
                TextRange::new(ref_file, call_range.end_byte - 4, call_range.end_byte);
            ReferenceCandidate::method(
                method_range,
                crate::core::MethodReferenceCandidate {
                    owner: user.namespace_parts(),
                    owner_kind: NamespaceKind::Instance,
                    method,
                    is_super: false,
                    access: crate::core::MethodReferenceAccess::ExplicitReceiver,
                    caller: None,
                    call_expression_range: Some(call_range),
                    preferred_definition_range: None,
                    diagnostics: crate::core::MethodReferenceDiagnostics {
                        diagnostic_range: method_range,
                        receiver_label: Some(crate::core::MethodReceiverLabel::Written(
                            "User".to_string(),
                        )),
                        receiver_expression_range: None,
                        receiver_type: None,
                        diagnose_unresolved: true,
                        allow_unindexed_owner: false,
                        safe_navigation: false,
                        signature: Some(crate::core::MethodCallSignatureCandidate::default()),
                    },
                },
            )
        })
        .collect();
    engine.update(
        ref_file,
        FileAnalysis {
            reference_candidates: candidates,
            ..Default::default()
        },
        ResolveMode::Deferred,
    );

    engine.resolve();

    let resolve_pass = engine.view().last_resolve_stats();
    assert_eq!(resolve_pass.get(ResolveStat::MethodReturnCacheMisses), 1);
    assert_eq!(resolve_pass.get(ResolveStat::MethodReturnCacheHits), 1);
    assert_eq!(resolve_pass.get(ResolveStat::MethodReturnCacheEntries), 1);
    assert_eq!(
        resolve_pass.get(ResolveStat::MethodVisibilityCacheMisses),
        1
    );
    assert_eq!(resolve_pass.get(ResolveStat::MethodVisibilityCacheHits), 1);
    assert_eq!(
        resolve_pass.get(ResolveStat::MethodVisibilityCacheEntries),
        1
    );
    let query = engine.view();
    let outcomes = query
        .call_expression_outcomes_in_file(ref_file)
        .expect("resolved calls must retain proof outcomes");
    assert_eq!(outcomes.len(), 2);
    assert!(outcomes
        .iter()
        .all(|(_, outcome)| outcome.proven_type() == Some(&RubyType::string())));
}

#[test]
fn resolve_local_call_outcome_cache_reuses_one_ambiguous_method_proof() {
    let mut engine = Project::new();
    let first_def = register_project_file(
        &mut engine,
        "app/user_first.rb",
        "class User; def name = 'Ada'; end",
    );
    let second_def = register_project_file(
        &mut engine,
        "app/user_second.rb",
        "class User; def name = 'Lovelace'; end",
    );
    let ref_file =
        register_project_file(&mut engine, "app/use_user.rb", "first.name\nsecond.name\n");
    let user = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);
    let instance_user =
        FullyQualifiedName::namespace_with_kind(user.namespace_parts(), NamespaceKind::Instance);
    let method = RubyMethod::new("name").unwrap();
    let method_fqn = FullyQualifiedName::method(user.namespace_parts(), method);

    for (file_id, range) in [
        (first_def, TextRange::new(first_def, 12, 28)),
        (second_def, TextRange::new(second_def, 12, 33)),
    ] {
        engine.update(
            file_id,
            FileAnalysis {
                graph_nodes: vec![GraphNodeFact::new(
                    user.clone(),
                    GraphNodeKind::Class,
                    TextRange::new(file_id, 0, range.end_byte + 6),
                )],
                methods: vec![MethodFact::new(
                    method_fqn.clone(),
                    instance_user.clone(),
                    range,
                )],
                types: vec![TypeFact::new(
                    TypeSubject::MethodReturn(method_fqn.clone()),
                    RubyType::string(),
                    range,
                    TypeProvenance::Inferred,
                )],
                ..Default::default()
            },
            ResolveMode::Deferred,
        );
    }

    let first_call = TextRange::new(ref_file, 0, 10);
    let second_call = TextRange::new(ref_file, 11, 22);
    let candidates = [first_call, second_call]
        .into_iter()
        .map(|call_range| {
            let method_range =
                TextRange::new(ref_file, call_range.end_byte - 4, call_range.end_byte);
            ReferenceCandidate::method(
                method_range,
                crate::core::MethodReferenceCandidate {
                    owner: user.namespace_parts(),
                    owner_kind: NamespaceKind::Instance,
                    method,
                    is_super: false,
                    access: crate::core::MethodReferenceAccess::Normal,
                    caller: None,
                    call_expression_range: Some(call_range),
                    preferred_definition_range: None,
                    diagnostics: crate::core::MethodReferenceDiagnostics {
                        diagnostic_range: method_range,
                        receiver_label: Some(crate::core::MethodReceiverLabel::Written(
                            "User".to_string(),
                        )),
                        receiver_expression_range: None,
                        receiver_type: None,
                        diagnose_unresolved: true,
                        allow_unindexed_owner: false,
                        safe_navigation: false,
                        signature: Some(crate::core::MethodCallSignatureCandidate::default()),
                    },
                },
            )
        })
        .collect();
    engine.update(
        ref_file,
        FileAnalysis {
            reference_candidates: candidates,
            ..Default::default()
        },
        ResolveMode::Deferred,
    );

    engine.resolve();

    let resolve_pass = engine.view().last_resolve_stats();
    assert_eq!(
        resolve_pass.get(ResolveStat::AmbiguousMethodReturnCacheMisses),
        1
    );
    assert_eq!(
        resolve_pass.get(ResolveStat::AmbiguousMethodReturnCacheHits),
        1
    );
    assert_eq!(
        resolve_pass.get(ResolveStat::AmbiguousMethodReturnCacheEntries),
        1
    );
    let query = engine.view();
    let outcomes = query
        .call_expression_outcomes_in_file(ref_file)
        .expect("ambiguous resolved calls must retain proof outcomes");
    assert_eq!(outcomes.len(), 2);
    assert!(outcomes
        .iter()
        .all(|(_, outcome)| outcome.proven_type() == Some(&RubyType::string())));
}

#[test]
fn nested_call_uses_the_same_pass_inner_outcome_as_deferred_receiver() {
    let mut engine = Project::new();
    let def_file = register_project_file(
        &mut engine,
        "app/user.rb",
        "class User; def child; self; end; def name = 'Ada'; end",
    );
    let ref_file = register_project_file(&mut engine, "app/use_user.rb", "user.child.name");
    let user = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);
    let user_constant = FullyQualifiedName::constant(user.namespace_parts());
    let instance_user =
        FullyQualifiedName::namespace_with_kind(user.namespace_parts(), NamespaceKind::Instance);
    let child = RubyMethod::new("child").unwrap();
    let name = RubyMethod::new("name").unwrap();
    let child_fqn = FullyQualifiedName::method(user.namespace_parts(), child);
    let name_fqn = FullyQualifiedName::method(user.namespace_parts(), name);
    let child_def = TextRange::new(def_file, 12, 30);
    let name_def = TextRange::new(def_file, 32, 50);

    engine.update(
        def_file,
        FileAnalysis {
            graph_nodes: vec![GraphNodeFact::new(
                user.clone(),
                GraphNodeKind::Class,
                TextRange::new(def_file, 0, 54),
            )],
            methods: vec![
                MethodFact::new(child_fqn.clone(), instance_user.clone(), child_def),
                MethodFact::new(name_fqn.clone(), instance_user, name_def),
            ],
            types: vec![
                TypeFact::new(
                    TypeSubject::MethodReturn(child_fqn),
                    RubyType::Class(user_constant),
                    child_def,
                    TypeProvenance::Inferred,
                ),
                TypeFact::new(
                    TypeSubject::MethodReturn(name_fqn),
                    RubyType::string(),
                    name_def,
                    TypeProvenance::Inferred,
                ),
            ],
            ..Default::default()
        },
        ResolveMode::Deferred,
    );

    let inner_call = TextRange::new(ref_file, 0, 10);
    let outer_call = TextRange::new(ref_file, 0, 15);
    let missing_owner = vec![RubyConstant::new("MissingOwner").unwrap()];
    engine.update(
        ref_file,
        FileAnalysis {
            reference_candidates: vec![
                explicit_method_call_candidate(
                    TextRange::new(ref_file, 5, 10),
                    inner_call,
                    user.namespace_parts(),
                    NamespaceKind::Instance,
                    child,
                    None,
                ),
                explicit_method_call_candidate(
                    TextRange::new(ref_file, 11, 15),
                    outer_call,
                    missing_owner,
                    NamespaceKind::Instance,
                    name,
                    Some(inner_call),
                ),
            ],
            ..Default::default()
        },
        ResolveMode::Deferred,
    );

    engine.resolve();

    let resolve_pass = engine.view().last_resolve_stats();
    assert_eq!(resolve_pass.get(ResolveStat::DeferredReceiverCandidates), 1);
    assert_eq!(resolve_pass.get(ResolveStat::DeferredReceiverProven), 1);
    assert_eq!(resolve_pass.get(ResolveStat::DeferredReceiverUnknown), 0);
    let query = engine.view();
    let outcomes = query
        .call_expression_outcomes_in_file(ref_file)
        .expect("nested calls must retain same-pass proof outcomes");
    assert_eq!(outcomes.len(), 2);
    let inner = outcomes
        .iter()
        .find(|(range, _)| *range == inner_call)
        .map(|(_, outcome)| outcome.proven_type().cloned());
    let outer = outcomes
        .iter()
        .find(|(range, _)| *range == outer_call)
        .map(|(_, outcome)| outcome.proven_type().cloned());
    assert_eq!(
        inner,
        Some(Some(RubyType::Class(FullyQualifiedName::constant(
            user.namespace_parts()
        ))))
    );
    assert_eq!(outer, Some(Some(RubyType::string())));
}

#[test]
fn file_owned_call_outcome_survives_resolve_merge_on_a_disjoint_range() {
    let mut engine = Project::new();
    let def_file = register_project_file(
        &mut engine,
        "app/user.rb",
        "class User; def name = 'Ada'; end",
    );
    let ref_file = register_project_file(&mut engine, "app/use_user.rb", "kept()\nuser.name");
    let user = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);
    let instance_user =
        FullyQualifiedName::namespace_with_kind(user.namespace_parts(), NamespaceKind::Instance);
    let method = RubyMethod::new("name").unwrap();
    let method_fqn = FullyQualifiedName::method(user.namespace_parts(), method);
    let method_range = TextRange::new(def_file, 12, 28);
    let kept_range = TextRange::new(ref_file, 0, 6);
    let name_call = TextRange::new(ref_file, 7, 16);

    engine.update(
        def_file,
        FileAnalysis {
            graph_nodes: vec![GraphNodeFact::new(
                user.clone(),
                GraphNodeKind::Class,
                TextRange::new(def_file, 0, 34),
            )],
            methods: vec![MethodFact::new(
                method_fqn.clone(),
                instance_user,
                method_range,
            )],
            types: vec![TypeFact::new(
                TypeSubject::MethodReturn(method_fqn),
                RubyType::string(),
                method_range,
                TypeProvenance::Inferred,
            )],
            ..Default::default()
        },
        ResolveMode::Deferred,
    );
    engine.update(
        ref_file,
        FileAnalysis {
            inference: InferenceEvidence {
                call_expression_outcomes: vec![(
                    kept_range,
                    TypeInferenceOutcome::proven(RubyType::integer()),
                )],
                ..Default::default()
            },
            reference_candidates: vec![explicit_method_call_candidate(
                TextRange::new(ref_file, 12, 16),
                name_call,
                user.namespace_parts(),
                NamespaceKind::Instance,
                method,
                None,
            )],
            ..Default::default()
        },
        ResolveMode::Deferred,
    );

    engine.resolve();

    let query = engine.view();
    let outcomes = query
        .call_expression_outcomes_in_file(ref_file)
        .expect("disjoint file-owned and resolved call outcomes must both remain");
    assert_eq!(outcomes.len(), 2);
    let kept = outcomes
        .iter()
        .find(|(range, _)| *range == kept_range)
        .map(|(_, outcome)| outcome.proven_type().cloned());
    let resolved = outcomes
        .iter()
        .find(|(range, _)| *range == name_call)
        .map(|(_, outcome)| outcome.proven_type().cloned());
    assert_eq!(kept, Some(Some(RubyType::integer())));
    assert_eq!(resolved, Some(Some(RubyType::string())));
}

#[test]
#[should_panic(expected = "one call expression resolved through multiple method candidates")]
fn duplicate_call_expression_range_is_an_invariant_violation() {
    let mut engine = Project::new();
    let def_file = register_project_file(
        &mut engine,
        "app/user.rb",
        "class User; def name = 'Ada'; end",
    );
    let ref_file = register_project_file(&mut engine, "app/use_user.rb", "user.name");
    let user = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);
    let instance_user =
        FullyQualifiedName::namespace_with_kind(user.namespace_parts(), NamespaceKind::Instance);
    let method = RubyMethod::new("name").unwrap();
    let method_fqn = FullyQualifiedName::method(user.namespace_parts(), method);
    let method_range = TextRange::new(def_file, 12, 28);
    let call_range = TextRange::new(ref_file, 0, 9);

    engine.update(
        def_file,
        FileAnalysis {
            graph_nodes: vec![GraphNodeFact::new(
                user.clone(),
                GraphNodeKind::Class,
                TextRange::new(def_file, 0, 34),
            )],
            methods: vec![MethodFact::new(
                method_fqn.clone(),
                instance_user,
                method_range,
            )],
            types: vec![TypeFact::new(
                TypeSubject::MethodReturn(method_fqn),
                RubyType::string(),
                method_range,
                TypeProvenance::Inferred,
            )],
            ..Default::default()
        },
        ResolveMode::Deferred,
    );
    engine.update(
        ref_file,
        FileAnalysis {
            reference_candidates: vec![
                explicit_method_call_candidate(
                    TextRange::new(ref_file, 5, 9),
                    call_range,
                    user.namespace_parts(),
                    NamespaceKind::Instance,
                    method,
                    None,
                ),
                explicit_method_call_candidate(
                    TextRange::new(ref_file, 5, 8),
                    call_range,
                    user.namespace_parts(),
                    NamespaceKind::Instance,
                    method,
                    None,
                ),
            ],
            ..Default::default()
        },
        ResolveMode::Deferred,
    );

    engine.resolve();
}

#[test]
fn resolve_files_materializes_only_selected_open_document_candidates() {
    let mut engine = Project::new();
    let first_ref = register_project_file(&mut engine, "app/first.rb", "User.new");
    let second_ref = register_project_file(&mut engine, "app/second.rb", "User.new");
    let def_file = register_project_file(&mut engine, "app/user.rb", "class User; end");
    let user = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);

    for file_id in [first_ref, second_ref] {
        engine.update(
            file_id,
            FileAnalysis {
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
    engine.update(
        def_file,
        FileAnalysis {
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

    assert_eq!(View::new(&engine).references_in_file(first_ref).len(), 1);
    assert!(
        View::new(&engine).references_in_file(second_ref).is_empty(),
        "closed-file candidates must remain deferred until the complete project resolution"
    );

    engine.resolve();
    assert_eq!(View::new(&engine).references_in_file(second_ref).len(), 1);
}

#[test]
fn reopened_method_return_requires_every_definition_to_resolve() {
    let mut engine = Project::new();
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

    engine.update(
        known_file,
        FileAnalysis {
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
    engine.update(
        unresolved_file,
        FileAnalysis {
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

    let callees = ask_any(&engine.view(), &owner, &method_name, MethodWant::Callees)
        .into_callees()
        .expect("reopened Service#value must resolve");
    assert_eq!(callees.len(), 1);
    invariant_eq!(
        ask_any(&engine.view(), &owner, &method_name, MethodWant::Return).into_return_type(),
        None,
        what = "receiver return inference discarded an unresolved reopened method definition",
        why = "every definition is a reachable static outcome",
        fix = "return Unknown/None unless every matching definition proves a return type",
    );
    invariant_eq!(
        engine.view().method_return_type_for_callee(&callees[0]),
        None,
        what =
            "resolved-callee return inference discarded an unresolved reopened method definition",
        why = "chained calls would consume a partial concrete type",
        fix = "require a return type for every resolved definition range",
    );
}

#[test]
fn default_basic_object_method_missing_is_not_a_return_type() {
    let mut engine = Project::new();
    let stub_file = engine.register_file(SourceFileInput {
        path: "core/basic_object.rb".into(),
        content: "class BasicObject; def method_missing(name, *args); end; end".into(),
        kind: SourceKind::Stub,
    });
    let project_file = register_project_file(
        &mut engine,
        "lib/widget.rb",
        "class Widget; end\nclass Dynamic; def method_missing(name, *args); 1; end; end\n",
    );
    let basic_object =
        FullyQualifiedName::namespace(vec![RubyConstant::new("BasicObject").unwrap()]);
    let widget = FullyQualifiedName::namespace(vec![RubyConstant::new("Widget").unwrap()]);
    let dynamic = FullyQualifiedName::namespace(vec![RubyConstant::new("Dynamic").unwrap()]);
    let method_missing = RubyMethod::new("method_missing").unwrap();
    let ghost = RubyMethod::new("ghost").unwrap();
    let stub_range = TextRange::new(stub_file, 18, 56);
    let dynamic_range = TextRange::new(project_file, 28, 70);
    let stub_method = FullyQualifiedName::method(basic_object.namespace_parts(), method_missing);
    let dynamic_method = FullyQualifiedName::method(dynamic.namespace_parts(), method_missing);

    engine.update(
        stub_file,
        FileAnalysis {
            graph_nodes: vec![GraphNodeFact::new(
                basic_object.clone(),
                GraphNodeKind::Class,
                TextRange::new(stub_file, 0, 62),
            )],
            methods: vec![MethodFact::new(
                stub_method.clone(),
                basic_object.clone(),
                stub_range,
            )],
            types: vec![TypeFact::new(
                TypeSubject::MethodReturn(stub_method),
                RubyType::string(),
                stub_range,
                TypeProvenance::Rbs,
            )],
            ..Default::default()
        },
        ResolveMode::Deferred,
    );
    engine.update(
        project_file,
        FileAnalysis {
            graph_nodes: vec![
                GraphNodeFact::new(
                    widget.clone(),
                    GraphNodeKind::Class,
                    TextRange::new(project_file, 0, 14),
                ),
                GraphNodeFact::new(
                    dynamic.clone(),
                    GraphNodeKind::Class,
                    TextRange::new(project_file, 15, 80),
                ),
            ],
            graph_edges: vec![
                GraphEdgeFact::new(
                    widget.clone(),
                    basic_object.clone(),
                    GraphEdgeKind::Superclass,
                    TextRange::new(project_file, 0, 14),
                ),
                GraphEdgeFact::new(
                    dynamic.clone(),
                    basic_object.clone(),
                    GraphEdgeKind::Superclass,
                    TextRange::new(project_file, 15, 29),
                ),
            ],
            methods: vec![MethodFact::new(
                dynamic_method.clone(),
                dynamic.clone(),
                dynamic_range,
            )],
            types: vec![TypeFact::new(
                TypeSubject::MethodReturn(dynamic_method),
                RubyType::integer(),
                dynamic_range,
                TypeProvenance::Inferred,
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert!(
        method_lookup_chain(&engine, &widget)
            .iter()
            .any(|fqn| fqn == &basic_object),
        "Widget must inherit BasicObject so the stub method_missing is on the lookup chain"
    );
    invariant_eq!(
        ask_any(&engine.view(), &widget, &ghost, MethodWant::Return).into_return_type(),
        None,
        what = "Widget#ghost inherited BasicObject#method_missing's stub return",
        why = "default language fallback is not a proven return",
        fix = "skip stub/signature BasicObject#method_missing in receiver return lookup",
    );
    assert_eq!(
        ask_any(&engine.view(), &dynamic, &ghost, MethodWant::Return).into_return_type(),
        Some(RubyType::integer()),
        "a project method_missing must still prove the fallback return"
    );
}

#[test]
fn expression_end_query_treats_exact_unknown_call_outcome_as_authoritative() {
    let mut engine = Project::new();
    let file_id = register_project_file(&mut engine, "consumer.rb", "payload[:name]\n");
    let range = TextRange::new(file_id, 0, 14);
    engine.update(
        file_id,
        FileAnalysis {
            types: vec![TypeFact::new(
                TypeSubject::Expression(range),
                RubyType::string(),
                range,
                TypeProvenance::Inferred,
            )],
            inference: InferenceEvidence {
                call_expression_outcomes: vec![(
                    range,
                    TypeInferenceOutcome::unknown(UnknownReason::MutableShapeInvalidated),
                )],
                ..Default::default()
            },
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert_eq!(
        engine.view().expression_type_ending_at(file_id, 14),
        Some(RubyType::Unknown),
        "a call-level Unknown must prevent completion from using a stale expression fact"
    );
    assert_eq!(
        engine.view().proven_expression_type_ending_at(file_id, 14),
        None,
        "proven-only consumers must omit the same exact Unknown"
    );
}
