//! Method return, callee, signature, and lookup chain caches.

use super::*;

#[test]
fn cached_method_return_queries_invalidate_after_semantic_replacement() {
    let mut engine = Project::new();
    let file_id = register_project_file(
        &mut engine,
        "lib/widget.rb",
        "class Widget\n  def value = 'first'\nend\n",
    );
    let owner = FullyQualifiedName::namespace(vec![RubyConstant::new("Widget").unwrap()]);
    let method_name = RubyMethod::new("value").unwrap();
    let method = FullyQualifiedName::method(owner.namespace_parts(), method_name);
    let range = TextRange::new(file_id, 15, 34);
    let facts = |ruby_type| FileAnalysis {
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
    engine.update(file_id, facts(RubyType::string()), ResolveMode::Immediate);
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
    assert_eq!(cache.valid_entry_counts_for_test(), (1, 1, 0));

    engine.update(file_id, facts(RubyType::integer()), ResolveMode::Immediate);
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
        (1, 1, 0),
        "replacement must discard both cache families before inserting new-revision entries"
    );
}

#[test]
fn thread_local_method_return_cache_reuses_across_per_source_caches() {
    let mut engine = Project::new();
    let file_id = register_project_file(
        &mut engine,
        "lib/widget.rb",
        "class Widget\n  def value = 'first'\nend\n",
    );
    let owner = FullyQualifiedName::namespace(vec![RubyConstant::new("Widget").unwrap()]);
    let method_name = RubyMethod::new("value").unwrap();
    let method = FullyQualifiedName::method(owner.namespace_parts(), method_name);
    let range = TextRange::new(file_id, 15, 34);
    engine.update(
        file_id,
        FileAnalysis {
            graph_nodes: vec![GraphNodeFact::new(
                owner.clone(),
                GraphNodeKind::Class,
                TextRange::new(file_id, 0, 41),
            )],
            methods: vec![MethodFact::new(method.clone(), owner.clone(), range)],
            types: vec![TypeFact::new(
                TypeSubject::MethodReturn(method),
                RubyType::string(),
                range,
                TypeProvenance::Inferred,
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    let first_cache = AnalysisQueryCache::default();
    assert_eq!(
        engine
            .query()
            .method_return_type_for_receiver_cached(&owner, &method_name, &first_cache),
        Some(RubyType::string())
    );
    assert_eq!(first_cache.valid_entry_counts_for_test(), (1, 0, 0));

    let second_cache = AnalysisQueryCache::default();
    assert_eq!(
        engine
            .query()
            .method_return_type_for_receiver_cached(&owner, &method_name, &second_cache),
        Some(RubyType::string()),
        "identical receiver/method lookups on one engine identity must reuse the thread-local return"
    );
    assert_eq!(
        second_cache.valid_entry_counts_for_test(),
        (0, 0, 0),
        "thread-local reuse must not populate a second per-source cache"
    );
}

#[test]
fn thread_local_method_return_cache_does_not_reuse_a_different_method() {
    let mut engine = Project::new();
    let file_id = register_project_file(
        &mut engine,
        "lib/widget.rb",
        "class Widget\n  def value = 'first'\n  def count = 1\nend\n",
    );
    let owner = FullyQualifiedName::namespace(vec![RubyConstant::new("Widget").unwrap()]);
    let value = RubyMethod::new("value").unwrap();
    let count = RubyMethod::new("count").unwrap();
    let value_fqn = FullyQualifiedName::method(owner.namespace_parts(), value);
    let count_fqn = FullyQualifiedName::method(owner.namespace_parts(), count);
    let value_range = TextRange::new(file_id, 15, 34);
    let count_range = TextRange::new(file_id, 37, 50);
    engine.update(
        file_id,
        FileAnalysis {
            graph_nodes: vec![GraphNodeFact::new(
                owner.clone(),
                GraphNodeKind::Class,
                TextRange::new(file_id, 0, 56),
            )],
            methods: vec![
                MethodFact::new(value_fqn.clone(), owner.clone(), value_range),
                MethodFact::new(count_fqn.clone(), owner.clone(), count_range),
            ],
            types: vec![
                TypeFact::new(
                    TypeSubject::MethodReturn(value_fqn),
                    RubyType::string(),
                    value_range,
                    TypeProvenance::Inferred,
                ),
                TypeFact::new(
                    TypeSubject::MethodReturn(count_fqn),
                    RubyType::integer(),
                    count_range,
                    TypeProvenance::Inferred,
                ),
            ],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    let first_cache = AnalysisQueryCache::default();
    assert_eq!(
        engine
            .query()
            .method_return_type_for_receiver_cached(&owner, &value, &first_cache),
        Some(RubyType::string())
    );

    let second_cache = AnalysisQueryCache::default();
    assert_eq!(
        engine
            .query()
            .method_return_type_for_receiver_cached(&owner, &count, &second_cache),
        Some(RubyType::integer()),
        "a different method on the same owner must not reuse the thread-local Widget#value return"
    );
    assert_eq!(
        second_cache.valid_entry_counts_for_test(),
        (1, 0, 0),
        "a thread-local miss must still populate the per-source cache that computed the new method"
    );
}

#[test]
fn thread_local_method_return_cache_reuses_public_returns_across_callers() {
    let mut engine = Project::new();
    let file_id = register_project_file(
        &mut engine,
        "lib/widget.rb",
        "class Widget\n  def value = 'first'\nend\n",
    );
    let owner = FullyQualifiedName::namespace(vec![RubyConstant::new("Widget").unwrap()]);
    let controller = FullyQualifiedName::namespace(vec![RubyConstant::new("Controller").unwrap()]);
    let helper = FullyQualifiedName::namespace(vec![RubyConstant::new("Helper").unwrap()]);
    let method_name = RubyMethod::new("value").unwrap();
    let method = FullyQualifiedName::method(owner.namespace_parts(), method_name);
    let range = TextRange::new(file_id, 15, 34);
    engine.update(
        file_id,
        FileAnalysis {
            graph_nodes: vec![GraphNodeFact::new(
                owner.clone(),
                GraphNodeKind::Class,
                TextRange::new(file_id, 0, 41),
            )],
            methods: vec![MethodFact::new(method.clone(), owner.clone(), range)],
            types: vec![TypeFact::new(
                TypeSubject::MethodReturn(method),
                RubyType::string(),
                range,
                TypeProvenance::Inferred,
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    let first_cache = AnalysisQueryCache::default();
    assert_eq!(
        engine
            .query()
            .method_return_type_for_protected_receiver_cached(
                &owner,
                &method_name,
                &controller,
                &first_cache,
            ),
        Some(RubyType::string())
    );
    assert_eq!(first_cache.valid_entry_counts_for_test(), (1, 0, 0));

    let second_cache = AnalysisQueryCache::default();
    assert_eq!(
        engine
            .query()
            .method_return_type_for_protected_receiver_cached(
                &owner,
                &method_name,
                &helper,
                &second_cache,
            ),
        Some(RubyType::string()),
        "public Widget#value must reuse the thread-local return across callers"
    );
    assert_eq!(
        second_cache.valid_entry_counts_for_test(),
        (0, 0, 0),
        "caller-insensitive public returns must not populate a second per-source cache"
    );
}

#[test]
fn protected_override_does_not_reuse_a_parent_public_return() {
    let mut engine = Project::new();
    let file_id = register_project_file(
        &mut engine,
        "lib/vault.rb",
        "class Parent\n  def value = 'public'\nend\nclass Child < Parent\n  def value = 1\n  protected :value\nend\n",
    );
    let parent = FullyQualifiedName::namespace(vec![RubyConstant::new("Parent").unwrap()]);
    let child = FullyQualifiedName::namespace(vec![RubyConstant::new("Child").unwrap()]);
    let outsider = FullyQualifiedName::namespace(vec![RubyConstant::new("Outsider").unwrap()]);
    let method_name = RubyMethod::new("value").unwrap();
    let parent_method = FullyQualifiedName::method(parent.namespace_parts(), method_name);
    let child_method = FullyQualifiedName::method(child.namespace_parts(), method_name);
    let parent_range = TextRange::new(file_id, 15, 36);
    let child_range = TextRange::new(file_id, 70, 84);
    engine.update(
        file_id,
        FileAnalysis {
            graph_nodes: vec![
                GraphNodeFact::new(
                    parent.clone(),
                    GraphNodeKind::Class,
                    TextRange::new(file_id, 0, 40),
                ),
                GraphNodeFact::new(
                    child.clone(),
                    GraphNodeKind::Class,
                    TextRange::new(file_id, 41, 110),
                ),
            ],
            graph_edges: vec![GraphEdgeFact::new(
                child.clone(),
                parent.clone(),
                GraphEdgeKind::Superclass,
                TextRange::new(file_id, 41, 60),
            )],
            methods: vec![
                MethodFact::new(parent_method.clone(), parent.clone(), parent_range),
                MethodFact::new(child_method.clone(), child.clone(), child_range)
                    .with_visibility(crate::core::MethodVisibility::Protected),
            ],
            types: vec![
                TypeFact::new(
                    TypeSubject::MethodReturn(parent_method),
                    RubyType::string(),
                    parent_range,
                    TypeProvenance::Inferred,
                ),
                TypeFact::new(
                    TypeSubject::MethodReturn(child_method),
                    RubyType::integer(),
                    child_range,
                    TypeProvenance::Inferred,
                ),
            ],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    let cache = AnalysisQueryCache::default();
    assert_eq!(
        engine.query().method_return_type_for_protected_receiver_cached(
            &child,
            &method_name,
            &child,
            &cache,
        ),
        Some(RubyType::integer()),
        "a same-family explicit call must keep the closer protected Child#value return"
    );
    assert_eq!(
        engine
            .query()
            .method_return_type_for_protected_receiver_cached(
                &child,
                &method_name,
                &outsider,
                &cache,
            ),
        Some(RubyType::string()),
        "an outsider explicit call must keep the public Parent#value return"
    );
}

#[test]
fn resolved_method_callee_cache_is_bounded_per_source_collection() {
    let mut engine = Project::new();
    let file_id = register_project_file(&mut engine, "lib/widget.rb", "class Widget; end\n");
    let owner = FullyQualifiedName::namespace(vec![RubyConstant::new("Widget").unwrap()]);
    engine.update(
        file_id,
        FileAnalysis {
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
        (0, 256, 0),
        "one pathological source must not retain an unbounded method-callee catalog"
    );
}

#[test]
fn cached_method_signature_facts_invalidate_after_semantic_replacement() {
    let mut engine = Project::new();
    let file_id = register_project_file(
        &mut engine,
        "lib/widget.rb",
        "class Widget\n  def value = 'first'\nend\n",
    );
    let owner = FullyQualifiedName::namespace(vec![RubyConstant::new("Widget").unwrap()]);
    let method_name = RubyMethod::new("value").unwrap();
    let method = FullyQualifiedName::method(owner.namespace_parts(), method_name);
    let first_range = TextRange::new(file_id, 15, 34);
    let second_range = TextRange::new(file_id, 15, 35);
    let facts = |range| FileAnalysis {
        graph_nodes: vec![GraphNodeFact::new(
            owner.clone(),
            GraphNodeKind::Class,
            TextRange::new(file_id, 0, 41),
        )],
        methods: vec![MethodFact::new(method.clone(), owner.clone(), range)],
        ..Default::default()
    };
    engine.update(file_id, facts(first_range), ResolveMode::Immediate);
    let cache = AnalysisQueryCache::default();

    let uncached = engine
        .query()
        .resolve_method_signature_facts(&owner, &method_name);
    let cached = engine
        .query()
        .resolve_method_signature_facts_cached(&owner, &method_name, &cache);
    assert_eq!(uncached, cached);
    assert_eq!(uncached.len(), 1);
    assert_eq!(uncached[0].range, first_range);
    assert_eq!(cache.valid_entry_counts_for_test(), (0, 0, 1));

    engine.update(file_id, facts(second_range), ResolveMode::Immediate);
    let replaced =
        engine
            .query()
            .resolve_method_signature_facts_cached(&owner, &method_name, &cache);
    assert_eq!(replaced.len(), 1);
    assert_eq!(
        replaced[0].range, second_range,
        "a semantic replacement must invalidate cached signature facts"
    );
    assert_eq!(
        cache.valid_entry_counts_for_test(),
        (0, 0, 1),
        "replacement must discard signature facts before inserting the new-revision entry"
    );
}

#[test]
fn method_signature_fact_cache_is_bounded_per_source_collection() {
    let mut engine = Project::new();
    let file_id = register_project_file(&mut engine, "lib/widget.rb", "class Widget; end\n");
    let owner = FullyQualifiedName::namespace(vec![RubyConstant::new("Widget").unwrap()]);
    engine.update(
        file_id,
        FileAnalysis {
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
            .resolve_method_signature_facts_cached(&owner, &method, &cache)
            .is_empty());
    }

    assert_eq!(
        cache.valid_entry_counts_for_test(),
        (0, 0, 256),
        "one pathological source must not retain an unbounded signature-fact catalog"
    );
}

#[test]
fn method_lookup_chain_cache_is_engine_local_and_invalidates_on_replacement() {
    let mut engine = Project::new();
    let file_id = register_project_file(
        &mut engine,
        "lib/child.rb",
        "class Parent\n  def value = 'ok'\nend\nclass Child < Parent\nend\n",
    );
    let parent = FullyQualifiedName::namespace(vec![RubyConstant::new("Parent").unwrap()]);
    let child = FullyQualifiedName::namespace(vec![RubyConstant::new("Child").unwrap()]);
    let method = RubyMethod::new("value").unwrap();
    engine.update(
        file_id,
        FileAnalysis {
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

    engine.update(file_id, FileAnalysis::default(), ResolveMode::Immediate);
    assert_eq!(
        engine.valid_method_lookup_chain_cache_len_for_test(),
        0,
        "semantic replacement must invalidate cached lookup chains"
    );
}

#[test]
fn method_lookup_chain_reuses_construction_for_one_engine_identity() {
    let mut engine = Project::new();
    let file_id = register_project_file(
        &mut engine,
        "lib/child.rb",
        "class Parent\n  def value = 'ok'\nend\nclass Child < Parent\nend\n",
    );
    let parent = FullyQualifiedName::namespace(vec![RubyConstant::new("Parent").unwrap()]);
    let child = FullyQualifiedName::namespace(vec![RubyConstant::new("Child").unwrap()]);
    engine.update(
        file_id,
        FileAnalysis {
            graph_nodes: vec![
                GraphNodeFact::new(
                    parent.clone(),
                    GraphNodeKind::Class,
                    crate::core::TextRange::new(file_id, 0, 38),
                ),
                GraphNodeFact::new(
                    child.clone(),
                    GraphNodeKind::Class,
                    crate::core::TextRange::new(file_id, 39, 63),
                ),
            ],
            graph_edges: vec![GraphEdgeFact::new(
                child.clone(),
                parent.clone(),
                GraphEdgeKind::Superclass,
                crate::core::TextRange::new(file_id, 39, 59),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    let before = method_lookup_chain_uncached_construction_count();
    let first = method_lookup_chain(&engine, &child);
    let second = method_lookup_chain(&engine, &child);
    assert_eq!(first, second);
    assert!(first.contains(&parent));
    assert_eq!(
        method_lookup_chain_uncached_construction_count() - before,
        1,
        "repeated lookup of one owner must reuse the constructed MRO for the current engine identity"
    );

    engine.update(file_id, FileAnalysis::default(), ResolveMode::Immediate);
    let after_replace = method_lookup_chain_uncached_construction_count();
    let replaced = method_lookup_chain(&engine, &child);
    assert!(
        !replaced.contains(&parent),
        "replacement must drop stale ancestry instead of returning a cached pre-replacement chain"
    );
    assert_eq!(
        method_lookup_chain_uncached_construction_count() - after_replace,
        1,
        "semantic replacement must rebuild the lookup chain for the new engine identity"
    );
}

#[test]
fn method_reference_chain_cache_returns_the_stored_chain_by_borrow() {
    let mut engine = Project::new();
    let file_id = register_project_file(
        &mut engine,
        "lib/child.rb",
        "class Parent\n  def value = 'ok'\nend\nclass Child < Parent\nend\n",
    );
    let parent = FullyQualifiedName::namespace(vec![RubyConstant::new("Parent").unwrap()]);
    let child = FullyQualifiedName::namespace(vec![RubyConstant::new("Child").unwrap()]);
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
    let mut engine = Project::new();
    let file_id = register_project_file(
        &mut engine,
        "lib/child.rb",
        "class Parent\n  def first = 1\n  def second = 2\nend\nclass Child < Parent\nend\n",
    );
    let parent = FullyQualifiedName::namespace(vec![RubyConstant::new("Parent").unwrap()]);
    let child = FullyQualifiedName::namespace(vec![RubyConstant::new("Child").unwrap()]);
    let first = RubyMethod::new("first").unwrap();
    let second = RubyMethod::new("second").unwrap();
    engine.update(
        file_id,
        FileAnalysis {
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
    let first_resolution = engine
        .query()
        .resolve_method_reference_with_chain_cache(&child, &first, &mut cache);
    let cloned_resolution = first_resolution.clone();
    match (&first_resolution, &cloned_resolution) {
        (
            crate::engine::resolution::MethodLookupResult::Unique(first),
            crate::engine::resolution::MethodLookupResult::Unique(cloned),
        ) => assert!(
            std::sync::Arc::ptr_eq(first, cloned),
            "a cached method resolution clone must share its immutable MethodFact; deep cloning facts multiplies resolve-pass memory by reference count"
        ),
        (first, cloned) => unreachable_invariant!(
            what = "method lookup result changed shape while cloning (first={:?}, cloned={:?})",
            why = "clone must preserve an exact immutable resolution",
            fix = "clone every MethodLookupResult variant without semantic conversion",
            std::mem::discriminant(first),
            std::mem::discriminant(cloned),
        ),
    }

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
fn metaclass_fallback_cache_keeps_ambiguous_owner_receiver_independent() {
    let mut engine = Project::new();
    let file_id = register_project_file(
        &mut engine,
        "lib/metaclass.rb",
        "class Class; def any_instance; end; end\nclass Alpha; end\nclass Beta; end\n",
    );
    let class = FullyQualifiedName::namespace(vec![RubyConstant::new("Class").unwrap()]);
    let alpha = FullyQualifiedName::namespace_with_kind(
        vec![RubyConstant::new("Alpha").unwrap()],
        NamespaceKind::Singleton,
    );
    let beta = FullyQualifiedName::namespace_with_kind(
        vec![RubyConstant::new("Beta").unwrap()],
        NamespaceKind::Singleton,
    );
    let method = RubyMethod::new("any_instance").unwrap();
    let range = TextRange::new(file_id, 0, 75);
    engine.update(
        file_id,
        FileAnalysis {
            graph_nodes: vec![
                GraphNodeFact::new(class.clone(), GraphNodeKind::Class, range),
                GraphNodeFact::new(alpha.clone(), GraphNodeKind::Class, range),
                GraphNodeFact::new(beta.clone(), GraphNodeKind::Class, range),
            ],
            methods: vec![MethodFact::with_params(
                FullyQualifiedName::method(class.namespace_parts(), method),
                class.clone(),
                range,
                Vec::new(),
            )],
            ..Default::default()
        },
        ResolveMode::Immediate,
    );

    let resolve_pair = |first: &FullyQualifiedName, second: &FullyQualifiedName| {
        let mut cache = MethodLookupChainCache::new();
        [first, second].map(|receiver| {
            let result = engine
                .query()
                .resolve_method_reference_with_chain_cache(receiver, &method, &mut cache);
            match result {
                crate::engine::resolution::MethodLookupResult::Ambiguous { owner, method } => {
                    (owner, method)
                }
                crate::engine::resolution::MethodLookupResult::Unique(fact) => unreachable_invariant!(
                    what = "project-defined Class fallback resolved concretely for `{receiver}` through `{}`",
                    why = "indexing the defining file does not prove the runtime monkeypatch was loaded",
                    fix = "keep non-language metaclass fallbacks Unknown",
                    fact.owner,
                    receiver = receiver,
                ),
                crate::engine::resolution::MethodLookupResult::Missing => unreachable_invariant!(
                    what = "indexed Class fallback became definitely missing for `{receiver}`",
                    why = "the runtime load state is unknown",
                    fix = "retain an ambiguous canonical metaclass candidate",
                    receiver = receiver,
                ),
            }
        })
    };
    let forward = resolve_pair(&alpha, &beta);
    let reverse = resolve_pair(&beta, &alpha);
    assert_eq!(forward, [(class.clone(), method), (class.clone(), method)]);
    assert_eq!(reverse, [(class.clone(), method), (class, method)]);
}
