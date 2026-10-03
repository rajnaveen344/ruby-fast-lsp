//! Namespace existence and constant reference resolution.

use super::*;

#[test]
fn namespace_target_exists_accepts_interned_instance_without_a_sibling_declaration() {
    let mut engine = Project::new();
    let file_id = register_project_file(&mut engine, "lib/user.rb", "class User; end\n");
    let user = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);
    let missing = FullyQualifiedName::namespace(vec![RubyConstant::new("Missing").unwrap()]);
    engine.update(
        file_id,
        FileAnalysis {
            graph_nodes: vec![GraphNodeFact::new(
                user.clone(),
                GraphNodeKind::Class,
                TextRange::new(file_id, 0, 16),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert!(namespace_target_exists(&engine, &user));
    assert!(
        !namespace_target_exists(&engine, &missing),
        "an interned-looking name without a graph node or constant fact must not exist"
    );
}

#[test]
fn namespace_target_exists_accepts_singleton_when_instance_is_absent() {
    let mut engine = Project::new();
    let file_id = register_project_file(&mut engine, "lib/eigen.rb", "class << User; end\n");
    let instance = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);
    let singleton =
        FullyQualifiedName::singleton_namespace(vec![RubyConstant::new("User").unwrap()]);
    engine.update(
        file_id,
        FileAnalysis {
            graph_nodes: vec![GraphNodeFact::new(
                singleton.clone(),
                GraphNodeKind::Class,
                TextRange::new(file_id, 0, 18),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert!(namespace_target_exists(&engine, &singleton));
    assert!(
        namespace_target_exists(&engine, &instance),
        "method lookup on the instance identity must see a declared singleton of the same parts"
    );
}

#[test]
fn namespace_target_exists_accepts_a_value_constant_without_a_namespace_node() {
    let mut engine = Project::new();
    let file_id = register_project_file(&mut engine, "lib/status.rb", "STATUS = 1\n");
    let constant = FullyQualifiedName::constant(vec![RubyConstant::new("STATUS").unwrap()]);
    let as_namespace = FullyQualifiedName::namespace(vec![RubyConstant::new("STATUS").unwrap()]);
    engine.update(
        file_id,
        FileAnalysis {
            symbols: vec![SymbolFact::new(
                constant.clone(),
                SymbolKind::Constant,
                TextRange::new(file_id, 0, 6),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert!(
        namespace_target_exists(&engine, &as_namespace),
        "a value constant of the same parts must still be a method-lookup target"
    );
    assert!(namespace_target_exists(&engine, &constant));
}

#[test]
fn constant_reference_resolves_a_value_constant_without_a_namespace_node() {
    let mut engine = Project::new();
    let def_file = register_project_file(&mut engine, "lib/status.rb", "STATUS = 1\n");
    let ref_file = register_project_file(&mut engine, "lib/use_status.rb", "STATUS\n");
    let status = FullyQualifiedName::constant(vec![RubyConstant::new("STATUS").unwrap()]);
    engine.update(
        def_file,
        FileAnalysis {
            symbols: vec![SymbolFact::new(
                status.clone(),
                SymbolKind::Constant,
                TextRange::new(def_file, 0, 6),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    engine.update(
        ref_file,
        FileAnalysis {
            reference_candidates: vec![ReferenceCandidate::constant(
                TextRange::new(ref_file, 0, 6),
                status.namespace_parts(),
                Vec::new(),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert_eq!(engine.view().reference_facts_for(&status).len(), 1);
    assert!(engine
        .view()
        .diagnostic_facts_in_file(ref_file)
        .iter()
        .all(|fact| fact.code != "unresolved-constant"));
}

#[test]
fn constant_reference_prefers_a_nested_class_over_an_outer_value_constant() {
    let mut engine = Project::new();
    let def_file = register_project_file(
        &mut engine,
        "lib/nested.rb",
        "class Outer\n  C = 1\n  class Inner\n    class C; end\n  end\nend\n",
    );
    let ref_file = register_project_file(&mut engine, "lib/use_nested.rb", "C\n");
    let outer = FullyQualifiedName::namespace(vec![RubyConstant::new("Outer").unwrap()]);
    let inner = FullyQualifiedName::namespace(vec![
        RubyConstant::new("Outer").unwrap(),
        RubyConstant::new("Inner").unwrap(),
    ]);
    let nested_class = FullyQualifiedName::namespace(vec![
        RubyConstant::new("Outer").unwrap(),
        RubyConstant::new("Inner").unwrap(),
        RubyConstant::new("C").unwrap(),
    ]);
    let outer_constant = FullyQualifiedName::constant(vec![
        RubyConstant::new("Outer").unwrap(),
        RubyConstant::new("C").unwrap(),
    ]);
    engine.update(
        def_file,
        FileAnalysis {
            symbols: vec![
                SymbolFact::new(
                    outer.clone(),
                    SymbolKind::Class,
                    TextRange::new(def_file, 6, 11),
                ),
                SymbolFact::new(
                    outer_constant.clone(),
                    SymbolKind::Constant,
                    TextRange::new(def_file, 14, 15),
                ),
                SymbolFact::new(
                    inner.clone(),
                    SymbolKind::Class,
                    TextRange::new(def_file, 28, 33),
                ),
                SymbolFact::new(
                    nested_class.clone(),
                    SymbolKind::Class,
                    TextRange::new(def_file, 44, 45),
                ),
            ],
            graph_nodes: vec![
                GraphNodeFact::new(outer, GraphNodeKind::Class, TextRange::new(def_file, 0, 64)),
                GraphNodeFact::new(
                    inner.clone(),
                    GraphNodeKind::Class,
                    TextRange::new(def_file, 22, 58),
                ),
                GraphNodeFact::new(
                    nested_class.clone(),
                    GraphNodeKind::Class,
                    TextRange::new(def_file, 38, 52),
                ),
            ],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    engine.update(
        ref_file,
        FileAnalysis {
            reference_candidates: vec![ReferenceCandidate::constant(
                TextRange::new(ref_file, 0, 1),
                vec![RubyConstant::new("C").unwrap()],
                inner.namespace_parts(),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert_eq!(engine.view().reference_facts_for(&nested_class).len(), 1);
    assert!(
        engine
            .view()
            .reference_facts_for(&outer_constant)
            .is_empty(),
        "lexical Inner::C must win over Outer::C"
    );
}

#[test]
fn constant_reference_walks_out_to_an_outer_value_constant() {
    let mut engine = Project::new();
    let def_file = register_project_file(
        &mut engine,
        "lib/outer.rb",
        "class Outer\n  C = 1\n  class Inner; end\nend\n",
    );
    let ref_file = register_project_file(&mut engine, "lib/use_outer.rb", "C\n");
    let outer = FullyQualifiedName::namespace(vec![RubyConstant::new("Outer").unwrap()]);
    let inner = FullyQualifiedName::namespace(vec![
        RubyConstant::new("Outer").unwrap(),
        RubyConstant::new("Inner").unwrap(),
    ]);
    let outer_constant = FullyQualifiedName::constant(vec![
        RubyConstant::new("Outer").unwrap(),
        RubyConstant::new("C").unwrap(),
    ]);
    engine.update(
        def_file,
        FileAnalysis {
            symbols: vec![
                SymbolFact::new(
                    outer.clone(),
                    SymbolKind::Class,
                    TextRange::new(def_file, 6, 11),
                ),
                SymbolFact::new(
                    outer_constant.clone(),
                    SymbolKind::Constant,
                    TextRange::new(def_file, 14, 15),
                ),
                SymbolFact::new(
                    inner.clone(),
                    SymbolKind::Class,
                    TextRange::new(def_file, 28, 33),
                ),
            ],
            graph_nodes: vec![
                GraphNodeFact::new(outer, GraphNodeKind::Class, TextRange::new(def_file, 0, 44)),
                GraphNodeFact::new(
                    inner.clone(),
                    GraphNodeKind::Class,
                    TextRange::new(def_file, 22, 38),
                ),
            ],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    engine.update(
        ref_file,
        FileAnalysis {
            reference_candidates: vec![ReferenceCandidate::constant(
                TextRange::new(ref_file, 0, 1),
                vec![RubyConstant::new("C").unwrap()],
                inner.namespace_parts(),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    assert_eq!(engine.view().reference_facts_for(&outer_constant).len(), 1);
}

#[test]
fn method_facts_alone_do_not_prove_a_missing_method_on_their_owner() {
    let mut engine = Project::new();
    let stub_file = engine.register_file(SourceFileInput {
        path: PathBuf::from("/stubs/seeded.rb"),
        content: String::new(),
        kind: SourceKind::Stub,
    });
    let ref_file = register_project_file(&mut engine, "app/use_seeded.rb", "Seeded.run");
    let seeded = vec![RubyConstant::new("Seeded").unwrap()];
    let run = RubyMethod::new("run").unwrap();
    engine.update(
        stub_file,
        FileAnalysis {
            methods: vec![MethodFact::new(
                FullyQualifiedName::method(seeded.clone(), run),
                FullyQualifiedName::singleton_namespace(seeded.clone()),
                TextRange::new(stub_file, 0, 0),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );
    engine.update(
        ref_file,
        FileAnalysis {
            reference_candidates: vec![explicit_method_call_candidate(
                TextRange::new(ref_file, 7, 10),
                TextRange::new(ref_file, 0, 10),
                seeded,
                NamespaceKind::Singleton,
                run,
                None,
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    let diagnostics = engine.view().diagnostic_facts_in_file(ref_file);
    assert!(
        diagnostics
            .iter()
            .all(|fact| fact.code != "unresolved-method"),
        "a receiver namespace without a declaration leaves method absence unknown: {diagnostics:?}"
    );
}
