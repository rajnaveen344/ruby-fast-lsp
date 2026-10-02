//! Reference resolution, rename targets, completion receivers, and definition ranking.

use super::*;

#[test]
fn union_method_completion_requires_every_receiver_member() {
    let mut engine = Project::new();
    let file_id = register_project_file(&mut engine, "app/types.rb", "class Alpha; end");
    let alpha = FullyQualifiedName::namespace_with_kind(
        vec![RubyConstant::new("Alpha").unwrap()],
        NamespaceKind::Instance,
    );
    let beta = FullyQualifiedName::namespace_with_kind(
        vec![RubyConstant::new("Beta").unwrap()],
        NamespaceKind::Instance,
    );
    let shared = RubyMethod::new("shared").unwrap();
    let alpha_only = RubyMethod::new("alpha_only").unwrap();
    let beta_only = RubyMethod::new("beta_only").unwrap();
    let alpha_shared = FullyQualifiedName::method(alpha.namespace_parts(), shared.clone());
    let beta_shared = FullyQualifiedName::method(beta.namespace_parts(), shared);
    let range = crate::core::TextRange::new(file_id, 0, 1);

    engine.update(
        file_id,
        FileAnalysis {
            graph_nodes: vec![
                GraphNodeFact::new(alpha.clone(), GraphNodeKind::Class, range),
                GraphNodeFact::new(beta.clone(), GraphNodeKind::Class, range),
            ],
            methods: vec![
                MethodFact::with_params(
                    alpha_shared.clone(),
                    alpha.clone(),
                    range,
                    vec!["value".to_string()],
                ),
                MethodFact::new(
                    FullyQualifiedName::method(alpha.namespace_parts(), alpha_only),
                    alpha.clone(),
                    range,
                ),
                MethodFact::with_params(
                    beta_shared.clone(),
                    beta.clone(),
                    range,
                    vec!["value".to_string()],
                ),
                MethodFact::new(
                    FullyQualifiedName::method(beta.namespace_parts(), beta_only),
                    beta.clone(),
                    range,
                ),
            ],
            types: vec![
                TypeFact::new(
                    TypeSubject::MethodReturn(alpha_shared),
                    RubyType::string(),
                    range,
                    TypeProvenance::Inferred,
                ),
                TypeFact::new(
                    TypeSubject::MethodReturn(beta_shared),
                    RubyType::integer(),
                    range,
                    TypeProvenance::Inferred,
                ),
            ],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    let receiver = RubyType::union([
        RubyType::Class(FullyQualifiedName::constant(alpha.namespace_parts())),
        RubyType::Class(FullyQualifiedName::constant(beta.namespace_parts())),
    ]);
    let matches = engine
        .view()
        .method_matches_for_type(&receiver, "", NamespaceKind::Instance);

    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].name, "shared");
    assert_eq!(matches[0].params, vec!["value"]);
    assert_eq!(
        matches[0].return_type,
        Some(RubyType::union([RubyType::integer(), RubyType::string()]))
    );

    let query = engine.view();
    let shared = RubyMethod::new("shared").unwrap();
    let protected = ReceiverAccess::Protected { caller: &alpha };
    let ask = |method, want| {
        let request = MethodRequest::new(LookupReceiver::Type(&receiver), method, want);
        lookup::method(&query, request.with_access(protected))
    };
    let exact_callees = ask(shared, MethodWant::Callees)
        .into_callees()
        .expect("shared must resolve exactly for every union receiver member");
    assert_eq!(exact_callees.len(), 2);
    assert!(exact_callees
        .iter()
        .all(|callee| callee.resolution == MethodCalleeResolution::Exact));

    let signature_facts = ask(shared, MethodWant::Signatures).into_signature_vec();
    assert_eq!(signature_facts.len(), 2);
    assert!(signature_facts
        .iter()
        .all(|fact| fact.params == vec!["value"]));

    let alpha_only = RubyMethod::new("alpha_only").unwrap();
    assert!(
        ask(alpha_only, MethodWant::Callees)
            .into_callees()
            .is_none(),
        "a partial union method must not return one member's navigation target"
    );
    assert!(
        ask(alpha_only, MethodWant::Signatures)
            .into_signatures()
            .is_empty(),
        "a partial union method must not return one member's signature"
    );
}

#[test]
fn method_rename_rejects_external_definition_even_with_exact_name_range() {
    let mut engine = Project::new();
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
    engine.update(
        file_id,
        FileAnalysis {
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
        View::new(&engine)
            .method_rename_target_at(file_id, 17)
            .is_none(),
        "dependency sources are navigation inputs, never editable rename truth"
    );
}

#[test]
fn reference_candidate_resolves_when_definition_arrives_later() {
    let mut engine = Project::new();
    let ref_file = register_project_file(&mut engine, "app/use_user.rb", "User.new");
    let def_file = register_project_file(&mut engine, "app/user.rb", "class User; end");
    let user_name = RubyConstant::new("User").unwrap();
    let user = FullyQualifiedName::namespace(vec![user_name]);

    engine.update(
        ref_file,
        FileAnalysis {
            reference_candidates: vec![ReferenceCandidate::constant(
                TextRange::new(ref_file, 0, 4),
                user.namespace_parts(),
                Vec::new(),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert!(engine.view().reference_facts_for(&user).is_empty());
    assert!(engine
        .view()
        .diagnostic_facts_in_file(ref_file)
        .iter()
        .any(|fact| fact.code == "unresolved-constant"));

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
        ResolveMode::Immediate,
    );

    assert_eq!(engine.view().reference_facts_for(&user).len(), 1);
    assert!(engine
        .view()
        .diagnostic_facts_in_file(ref_file)
        .iter()
        .all(|fact| fact.code != "unresolved-constant"));
}

#[test]
fn resolved_reference_definition_query_requires_one_exact_target() {
    let mut engine = Project::new();
    let source_file = register_project_file(&mut engine, "app/model.rb", "field :user");
    let user_file = register_project_file(&mut engine, "app/user.rb", "class User; end");
    let account_file = register_project_file(&mut engine, "app/account.rb", "class Account; end");
    let user = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);
    let account = FullyQualifiedName::namespace(vec![RubyConstant::new("Account").unwrap()]);
    let user_range = TextRange::new(user_file, 0, 10);
    let account_range = TextRange::new(account_file, 0, 13);

    engine.update(
        user_file,
        FileAnalysis {
            symbols: vec![SymbolFact::new(user.clone(), SymbolKind::Class, user_range)],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    engine.update(
        account_file,
        FileAnalysis {
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
    engine.update(
        source_file,
        FileAnalysis {
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
        View::new(&engine).resolved_reference_definition_ranges_at(source_file, 8),
        vec![user_range]
    );

    engine.update(
        source_file,
        FileAnalysis {
            reference_candidates: vec![
                ReferenceCandidate::resolved(reference_range, user, None),
                ReferenceCandidate::resolved(reference_range, account, None),
            ],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    assert!(
        View::new(&engine)
            .resolved_reference_definition_ranges_at(source_file, 8)
            .is_empty(),
        "ambiguous resolved reference targets must not guess a definition"
    );
}

#[test]
fn exact_method_reference_uses_engine_resolution_and_lifecycle() {
    let mut engine = Project::new();
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

    engine.update(
        model_file,
        FileAnalysis {
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
            .with_visibility(crate::core::MethodVisibility::Private)],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    engine.update(
        callback_file,
        FileAnalysis {
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
        View::new(&engine).resolved_reference_definition_ranges_at(callback_file, 15),
        vec![method_range],
        "exact callback target must use normal engine method lookup, including private methods"
    );
    assert_eq!(
        engine
            .view()
            .reference_facts_for(&FullyQualifiedName::method(user.namespace_parts(), method))
            .len(),
        1,
        "exact callback target must participate in ordinary method references"
    );

    engine.update(
        callback_file,
        FileAnalysis::default(),
        ResolveMode::Immediate,
    );
    assert!(
        View::new(&engine)
            .resolved_reference_definition_ranges_at(callback_file, 15)
            .is_empty(),
        "removing callback facts must remove exact method navigation"
    );
}

#[test]
fn exact_method_reference_prefers_a_verified_declaration_and_falls_back_after_removal() {
    let mut engine = Project::new();
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
    engine.update(
        signature_file,
        FileAnalysis {
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
    engine.update(
        implementation_file,
        FileAnalysis {
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
    engine.update(
        source_file,
        FileAnalysis {
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
                        receiver_expression_range: None,
                        receiver_type: None,
                        diagnose_unresolved: false,
                        allow_unindexed_owner: false,
                        safe_navigation: false,
                        signature: Some(crate::core::MethodCallSignatureCandidate::default()),
                    },
                },
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert_eq!(
        View::new(&engine).resolved_reference_definition_ranges_at(source_file, 17),
        vec![int_range],
        "a verified JVM overload range must outrank same-named methods and signatures"
    );

    engine.update(
        implementation_file,
        FileAnalysis::default(),
        ResolveMode::Immediate,
    );
    assert_eq!(
        View::new(&engine).resolved_reference_definition_ranges_at(source_file, 17),
        vec![signature_range],
        "a removed preferred declaration must not leave a stale location and must fall back normally"
    );
}

#[test]
fn runtime_constant_alias_definition_prefers_the_external_proxy_declaration() {
    let mut engine = Project::new();
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
    engine.update(
        import_file,
        FileAnalysis {
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
            ..FileAnalysis::default()
        },
        ResolveMode::Immediate,
    );
    engine.update(
        implementation_file,
        FileAnalysis {
            symbols: vec![SymbolFact::new(
                proxy,
                SymbolKind::Class,
                implementation_range,
            )],
            ..FileAnalysis::default()
        },
        ResolveMode::Immediate,
    );

    assert_eq!(
        View::new(&engine)
            .constant_definition_ranges(&[RubyConstant::new("TimeUnit").unwrap()], &[]),
        vec![implementation_range],
        "a runtime import alias must navigate to the implementation class instead of its import statement"
    );
}

#[test]
fn method_candidate_resolves_when_method_definition_arrives_later() {
    let mut engine = Project::new();
    let ref_file = register_project_file(&mut engine, "app/use_user.rb", "user.name");
    let def_file =
        register_project_file(&mut engine, "app/user.rb", "class User; def name; end; end");
    let user_name = RubyConstant::new("User").unwrap();
    let user = FullyQualifiedName::namespace(vec![user_name]);
    let method = RubyMethod::new("name").unwrap();
    let method_fqn = FullyQualifiedName::method(user.namespace_parts(), method);

    engine.update(
        def_file,
        FileAnalysis {
            graph_nodes: vec![GraphNodeFact::new(
                user.clone(),
                GraphNodeKind::Class,
                TextRange::new(def_file, 0, 10),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    engine.update(
        ref_file,
        FileAnalysis {
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
                        receiver_expression_range: None,
                        receiver_type: None,
                        diagnose_unresolved: true,
                        allow_unindexed_owner: false,
                        safe_navigation: false,
                        signature: Some(crate::core::MethodCallSignatureCandidate::default()),
                    },
                },
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert_eq!(engine.view().reference_facts_for(&method_fqn).len(), 1);
    assert!(engine
        .view()
        .diagnostic_facts_in_file(ref_file)
        .iter()
        .any(|fact| fact.code == "unresolved-method"));

    engine.update(
        def_file,
        FileAnalysis {
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

    assert_eq!(engine.view().reference_facts_for(&method_fqn).len(), 1);
    assert!(engine
        .view()
        .diagnostic_facts_in_file(ref_file)
        .iter()
        .all(|fact| fact.code != "unresolved-method"));
    assert_eq!(
        View::new(&engine).resolved_reference_definition_ranges_at(ref_file, 6),
        vec![TextRange::new(def_file, 12, 20)],
        "ordinary diagnostics-bearing method candidates must navigate through their resolved reference fact"
    );
}

#[test]
fn constant_rename_rejects_external_only_definition() {
    let mut engine = Project::new();
    let file_id = engine.register_file(SourceFileInput {
        path: "gem/user.rb".into(),
        content: "class User\nend\n".to_string(),
        kind: SourceKind::Gem,
    });
    let user = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);
    engine.update(
        file_id,
        FileAnalysis {
            symbols: vec![
                SymbolFact::new(user, SymbolKind::Class, TextRange::new(file_id, 0, 14))
                    .with_name_range(TextRange::new(file_id, 6, 10)),
            ],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert!(engine
        .view()
        .constant_rename_target(&[RubyConstant::new("User").unwrap()], &[])
        .is_none());
}

#[test]
fn method_navigation_prefers_implementation_over_matching_rbs_declaration() {
    let mut engine = Project::new();
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

    engine.update(
        signature_file,
        FileAnalysis {
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
    engine.update(
        implementation_file,
        FileAnalysis {
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

    let callees = ask_any(&engine.view(), &owner, &method_name, MethodWant::Callees)
        .into_callees()
        .expect("method owner must resolve");
    assert_eq!(callees.len(), 1);
    assert_eq!(callees[0].definition_ranges, vec![implementation_range]);
    let signatures =
        ask_any(&engine.view(), &owner, &method_name, MethodWant::Signatures).into_signature_vec();
    assert_eq!(signatures.len(), 1);
    assert_eq!(signatures[0].range, signature_range);
    assert_eq!(signatures[0].return_type_label.as_deref(), Some("String"));
    assert_eq!(
        ask_any(&engine.view(), &owner, &method_name, MethodWant::Return).into_return_type(),
        Some(RubyType::string())
    );
    assert_eq!(
        engine.view().method_return_type_for_callee(&callees[0]),
        Some(RubyType::string()),
        "an already-resolved callee must expose its return type without another receiver lookup"
    );
    assert_eq!(
        engine
            .view()
            .constant_definition_ranges(&[RubyConstant::new("Widget").unwrap()], &[],),
        vec![TextRange::new(implementation_file, 0, 37)]
    );
}

#[test]
fn inherited_method_callee_keeps_the_defining_parent_owner() {
    let mut engine = Project::new();
    let file_id = register_project_file(
        &mut engine,
        "lib/child.rb",
        "class Parent\n  def value = 'ok'\nend\nclass Child < Parent\nend\n",
    );
    let parent = FullyQualifiedName::namespace(vec![RubyConstant::new("Parent").unwrap()]);
    let child = FullyQualifiedName::namespace(vec![RubyConstant::new("Child").unwrap()]);
    let method = RubyMethod::new("value").unwrap();
    let definition_range = TextRange::new(file_id, 15, 31);
    engine.update(
        file_id,
        FileAnalysis {
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
                parent.clone(),
                GraphEdgeKind::Superclass,
                TextRange::new(file_id, 39, 59),
            )],
            methods: vec![MethodFact::new(
                FullyQualifiedName::method(parent.namespace_parts(), method),
                parent.clone(),
                definition_range,
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    let callees = ask_any(&engine.view(), &child, &method, MethodWant::Callees)
        .into_callees()
        .expect("Child must resolve Parent#value through ordinary ancestry");
    assert_eq!(callees.len(), 1);
    assert_eq!(callees[0].owner, parent);
    assert_eq!(callees[0].resolution, MethodCalleeResolution::Exact);
    assert_eq!(callees[0].definition_ranges, vec![definition_range]);
}

#[test]
fn public_lookup_of_a_private_method_is_receiver_only() {
    let mut engine = Project::new();
    let file_id = register_project_file(
        &mut engine,
        "lib/user.rb",
        "class User\n  def secret; end\nend\n",
    );
    let owner = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);
    let method = RubyMethod::new("secret").unwrap();
    let definition_range = TextRange::new(file_id, 13, 24);
    engine.update(
        file_id,
        FileAnalysis {
            graph_nodes: vec![GraphNodeFact::new(
                owner.clone(),
                GraphNodeKind::Class,
                TextRange::new(file_id, 0, 34),
            )],
            methods: vec![MethodFact::new(
                FullyQualifiedName::method(owner.namespace_parts(), method),
                owner.clone(),
                definition_range,
            )
            .with_visibility(crate::core::MethodVisibility::Private)],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    let private_callees = ask_any(&engine.view(), &owner, &method, MethodWant::Callees)
        .into_callees()
        .expect("private lookup must still see User#secret");
    assert_eq!(private_callees.len(), 1);
    assert_eq!(private_callees[0].resolution, MethodCalleeResolution::Exact);
    assert_eq!(private_callees[0].definition_ranges, vec![definition_range]);

    let public_callees = ask(
        &engine.view(),
        &owner,
        &method,
        ReceiverAccess::Public,
        MethodWant::Callees,
    )
    .into_callees()
    .expect("public lookup must retain the receiver when the method is private");
    assert_eq!(public_callees.len(), 1);
    assert_eq!(
        public_callees[0].resolution,
        MethodCalleeResolution::ReceiverOnly
    );
    assert!(public_callees[0].definition_ranges.is_empty());
}
