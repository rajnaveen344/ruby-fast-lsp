//! Shared dependency products, single-flight provenance, and JRuby companion overlap.

use super::*;
use crate::loader::cache::persistent::PersistentProductStat;
use crate::utils::single_flight::SingleFlightStat;

#[test]
fn test_gem_indexer_creation() {
    let indexer = create_test_indexer();
    assert_eq!(indexer.gem_count(), 0);
    assert!(indexer.get_required_gems().is_empty());
}

fn shared_dependency_indexer(project_root: &Path, gem_root: &Path) -> IndexerGem {
    let mut indexer = IndexerGem::new(Some(project_root.to_path_buf()));
    indexer.set_required_gems(HashSet::from(["shared_widget".to_string()]));
    indexer.discovered_gems.insert(
        "shared_widget".to_string(),
        vec![GemInfo {
            name: "shared_widget".to_string(),
            version: "1.0.0".to_string(),
            platform: "ruby".to_string(),
            locked_version: "1.0.0".to_string(),
            source: GemSource::BundlerInstalled,
            path: gem_root.to_path_buf(),
            lib_paths: vec![gem_root.join("lib")],
            dependencies: Vec::new(),
            is_default: false,
        }],
    );
    indexer.set_file_processor(FileProcessor::new());
    indexer.set_dependency_seed_engine(AnalysisEngine::new());
    indexer
}

fn assert_shared_dependency_semantics(engine: &AnalysisEngine, expected_path: &Path) {
    let owner = FullyQualifiedName::namespace(vec![RubyConstant::new("SharedWidget").unwrap()]);
    let method = RubyMethod::new("label").unwrap();
    let query = AnalysisQuery::new(engine);
    let definitions =
        query.constant_definition_ranges(&[RubyConstant::new("SharedWidget").unwrap()], &[]);
    assert_eq!(definitions.len(), 1);
    assert_eq!(
        engine.file(definitions[0].file_id).unwrap().path,
        expected_path
    );
    assert_eq!(
        engine.file(definitions[0].file_id).unwrap().kind,
        SourceKind::Gem
    );

    let signatures = query.resolve_method_signature_facts(&owner, &method);
    assert_eq!(signatures.len(), 1);
    assert_eq!(signatures[0].params, ["prefix"]);
    assert_eq!(signatures[0].return_type_label.as_deref(), Some("String"));
    let value = FullyQualifiedName::constant(vec![
        RubyConstant::new("SharedWidget").unwrap(),
        RubyConstant::new("DEFAULT_LABEL").unwrap(),
    ]);
    assert_eq!(query.constant_value_type(&value), Some(RubyType::string()));
}

#[test]
fn ordinary_gem_products_ignore_unrelated_jruby_classpaths_but_java_gems_do_not() {
    let fixture = TempDir::new().unwrap();
    let first_project = fixture.path().join("first");
    let second_project = fixture.path().join("second");
    let first_gem = first_project.join("bundle/shared_widget-1.0.0");
    let second_gem = second_project.join("bundle/shared_widget-1.0.0");
    fs::create_dir_all(first_gem.join("lib")).unwrap();
    fs::create_dir_all(second_gem.join("lib")).unwrap();
    let ordinary = "module SharedWidget\nend\n";
    fs::write(first_gem.join("lib/shared_widget.rb"), ordinary).unwrap();
    fs::write(second_gem.join("lib/shared_widget.rb"), ordinary).unwrap();

    let mut first = shared_dependency_indexer(&first_project, &first_gem);
    first.set_runtime_provider_fingerprint(Some("classpath-a".to_string()));
    let mut second = shared_dependency_indexer(&second_project, &second_gem);
    second.set_runtime_provider_fingerprint(Some("classpath-b".to_string()));
    let seed = AnalysisEngine::new().semantic_context_fingerprint();
    let first_key = first.required_gem_manifests(seed).unwrap()[0].key().clone();
    let second_key = second.required_gem_manifests(seed).unwrap()[0]
        .key()
        .clone();
    assert_eq!(
        first_key, second_key,
        "ordinary Ruby gem facts must not be invalidated by an unrelated project classpath"
    );

    fs::write(
        first_gem.join("lib/shared_widget.rb"),
        "java_import 'fixtures.RichFixture'\n",
    )
    .unwrap();
    fs::write(
        second_gem.join("lib/shared_widget.rb"),
        "java_import 'fixtures.RichFixture'\n",
    )
    .unwrap();
    let first_key = first.required_gem_manifests(seed).unwrap()[0].key().clone();
    let second_key = second.required_gem_manifests(seed).unwrap()[0]
        .key()
        .clone();
    assert_ne!(
        first_key, second_key,
        "a gem using JRuby interop must retain the exact owning classpath identity"
    );
}

#[tokio::test]
async fn concurrent_isolated_projects_share_one_flight_with_exact_provenance() {
    let fixture = TempDir::new().unwrap();
    let first_project = fixture.path().join("first");
    let second_project = fixture.path().join("second");
    let third_project = fixture.path().join("third");
    let first_gem = first_project.join("bundle/shared_widget-1.0.0");
    let second_gem = second_project.join("bundle/shared_widget-1.0.0");
    let third_gem = third_project.join("bundle/shared_widget-1.0.0");
    let content = concat!(
        "class SharedWidget\n",
        "  DEFAULT_LABEL = \"label\"\n",
        "  # @param prefix [String]\n",
        "  # @return [String]\n",
        "  def label(prefix)\n",
        "    \"label\"\n",
        "  end\n",
        "end\n",
    );
    for (project, gem) in [
        (&first_project, &first_gem),
        (&second_project, &second_gem),
        (&third_project, &third_gem),
    ] {
        fs::create_dir_all(gem.join("lib")).unwrap();
        fs::write(project.join("Gemfile"), "gem 'shared_widget'\n").unwrap();
        fs::write(gem.join("lib/shared_widget.rb"), content).unwrap();
    }

    let first_indexer = shared_dependency_indexer(&first_project, &first_gem);
    let second_indexer = shared_dependency_indexer(&second_project, &second_gem);
    let server = RubyLanguageServer::with_user_cache_root(fixture.path().join("user-cache"))
        .expect("construct isolated cache server");
    let first_engine = Arc::new(parking_lot::RwLock::new(AnalysisEngine::new()));
    let second_engine = Arc::new(parking_lot::RwLock::new(AnalysisEngine::new()));

    let ctx = server.load_context_for_project(fixture.path());
    let (first_result, second_result) = tokio::join!(
        first_indexer.index_required_gems_with_shared_product(&ctx, first_engine.clone()),
        second_indexer.index_required_gems_with_shared_product(&ctx, second_engine.clone()),
    );
    assert_eq!(first_result.unwrap().len(), 1);
    assert_eq!(second_result.unwrap().len(), 1);
    let cache = server.products.gem_dependencies().snapshot();
    assert_eq!(cache.get(SingleFlightStat::Lookups), 2);
    assert_eq!(cache.get(SingleFlightStat::Producers), 1);
    assert_eq!(
        cache.get(SingleFlightStat::Hits) + cache.get(SingleFlightStat::JoinedFlights),
        1
    );
    assert_eq!(cache.get(SingleFlightStat::Entries), 0);

    let first_path = first_gem.join("lib/shared_widget.rb");
    let second_path = second_gem.join("lib/shared_widget.rb");
    assert_shared_dependency_semantics(&first_engine.read(), &first_path);
    assert_shared_dependency_semantics(&second_engine.read(), &second_path);
    let first_file_id = first_engine.read().file_id(&first_path).unwrap();
    let second_file_id = second_engine.read().file_id(&second_path).unwrap();
    assert!(first_engine.read().file_id(&second_path).is_none());
    assert!(second_engine.read().file_id(&first_path).is_none());
    assert_eq!(
        first_engine.read().file(first_file_id).unwrap().path,
        first_path
    );
    assert_eq!(
        second_engine.read().file(second_file_id).unwrap().path,
        second_path
    );

    first_engine.write().replace_facts(
        first_file_id,
        FileAnalysis::default(),
        ResolveMode::Immediate,
    );
    assert!(
        AnalysisQuery::new(&first_engine.read())
            .constant_definition_ranges(&[RubyConstant::new("SharedWidget").unwrap()], &[],)
            .is_empty(),
        "ordinary replacement must remove rebound facts from only the first consumer"
    );
    assert_shared_dependency_semantics(&second_engine.read(), &second_path);

    let third_indexer = shared_dependency_indexer(&third_project, &third_gem);
    let third_engine = Arc::new(parking_lot::RwLock::new(AnalysisEngine::new()));
    assert_eq!(
        third_indexer
            .index_required_gems_with_shared_product(
                &server.load_context_for_project(fixture.path()),
                third_engine.clone()
            )
            .await
            .unwrap()
            .len(),
        1
    );
    let after_sequential_consumer = server.products.gem_dependencies().snapshot();
    assert_eq!(after_sequential_consumer.get(SingleFlightStat::Lookups), 3);
    assert_eq!(
        after_sequential_consumer.get(SingleFlightStat::Producers),
        2
    );
    assert_eq!(after_sequential_consumer.get(SingleFlightStat::Entries), 0);
    let persistent = server.products.persistent().gem_product_snapshot();
    assert_eq!(persistent.get(PersistentProductStat::Producers), 1);
    assert_eq!(persistent.get(PersistentProductStat::Publications), 1);
    assert_eq!(persistent.get(PersistentProductStat::Hits), 1);
    assert_shared_dependency_semantics(
        &third_engine.read(),
        &third_gem.join("lib/shared_widget.rb"),
    );
}

#[tokio::test(flavor = "current_thread")]
async fn cold_active_gem_product_overlaps_the_jruby_runtime_companion() {
    let fixture = TempDir::new().unwrap();
    let project_root = fixture.path().join("project");
    let gem_root = project_root.join("bundle/shared_widget-1.0.0");
    fs::create_dir_all(gem_root.join("lib")).unwrap();
    fs::write(project_root.join("Gemfile"), "gem 'shared_widget'\n").unwrap();
    fs::write(
        gem_root.join("lib/shared_widget.rb"),
        "class SharedWidget\nend\n",
    )
    .unwrap();

    let indexer = shared_dependency_indexer(&project_root, &gem_root);
    let mut server = RubyLanguageServer::with_user_cache_root(fixture.path().join("user-cache"))
        .expect("construct isolated cache server");
    server.indexing.set_resources(
        crate::loader::scheduling::resources::IndexingResourceGovernor::new(
            crate::loader::scheduling::resources::IndexingResourcePolicy::with_limits(
                6,
                2,
                512 * 1024 * 1024,
                2,
            ),
        ),
    );
    server
        .indexing
        .resources()
        .prioritize_active_project_with_navigation_pending(&project_root, true);
    let server = Arc::new(server);

    let (runtime_started_tx, runtime_started_rx) = tokio::sync::oneshot::channel();
    let (runtime_release_tx, runtime_release_rx) = std::sync::mpsc::channel();
    let runtime_server = server.clone();
    let runtime_root = project_root.clone();
    let runtime = tokio::spawn(async move {
        runtime_server
            .indexing
            .resources()
            .run_partitioned_parallel_with_resources(
                "simulated JRuby runtime companion",
                IndexingWorkSpec::new(
                    Some(runtime_root),
                    IndexingResourcePriority::Background,
                    1,
                    GEM_PRODUCT_TRANSIENT_MEMORY_BYTES,
                    1,
                )
                .as_project_parallel(),
                None,
                move || {
                    runtime_started_tx.send(()).unwrap();
                    runtime_release_rx.recv().unwrap();
                },
            )
            .await
            .unwrap();
    });
    runtime_started_rx.await.unwrap();

    let engine = Arc::new(parking_lot::RwLock::new(AnalysisEngine::new()));
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        indexer.index_required_gems_with_shared_product(
            &server.load_context_for_project(fixture.path()),
            engine,
        ),
    )
    .await;
    runtime_release_tx.send(()).unwrap();
    runtime.await.unwrap();

    assert!(
        result.is_ok(),
        "cold gem-product construction must use the active project's five-lane partition so \
             it can finish while the one-lane JRuby companion owns the persistent-cache \
             maintenance path"
    );
    assert_eq!(result.unwrap().unwrap().len(), 1);
    let resources = server.indexing.resources().snapshot();
    assert_eq!(resources.peak_active_cpu_lanes, 6);
    assert_eq!(
        resources.peak_active_transient_memory_bytes,
        512 * 1024 * 1024
    );
}
