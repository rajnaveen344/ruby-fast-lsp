//! Graph edge retry, superclass and mixin ancestry, generated owners, and execution contexts.

use super::*;

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
fn delayed_class_superclass_materializes_singleton_inheritance() {
    let mut engine = AnalysisEngine::new();
    let child_file = register_project_file(&mut engine, "child.rb", "class Child < Parent; end");
    let parent_file = register_project_file(&mut engine, "parent.rb", "class Parent; end");

    let child = FullyQualifiedName::namespace(vec![RubyConstant::new("Child").unwrap()]);
    let parent = FullyQualifiedName::namespace(vec![RubyConstant::new("Parent").unwrap()]);
    let child_singleton = child.to_singleton_namespace().unwrap();
    let parent_singleton = parent.to_singleton_namespace().unwrap();
    engine.replace_facts(
        child_file,
        FileFacts {
            graph_nodes: vec![
                GraphNodeFact::new(
                    child.clone(),
                    GraphNodeKind::Class,
                    TextRange::new(child_file, 0, 25),
                ),
                GraphNodeFact::new(
                    child_singleton.clone(),
                    GraphNodeKind::Class,
                    TextRange::new(child_file, 0, 25),
                ),
            ],
            unresolved_graph_edges: vec![UnresolvedGraphEdgeFact::new(
                child.clone(),
                vec![RubyConstant::new("Parent").unwrap()],
                true,
                child,
                GraphEdgeKind::Superclass,
                TextRange::new(child_file, 14, 20),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    assert_eq!(engine.unresolved_graph_edges().len(), 1);

    engine.replace_facts(
        parent_file,
        FileFacts {
            graph_nodes: vec![
                GraphNodeFact::new(
                    parent.clone(),
                    GraphNodeKind::Class,
                    TextRange::new(parent_file, 0, 17),
                ),
                GraphNodeFact::new(
                    parent_singleton.clone(),
                    GraphNodeKind::Class,
                    TextRange::new(parent_file, 0, 17),
                ),
            ],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert!(engine.unresolved_graph_edges().is_empty());
    assert!(engine
        .graph_edges_from(&child_singleton)
        .iter()
        .any(|edge| { edge.kind == GraphEdgeKind::Superclass && edge.target == parent_singleton }));
}

#[test]
fn explicit_superclass_outranks_reopened_implicit_object_fact() {
    let mut engine = AnalysisEngine::new();
    let file_id = register_project_file(&mut engine, "child.rb", "class Child; end");
    let child = FullyQualifiedName::namespace(vec![RubyConstant::new("Child").unwrap()]);
    let object = FullyQualifiedName::namespace(vec![RubyConstant::new("Object").unwrap()]);
    let parent = FullyQualifiedName::namespace(vec![RubyConstant::new("Parent").unwrap()]);
    let range = TextRange::new(file_id, 0, 16);
    engine.replace_facts(
        file_id,
        FileFacts {
            graph_nodes: vec![GraphNodeFact::new(
                child.clone(),
                GraphNodeKind::Class,
                range,
            )],
            graph_edges: vec![
                GraphEdgeFact::new(child.clone(), object, GraphEdgeKind::Superclass, range)
                    .with_provenance(GraphEdgeProvenance::ImplicitObject),
                GraphEdgeFact::new(
                    child.clone(),
                    parent.clone(),
                    GraphEdgeKind::Superclass,
                    TextRange::new(file_id, 6, 12),
                ),
            ],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert!(!engine.superclass_is_ambiguous(&child));
    assert_eq!(
        engine
            .proven_superclass_edge(&child)
            .map(|edge| edge.target),
        Some(parent)
    );
}

#[test]
fn conditional_delayed_superclasses_make_instance_and_singleton_ancestry_unknown() {
    let mut engine = AnalysisEngine::new();
    let class_file = register_project_file(
        &mut engine,
        "pending.rb",
        "class Pending < OptionalError; end\nclass Pending < StandardError; end",
    );
    let target_file = register_project_file(
        &mut engine,
        "targets.rb",
        "class OptionalError; end\nclass StandardError; end",
    );
    let pending = FullyQualifiedName::namespace(vec![RubyConstant::new("Pending").unwrap()]);
    let optional = FullyQualifiedName::namespace(vec![RubyConstant::new("OptionalError").unwrap()]);
    let standard = FullyQualifiedName::namespace(vec![RubyConstant::new("StandardError").unwrap()]);
    let pending_singleton = pending.to_singleton_namespace().unwrap();
    let optional_singleton = optional.to_singleton_namespace().unwrap();
    let standard_singleton = standard.to_singleton_namespace().unwrap();
    engine.replace_facts(
        class_file,
        FileFacts {
            graph_nodes: vec![
                GraphNodeFact::new(
                    pending.clone(),
                    GraphNodeKind::Class,
                    TextRange::new(class_file, 0, 34),
                ),
                GraphNodeFact::new(
                    pending_singleton.clone(),
                    GraphNodeKind::Class,
                    TextRange::new(class_file, 0, 34),
                ),
            ],
            graph_edges: vec![
                GraphEdgeFact::new(
                    pending.clone(),
                    standard.clone(),
                    GraphEdgeKind::Superclass,
                    TextRange::new(class_file, 51, 64),
                ),
                GraphEdgeFact::new(
                    pending_singleton.clone(),
                    standard_singleton,
                    GraphEdgeKind::Superclass,
                    TextRange::new(class_file, 51, 64),
                ),
            ],
            unresolved_graph_edges: vec![UnresolvedGraphEdgeFact::new(
                pending.clone(),
                vec![RubyConstant::new("OptionalError").unwrap()],
                true,
                pending.clone(),
                GraphEdgeKind::Superclass,
                TextRange::new(class_file, 16, 29),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    assert!(engine.proven_superclass_edge(&pending).is_none());

    engine.replace_facts(
        target_file,
        FileFacts {
            graph_nodes: vec![
                GraphNodeFact::new(
                    optional.clone(),
                    GraphNodeKind::Class,
                    TextRange::new(target_file, 0, 24),
                ),
                GraphNodeFact::new(
                    optional_singleton,
                    GraphNodeKind::Class,
                    TextRange::new(target_file, 0, 24),
                ),
                GraphNodeFact::new(
                    standard,
                    GraphNodeKind::Class,
                    TextRange::new(target_file, 25, 49),
                ),
            ],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert!(engine.superclass_is_ambiguous(&pending));
    assert!(engine.superclass_is_ambiguous(&pending_singleton));
    assert!(engine.proven_superclass_edge(&pending).is_none());
    assert!(engine.proven_superclass_edge(&pending_singleton).is_none());
}

#[test]
fn later_include_wins_mro_when_facts_are_inserted_out_of_range_order() {
    let mut engine = AnalysisEngine::new();
    let file_id = register_project_file(
        &mut engine,
        "lib/child.rb",
        "module Early; end\nmodule Late; end\nclass Child\n  include Early\n  include Late\nend\n",
    );
    let early = FullyQualifiedName::namespace(vec![RubyConstant::new("Early").unwrap()]);
    let late = FullyQualifiedName::namespace(vec![RubyConstant::new("Late").unwrap()]);
    let child = FullyQualifiedName::namespace(vec![RubyConstant::new("Child").unwrap()]);
    engine.replace_facts(
        file_id,
        FileFacts {
            graph_nodes: vec![
                GraphNodeFact::new(
                    early.clone(),
                    GraphNodeKind::Module,
                    TextRange::new(file_id, 0, 16),
                ),
                GraphNodeFact::new(
                    late.clone(),
                    GraphNodeKind::Module,
                    TextRange::new(file_id, 17, 32),
                ),
                GraphNodeFact::new(
                    child.clone(),
                    GraphNodeKind::Class,
                    TextRange::new(file_id, 33, 80),
                ),
            ],
            graph_edges: vec![
                GraphEdgeFact::new(
                    child.clone(),
                    late.clone(),
                    GraphEdgeKind::Include,
                    TextRange::new(file_id, 60, 72),
                ),
                GraphEdgeFact::new(
                    child.clone(),
                    early.clone(),
                    GraphEdgeKind::Include,
                    TextRange::new(file_id, 46, 58),
                ),
            ],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    let chain = method_lookup_chain(&engine, &child);
    let child_idx = chain.iter().position(|fqn| fqn == &child).expect("Child");
    let late_idx = chain.iter().position(|fqn| fqn == &late).expect("Late");
    let early_idx = chain.iter().position(|fqn| fqn == &early).expect("Early");
    assert!(
        child_idx < late_idx && late_idx < early_idx,
        "later include must precede earlier include even when the later edge is inserted first; chain={chain:?}"
    );
}

#[test]
fn edge_only_graph_entries_do_not_promote_missing_namespaces() {
    let mut engine = AnalysisEngine::new();
    let file_id = register_project_file(&mut engine, "lib/edge.rb", "class Parent\nend\n");
    let parent = FullyQualifiedName::namespace(vec![RubyConstant::new("Parent").unwrap()]);
    let missing = FullyQualifiedName::namespace(vec![RubyConstant::new("Missing").unwrap()]);
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
                parent.clone(),
                GraphEdgeKind::Superclass,
                TextRange::new(file_id, 0, 16),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert_eq!(
        method_lookup_chain(&engine, &missing),
        vec![missing.clone()],
        "an edge-only namespace has no proven Object/Kernel ancestry and must not gain top-level method lookup"
    );
    assert!(engine.has_graph_node(&parent));
    assert!(
        !engine.has_graph_node(&missing),
        "edge-only interned endpoints must not count as declared namespaces"
    );
    assert_eq!(
        engine.latest_graph_node_kind(&parent),
        Some(GraphNodeKind::Class)
    );
    assert_eq!(engine.latest_graph_node_kind(&missing), None);
    assert_eq!(
        AnalysisQuery::new(&engine).namespace_node_kind(&parent),
        Some(GraphNodeKind::Class)
    );
    assert_eq!(
        AnalysisQuery::new(&engine).namespace_node_kind(&missing),
        None
    );
}

#[test]
fn non_core_object_monkeypatch_requires_load_proof_for_unrelated_receivers() {
    let mut engine = AnalysisEngine::new();
    let project_file = register_project_file(
        &mut engine,
        "spec/mock_support.rb",
        "class Object; def stub(name, value); end; end\nclass Client; end\n",
    );
    let stub_file = engine.register_file(SourceFileInput {
        path: "core/object.rb".into(),
        content: "class Object; def to_s; end; end".into(),
        kind: SourceKind::Stub,
    });
    let object = FullyQualifiedName::namespace(vec![RubyConstant::new("Object").unwrap()]);
    let object_mixin =
        FullyQualifiedName::namespace(vec![RubyConstant::new("ObjectMixin").unwrap()]);
    let direct_mixin =
        FullyQualifiedName::namespace(vec![RubyConstant::new("DirectMixin").unwrap()]);
    let client_instance = FullyQualifiedName::namespace(vec![RubyConstant::new("Client").unwrap()]);
    let client = FullyQualifiedName::namespace_with_kind(
        vec![RubyConstant::new("Client").unwrap()],
        NamespaceKind::Singleton,
    );
    let project_range = TextRange::new(project_file, 0, 45);
    let stub_range = TextRange::new(stub_file, 0, 34);
    engine.replace_facts(
        project_file,
        FileFacts {
            graph_nodes: vec![
                GraphNodeFact::new(client_instance.clone(), GraphNodeKind::Class, project_range),
                GraphNodeFact::new(client.clone(), GraphNodeKind::Class, project_range),
                GraphNodeFact::new(object_mixin.clone(), GraphNodeKind::Module, project_range),
                GraphNodeFact::new(direct_mixin.clone(), GraphNodeKind::Module, project_range),
            ],
            graph_edges: vec![
                GraphEdgeFact::new(
                    object.clone(),
                    object_mixin.clone(),
                    GraphEdgeKind::Include,
                    project_range,
                ),
                GraphEdgeFact::new(
                    client_instance.clone(),
                    direct_mixin.clone(),
                    GraphEdgeKind::Include,
                    project_range,
                ),
                GraphEdgeFact::new(
                    client_instance.clone(),
                    object.clone(),
                    GraphEdgeKind::Superclass,
                    project_range,
                )
                .with_provenance(GraphEdgeProvenance::ImplicitObject),
            ],
            methods: vec![
                MethodFact::with_params(
                    FullyQualifiedName::method(
                        object.namespace_parts(),
                        RubyMethod::new("stub").unwrap(),
                    ),
                    object.clone(),
                    project_range,
                    vec!["name".to_string(), "value".to_string()],
                ),
                MethodFact::new(
                    FullyQualifiedName::method(
                        object_mixin.namespace_parts(),
                        RubyMethod::new("object_mixin_method").unwrap(),
                    ),
                    object_mixin,
                    project_range,
                ),
                MethodFact::new(
                    FullyQualifiedName::method(
                        direct_mixin.namespace_parts(),
                        RubyMethod::new("direct_mixin_method").unwrap(),
                    ),
                    direct_mixin,
                    project_range,
                ),
            ],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    engine.replace_facts(
        stub_file,
        FileFacts {
            graph_nodes: vec![GraphNodeFact::new(
                object.clone(),
                GraphNodeKind::Class,
                stub_range,
            )],
            methods: vec![MethodFact::new(
                FullyQualifiedName::method(
                    object.namespace_parts(),
                    RubyMethod::new("to_s").unwrap(),
                ),
                object,
                stub_range,
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert!(matches!(
        engine
            .query()
            .resolve_method_reference(&client, &RubyMethod::new("stub").unwrap()),
        crate::engine::resolution::MethodLookupResult::Ambiguous { .. }
    ));
    assert!(matches!(
        engine
            .query()
            .resolve_method_reference(&client, &RubyMethod::new("to_s").unwrap()),
        crate::engine::resolution::MethodLookupResult::Unique(_)
    ));
    match engine.query().resolve_method_reference(
        &client_instance,
        &RubyMethod::new("object_mixin_method").unwrap(),
    ) {
        crate::engine::resolution::MethodLookupResult::Ambiguous { .. } => {}
        crate::engine::resolution::MethodLookupResult::Unique(fact) => unreachable_invariant!(
            what = "an Object-only project mixin resolved concretely for unrelated Client through `{}`",
            why = "workspace indexing does not prove that monkeypatch was loaded in Client's runtime",
            fix = "stop non-core ancestry proof at universal open roots",
            fact.owner,
        ),
        crate::engine::resolution::MethodLookupResult::Missing => unreachable_invariant!(
            what = "an unproven Object-only project mixin became definitely missing",
            why = "the method may exist if the monkeypatch loads at runtime",
            fix = "keep the lookup ambiguous Unknown; emit no missing-method diagnostic",
        ),
    }
    assert!(matches!(
        engine.query().resolve_method_reference(
            &client_instance,
            &RubyMethod::new("direct_mixin_method").unwrap()
        ),
        crate::engine::resolution::MethodLookupResult::Unique(_)
    ));
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
        crate::engine::MethodLookupResult::Ambiguous { .. }
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
