//! Compact JRuby replay of catalog-sensitive project files across provider handoff.

use super::*;
use crate::invariant::ExpectInvariant;

pub(super) fn jruby_provider(class_names: &[&str]) -> JrubyImportProvider {
    jruby_provider_with_superclasses(
        &class_names
            .iter()
            .map(|name| (*name, "java/lang/Object"))
            .collect::<Vec<_>>(),
    )
}

fn jruby_provider_with_superclasses(
    classes_with_superclasses: &[(&str, &str)],
) -> JrubyImportProvider {
    let classes = classes_with_superclasses
        .iter()
        .map(|(name, superclass)| ClassFile {
            minor_version: 0,
            major_version: 61,
            access_flags: 0x0021,
            name: (*name).into(),
            super_name: Some((*superclass).into()),
            interfaces: Box::default(),
            fields: Box::default(),
            methods: Box::default(),
            source_file: None,
            is_record: false,
        })
        .collect();
    JrubyImportProvider::new(Arc::new(ProjectJavaCatalog::from_test_classes(
        "fixture-classpath",
        PathBuf::from("/fixture/runtime.jar"),
        classes,
    )))
}

#[test]
fn providerless_project_pass_records_exact_compact_jruby_replay_candidates() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    let ordinary_path = root.join("ordinary.rb");
    let alias_path = root.join("alias.rb");
    let proxy_path = root.join("proxy.rb");
    std::fs::write(
        &ordinary_path,
        "module App\n  USER_NAME = user.profile.name\nend\n",
    )
    .unwrap();
    std::fs::write(
        &alias_path,
        "class Imported\n  java_alias :merged, :combine\nend\n",
    )
    .unwrap();
    std::fs::write(&proxy_path, "DEMO = com.example.Demo.new\n").unwrap();

    let server = Server::default();
    server.add_workspace(Url::from_directory_path(root).unwrap());
    let mut indexer = IndexerProject::new(
        root.to_path_buf(),
        FileProcessor::new(),
        IndexingConfig::default(),
    );
    indexer
        .collect_project_facts(&server.load_context_for_project(indexer.workspace_root()))
        .unwrap();

    assert_eq!(
        indexer.jruby_catalog_sensitive_files(&jruby_provider(&["com/example/Demo"])),
        vec![alias_path, proxy_path],
        "the providerless pass must retain bounded source hints so only actual JRuby \
             catalog consumers are replayed after the exact project catalog arrives"
    );
}

#[test]
fn exact_jruby_provider_replays_only_catalog_sensitive_project_files() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    let ordinary_path = root.join("ordinary.rb");
    let import_path = root.join("import.rb");
    std::fs::write(&ordinary_path, "class User\nend\n").unwrap();
    std::fs::write(
        &import_path,
        "java_import 'com.example.Demo'\nDEMO = Demo.new\n",
    )
    .unwrap();

    let server = Server::default();
    let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
    let mut indexer = IndexerProject::new(
        root.to_path_buf(),
        FileProcessor::new(),
        IndexingConfig::default(),
    );
    indexer
        .collect_project_facts(&server.load_context_for_project(indexer.workspace_root()))
        .unwrap();
    let imported =
        ruby_analysis::core::FullyQualifiedName::try_from("Demo").expect("valid fixture FQN");
    assert!(
        !workspace_state
            .handle()
            .test_read()
            .view()
            .symbol_facts()
            .any(|fact| fact.fqn == imported),
        "the providerless first pass must not invent a Java import alias"
    );

    let provider = Arc::new(
        jruby_provider(&["com/example/Demo"])
            .with_signature_cache_root(root.join("generated-signatures")),
    );
    let replayed = indexer
        .replay_jruby_catalog_sensitive_files(
            FileProcessor::new().with_jruby_import_provider(provider.clone()),
            &server.load_context_for_project(indexer.workspace_root()),
        )
        .unwrap();

    assert_eq!(replayed, 1, "ordinary Ruby files must not be replayed");
    assert!(
        workspace_state
            .handle()
            .test_read()
            .view()
            .symbol_facts()
            .any(|fact| fact.fqn == imported),
        "the exact provider pass must replace the Java-sensitive file with its imported alias"
    );
}

#[test]
fn exact_jruby_provider_installed_before_tail_replays_only_active_frontier_files() {
    let workspace = TempDir::new().unwrap();
    let signature_cache = TempDir::new().unwrap();
    let root = workspace.path();
    let active_path = root.join("a_active_import.rb");
    let tail_path = root.join("b_tail_import.rb");
    std::fs::write(
        &active_path,
        "java_import 'com.example.Active'\nACTIVE = Active.new\n",
    )
    .unwrap();
    std::fs::write(
        &tail_path,
        "java_import 'com.example.Tail'\nTAIL = Tail.new\n",
    )
    .unwrap();

    let server = Server::default();
    let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
    let mut indexer = IndexerProject::new(
        root.to_path_buf(),
        FileProcessor::new(),
        IndexingConfig::default(),
    );
    indexer
        .set_navigation_priority_keys(HashSet::from(["aactiveimport".to_string()]), HashSet::new());
    indexer
        .collect_project_navigation_facts(
            &server.load_context_for_project(indexer.workspace_root()),
        )
        .unwrap();

    let active = FullyQualifiedName::try_from("Active").unwrap();
    let tail = FullyQualifiedName::try_from("Tail").unwrap();
    assert!(
        !workspace_state
            .handle()
            .test_read()
            .view()
            .symbol_facts()
            .any(|fact| fact.fqn == active),
        "the latency frontier must remain providerless"
    );

    let provider = Arc::new(
        jruby_provider(&["com/example/Active", "com/example/Tail"])
            .with_signature_cache_root(signature_cache.path().to_path_buf()),
    );
    indexer.install_jruby_import_provider(provider.clone());
    indexer
        .collect_remaining_project_facts(&server.load_context_for_project(indexer.workspace_root()))
        .unwrap();

    {
        let engine = workspace_state.handle().test_read();
        let symbols = engine.view().symbol_facts().collect::<Vec<_>>();
        assert!(
            symbols.iter().any(|fact| fact.fqn == tail),
            "the exhaustive tail must be collected once with the exact provider"
        );
        assert!(
            !symbols.iter().any(|fact| fact.fqn == active),
            "the providerless active file must wait for its bounded exact replay"
        );
    }
    assert_eq!(
        indexer.jruby_catalog_sensitive_files(&provider),
        vec![active_path],
        "provider-aware tail files must not enter the replay set"
    );

    let replayed = indexer
        .replay_jruby_catalog_sensitive_files(
            FileProcessor::new().with_jruby_import_provider(provider),
            &server.load_context_for_project(indexer.workspace_root()),
        )
        .unwrap();
    assert_eq!(replayed, 1);
    assert!(
        workspace_state
            .handle()
            .test_read()
            .view()
            .symbol_facts()
            .any(|fact| fact.fqn == active),
        "the bounded replay must replace the active file with exact Java facts"
    );
}

#[test]
fn exact_jruby_provider_handoff_between_batches_replays_only_providerless_files() {
    let workspace = TempDir::new().unwrap();
    let signature_cache = TempDir::new().unwrap();
    let root = workspace.path();
    let active_path = root.join("a_active.rb");
    let first_path = root.join("b_first_import.rb");
    let second_path = root.join("c_second_import.rb");
    std::fs::write(&active_path, "class ActiveDocument\nend\n").unwrap();
    std::fs::write(
        &first_path,
        "java_import 'com.example.First'\nFIRST = First.new\n",
    )
    .unwrap();
    std::fs::write(
        &second_path,
        "java_import 'com.example.Second'\nSECOND = Second.new\n",
    )
    .unwrap();

    let server = Server::default();
    let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
    let mut indexer = IndexerProject::new(
        root.to_path_buf(),
        FileProcessor::new(),
        IndexingConfig::default(),
    );
    indexer.set_navigation_priority_keys(HashSet::from(["aactive".to_string()]), HashSet::new());
    indexer
        .collect_project_navigation_facts(
            &server.load_context_for_project(indexer.workspace_root()),
        )
        .unwrap();
    indexer.refresh_exhaustive_semantic_context().unwrap();

    let first_batch = indexer.take_next_remaining_project_files(1);
    assert_eq!(first_batch, vec![first_path.clone()]);
    indexer
        .collect_project_file_batch(
            &first_batch,
            &server.load_context_for_project(indexer.workspace_root()),
            false,
        )
        .unwrap();

    let provider = Arc::new(
        jruby_provider(&["com/example/First", "com/example/Second"])
            .with_signature_cache_root(signature_cache.path().to_path_buf()),
    );
    indexer.install_jruby_import_provider(provider.clone());

    let second_batch = indexer.take_next_remaining_project_files(1);
    assert_eq!(second_batch, vec![second_path.clone()]);
    indexer
        .collect_project_file_batch(
            &second_batch,
            &server.load_context_for_project(indexer.workspace_root()),
            true,
        )
        .unwrap();
    indexer.finish_remaining_project_facts();

    assert_eq!(
        indexer.jruby_catalog_sensitive_files(&provider),
        vec![first_path],
        "only files collected before the provider handoff may enter the replay set"
    );
    let second = FullyQualifiedName::try_from("Second").unwrap();
    assert!(
        workspace_state
            .handle()
            .test_read()
            .view()
            .symbol_facts()
            .any(|fact| fact.fqn == second),
        "the provider-aware batch must expose its Java import before replay"
    );

    let replayed = indexer
        .replay_jruby_catalog_sensitive_files(
            FileProcessor::new().with_jruby_import_provider(provider),
            &server.load_context_for_project(indexer.workspace_root()),
        )
        .unwrap();
    assert_eq!(replayed, 1);
    let first = FullyQualifiedName::try_from("First").unwrap();
    assert!(
        workspace_state
            .handle()
            .test_read()
            .view()
            .symbol_facts()
            .any(|fact| fact.fqn == first),
        "the bounded replay must replace the providerless batch with exact Java facts"
    );
}

#[test]
fn exact_jruby_provider_handoff_preserves_generated_signature_facts() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    let active_path = root.join("a_active.rb");
    let first_path = root.join("b_first_import.rb");
    let second_path = root.join("c_second_import.rb");
    std::fs::write(&active_path, "class ActiveDocument\nend\n").unwrap();
    std::fs::write(
        &first_path,
        "java_import 'com.example.First'\nFIRST = First.new\n",
    )
    .unwrap();
    std::fs::write(
        &second_path,
        "java_import 'com.example.Second'\nSECOND = Second.new\n",
    )
    .unwrap();

    let run = |install_before_tail: bool| {
        let signature_cache = TempDir::new().unwrap();
        let server = Server::default();
        let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
        let mut indexer = IndexerProject::new(
            root.to_path_buf(),
            FileProcessor::new(),
            IndexingConfig::default(),
        );
        indexer
            .set_navigation_priority_keys(HashSet::from(["aactive".to_string()]), HashSet::new());
        indexer
            .collect_project_navigation_facts(
                &server.load_context_for_project(indexer.workspace_root()),
            )
            .unwrap();
        indexer.refresh_exhaustive_semantic_context().unwrap();

        let provider = Arc::new(
            jruby_provider_with_superclasses(&[
                ("com/example/First", "com/example/Second"),
                ("com/example/Second", "java/lang/Object"),
            ])
            .with_signature_cache_root(signature_cache.path().to_path_buf()),
        );
        if install_before_tail {
            indexer.install_jruby_import_provider(provider.clone());
        }

        let first_batch = indexer.take_next_remaining_project_files(1);
        assert_eq!(first_batch, vec![first_path.clone()]);
        indexer
            .collect_project_file_batch(
                &first_batch,
                &server.load_context_for_project(indexer.workspace_root()),
                false,
            )
            .unwrap();
        if !install_before_tail {
            indexer.install_jruby_import_provider(provider.clone());
        }

        let second_batch = indexer.take_next_remaining_project_files(1);
        assert_eq!(second_batch, vec![second_path.clone()]);
        indexer
            .collect_project_file_batch(
                &second_batch,
                &server.load_context_for_project(indexer.workspace_root()),
                true,
            )
            .unwrap();
        indexer.finish_remaining_project_facts();
        indexer
            .replay_jruby_catalog_sensitive_files(
                FileProcessor::new().with_jruby_import_provider(provider),
                &server.load_context_for_project(indexer.workspace_root()),
            )
            .unwrap();
        workspace_state.handle().test_write().resolve();

        let engine = workspace_state.handle().test_read();
        let first_signature_path = signature_cache.path().join("com/example/First.rb");
        let first_signature_id = engine
            .view()
            .file_id(&first_signature_path)
            .expect_invariant(
                "generated First signature was not indexed",
                "both schedules import the exact catalog class",
                "keep the fixture import and signature cache identity aligned",
            );
        (
            engine
                .view()
                .semantic_export_fingerprint(first_signature_id)
                .expect_invariant(
                    "generated First signature has no export fingerprint",
                    "every indexed signature enters through Project::update",
                    "retain the ordinary file-owned signature lifecycle in the fixture",
                ),
            engine.view().semantic_result_fingerprint(),
        )
    };

    let exact_before_tail = run(true);
    let handed_off_between_batches = run(false);
    assert_eq!(
        handed_off_between_batches, exact_before_tail,
        "exact JRuby provider readiness timing must not change generated signature facts or the final semantic result"
    );
}

#[test]
fn exact_jruby_provider_handoff_preserves_ordinary_include_diagnostics() {
    let workspace = TempDir::new().unwrap();
    let root = workspace.path();
    let active_path = root.join("a_active.rb");
    let ordinary_path = root.join("b_ordinary_include.rb");
    let import_path = root.join("c_import.rb");
    std::fs::write(&active_path, "class ActiveDocument\nend\n").unwrap();
    std::fs::write(
        &ordinary_path,
        "[301, 302].should include last_response.status\n",
    )
    .unwrap();
    std::fs::write(
        &import_path,
        "java_import 'com.example.Imported'\nIMPORTED = Imported.new\n",
    )
    .unwrap();

    let run = |install_before_tail: bool| {
        let signature_cache = TempDir::new().unwrap();
        let server = Server::default();
        let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
        let mut indexer = IndexerProject::new(
            root.to_path_buf(),
            FileProcessor::new(),
            IndexingConfig::default(),
        );
        indexer
            .set_navigation_priority_keys(HashSet::from(["aactive".to_string()]), HashSet::new());
        indexer
            .collect_project_navigation_facts(
                &server.load_context_for_project(indexer.workspace_root()),
            )
            .unwrap();
        indexer.refresh_exhaustive_semantic_context().unwrap();

        let provider = Arc::new(
            jruby_provider(&["com/example/Imported"])
                .with_signature_cache_root(signature_cache.path().to_path_buf()),
        );
        if install_before_tail {
            indexer.install_jruby_import_provider(provider.clone());
        }

        let ordinary_batch = indexer.take_next_remaining_project_files(1);
        assert_eq!(ordinary_batch, vec![ordinary_path.clone()]);
        indexer
            .collect_project_file_batch(
                &ordinary_batch,
                &server.load_context_for_project(indexer.workspace_root()),
                false,
            )
            .unwrap();
        if !install_before_tail {
            indexer.install_jruby_import_provider(provider.clone());
        }

        let import_batch = indexer.take_next_remaining_project_files(1);
        assert_eq!(import_batch, vec![import_path.clone()]);
        indexer
            .collect_project_file_batch(
                &import_batch,
                &server.load_context_for_project(indexer.workspace_root()),
                true,
            )
            .unwrap();
        indexer.finish_remaining_project_facts();
        let replayed = indexer
            .replay_jruby_catalog_sensitive_files(
                FileProcessor::new().with_jruby_import_provider(provider),
                &server.load_context_for_project(indexer.workspace_root()),
            )
            .unwrap();
        assert_eq!(
            replayed, 0,
            "an ordinary Ruby include expression must not enter the JRuby replay set"
        );
        workspace_state.handle().test_write().resolve();

        let engine = workspace_state.handle().test_read();
        let ordinary_id = engine.view().file_id(&ordinary_path).expect_invariant(
            "ordinary include fixture was not indexed",
            "the exhaustive batch must register every selected source",
            "keep the fixture inside the project root and finish the batch",
        );
        (
            engine.view().diagnostic_facts_in_file(ordinary_id),
            engine.view().semantic_result_fingerprint(),
        )
    };

    let exact_before_tail = run(true);
    let handed_off_between_batches = run(false);
    assert_eq!(
        handed_off_between_batches, exact_before_tail,
        "provider readiness timing must not reinterpret ordinary Ruby include expressions as JRuby interfaces"
    );
}

#[test]
fn exact_jruby_replay_is_independent_of_exhaustive_batch_boundaries() {
    let workspace = TempDir::new().unwrap();
    let signature_cache = TempDir::new().unwrap();
    let root = workspace.path();
    let first_path = root.join("a_import.rb");
    let second_path = root.join("b_import.rb");
    std::fs::write(
        &first_path,
        "java_import 'com.example.First'\nFIRST = First.new\nFIRST_LATE = LateBound.new\n",
    )
    .unwrap();
    std::fs::write(
        &second_path,
        "java_import 'com.example.Second'\nSECOND = Second.new\nSECOND_LATE = LateBound.new\n",
    )
    .unwrap();

    let late_dependency_uri = Url::from_file_path(root.join("late_dependency.rb")).unwrap();
    let first = FullyQualifiedName::try_from("First").unwrap();
    let second = FullyQualifiedName::try_from("Second").unwrap();
    let mut expected = None;
    for batch_size in [1, 2] {
        let server = Server::default();
        let workspace_state = server.add_workspace(Url::from_directory_path(root).unwrap());
        let mut indexer = IndexerProject::new(
            root.to_path_buf(),
            FileProcessor::new(),
            IndexingConfig::default(),
        );
        indexer
            .collect_project_navigation_facts(
                &server.load_context_for_project(indexer.workspace_root()),
            )
            .unwrap();
        while indexer.remaining_project_file_count() > 0 {
            let batch = indexer.take_next_remaining_project_files(batch_size);
            let is_last = indexer.remaining_project_file_count() == 0;
            indexer
                .collect_project_file_batch(
                    &batch,
                    &server.load_context_for_project(indexer.workspace_root()),
                    is_last,
                )
                .unwrap();
        }
        indexer.finish_remaining_project_facts();

        FileProcessor::new()
            .collect_file_facts_as_deferred_resolution(
                &late_dependency_uri,
                "class LateBound\nend\n",
                &server,
                SourceKind::External,
            )
            .unwrap();
        let provider = Arc::new(
            jruby_provider(&["com/example/First", "com/example/Second"])
                .with_signature_cache_root(signature_cache.path().to_path_buf()),
        );
        let replayed = indexer
            .replay_jruby_catalog_sensitive_files(
                FileProcessor::new().with_jruby_import_provider(provider),
                &server.load_context_for_project(indexer.workspace_root()),
            )
            .unwrap();
        assert_eq!(replayed, 2);
        workspace_state.handle().test_write().resolve();
        let engine = workspace_state.handle().test_read();
        let symbols = engine.view().symbol_facts().collect::<Vec<_>>();
        assert!(symbols.iter().any(|fact| fact.fqn == first));
        assert!(symbols.iter().any(|fact| fact.fqn == second));
        let actual = engine.view().semantic_result_fingerprint();
        if let Some(expected) = expected {
            assert_eq!(
                actual, expected,
                "exact JRuby replay must not depend on arbitrary exhaustive batch boundaries"
            );
        } else {
            expected = Some(actual);
        }
    }
}
