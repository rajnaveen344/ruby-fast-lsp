use super::*;
use crate::environment::extensions::ExtensionStat;
use crate::indexer::cache::persistent::PersistentProductStat;

#[test]
fn tracked_call_name_set_is_shared_arc_and_covers_rspec_without_ordinary_ruby_names() {
    let registry = ExtensionRegistryHandle::empty();
    let names = registry.tracked_call_names();
    let again = registry.tracked_call_names();
    assert!(
        Arc::ptr_eq(&names, &again),
        "file hosts must clone the name-only Arc rather than copying the set"
    );
    assert!(
        names.contains("describe"),
        "RSpec frame names must remain in the name-only prefilter"
    );
    assert!(
        !names.contains("save"),
        "ordinary Ruby method names must miss the name-only prefilter so they never take the registry lock"
    );
}

#[test]
fn extension_discovery_rejects_oversized_wasm_before_allocating_its_payload() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    let wasm_path = temp_dir.path().join("oversized.wasm");
    let file = fs::File::create(&wasm_path).expect("sparse Wasm fixture must be created");
    file.set_len(MAX_EXTENSION_WASM_BYTES + 1)
        .expect("sparse Wasm fixture length must be set");

    let error = read_extension_wasm(&wasm_path)
        .expect_err("oversized Wasm must be rejected from metadata before payload allocation");

    assert!(error.to_string().contains("maximum is 67108864"));
}

#[test]
fn activation_failure_disables_extension_before_use() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    let package = temp_dir.path().join("activation-failure");
    write_activation_failure_package(&package);

    let registry = ExtensionRegistry::load(&ExtensionLoadConfig {
        package_paths: vec![ConfiguredExtensionPath {
            path: package,
            source: ExtensionPathSource::InitializationOptions,
        }],
        directory_paths: Vec::new(),
        project_package_paths: Vec::new(),
        settings: BTreeMap::new(),
    });

    let reports = registry.status_reports();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].id, "activation-failure");
    assert_eq!(reports[0].status, "failed");
    assert_eq!(reports[0].telemetry.get(ExtensionStat::GuestCalls), 1);
    assert_eq!(reports[0].telemetry.get(ExtensionStat::LifecycleCalls), 1);
    assert_eq!(reports[0].telemetry.get(ExtensionStat::IndexCalls), 0);
    assert_eq!(reports[0].telemetry.get(ExtensionStat::EventCalls), 0);
    assert_eq!(reports[0].telemetry.get(ExtensionStat::GuestFailures), 1);
    assert_eq!(reports[0].telemetry.get(ExtensionStat::Disablements), 1);
    assert!(
        reports[0].telemetry.get(ExtensionStat::MaxGuestTimeNs)
            <= reports[0].telemetry.get(ExtensionStat::TotalGuestTimeNs)
    );
    invariant!(
        reports[0]
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("activation")),
        what = "activation failure was not reported with lifecycle context",
        why = "users cannot diagnose why an extension was disabled",
        fix = "retain the activation error in extension status",
    );
}

#[test]
fn resource_limit_failure_is_visible_in_extension_telemetry() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    let package = temp_dir.path().join("resource-limit-failure");
    write_resource_limit_failure_package(&package);

    let registry = ExtensionRegistry::load(&ExtensionLoadConfig {
        package_paths: vec![ConfiguredExtensionPath {
            path: package,
            source: ExtensionPathSource::InitializationOptions,
        }],
        ..ExtensionLoadConfig::default()
    });

    let report = &registry.status_reports()[0];
    assert_eq!(report.status, "failed");
    assert_eq!(report.telemetry.get(ExtensionStat::LifecycleCalls), 1);
    assert_eq!(report.telemetry.get(ExtensionStat::GuestFailures), 1);
    assert_eq!(
        report.telemetry.get(ExtensionStat::ResourceLimitFailures),
        1
    );
    assert_eq!(report.telemetry.get(ExtensionStat::GuestTraps), 0);
    assert_eq!(report.telemetry.get(ExtensionStat::Disablements), 1);
    assert!(report
        .last_error
        .as_deref()
        .is_some_and(|error| error.contains("output payload") && error.contains("exceeds max")));
}

#[test]
fn guest_trap_is_visible_in_extension_telemetry() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    let package = temp_dir.path().join("trap-failure");
    write_trap_failure_package(&package);

    let registry = ExtensionRegistry::load(&ExtensionLoadConfig {
        package_paths: vec![ConfiguredExtensionPath {
            path: package,
            source: ExtensionPathSource::InitializationOptions,
        }],
        ..ExtensionLoadConfig::default()
    });

    let report = &registry.status_reports()[0];
    assert_eq!(report.status, "failed");
    assert_eq!(report.telemetry.get(ExtensionStat::LifecycleCalls), 1);
    assert_eq!(report.telemetry.get(ExtensionStat::GuestFailures), 1);
    assert_eq!(report.telemetry.get(ExtensionStat::GuestTraps), 1);
    assert_eq!(
        report.telemetry.get(ExtensionStat::ResourceLimitFailures),
        0
    );
    assert_eq!(report.telemetry.get(ExtensionStat::Disablements), 1);
    assert!(report
        .last_error
        .as_deref()
        .is_some_and(|error| error.contains("wasm trap") || error.contains("unreachable")));
}

#[test]
fn settings_only_reconfiguration_notifies_existing_extension() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    let package = temp_dir.path().join("settings-failure");
    write_settings_failure_package(&package);
    let mut config = RubyFastLspConfig {
        extension_packages: vec![package.to_string_lossy().into_owned()],
        ..RubyFastLspConfig::default()
    };
    let registry = ExtensionRegistryHandle::from_config(&config);
    assert_eq!(registry.status_reports()[0].status, "loaded");

    config.extension_settings.insert(
        "settings-failure".to_string(),
        serde_json::json!({"mode": "strict"}),
    );
    registry.configure_from_config(&config);

    let reports = registry.status_reports();
    assert_eq!(reports[0].status, "failed");
    invariant!(
        reports[0]
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("settings.changed")),
        what = "settings event failure lacks event context",
        why = "settings-only reload failures must be diagnosable",
        fix = "report the settings.changed event in extension status",
    );

    config.extension_settings.insert(
        "settings-failure".to_string(),
        serde_json::json!({"mode": "relaxed"}),
    );
    registry.configure_from_config(&config);
    assert_eq!(
        registry.status_reports()[0].status,
        "loaded",
        "a failed extension must be recreated so corrected settings can recover it"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn extension_reconfiguration_waits_for_weighted_admission_without_blocking_reactor() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    let package = temp_dir.path().join("rspec-ruby");
    copy_rspec_package(&package, "0.1.0-governed");
    let config = RubyFastLspConfig {
        extension_packages: vec![package.to_string_lossy().into_owned()],
        ..RubyFastLspConfig::default()
    };
    let registry = ExtensionRegistryHandle::from_config(&RubyFastLspConfig::default());
    let governor = IndexingResourceGovernor::new(
        crate::indexer::scheduling::resources::IndexingResourcePolicy::with_limits(
            1,
            1,
            256 * 1024 * 1024,
            1,
        ),
    );
    let holder_release = Arc::new(tokio::sync::Notify::new());
    let holder_release_task = holder_release.clone();
    let holder_governor = governor.clone();
    let holder = tokio::spawn(async move {
        holder_governor
            .run_async_with_resources(
                "extension reload contention holder",
                IndexingWorkSpec::new(
                    None,
                    IndexingResourcePriority::Background,
                    1,
                    256 * 1024 * 1024,
                    1,
                ),
                None,
                async move {
                    holder_release_task.notified().await;
                },
            )
            .await
            .unwrap();
    });
    tokio::time::timeout(Duration::from_secs(1), async {
        while governor.snapshot().active_tasks != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("resource holder must be admitted before extension reload");

    let reload_registry = registry.clone();
    let reload_governor = governor.clone();
    let reload = tokio::spawn(async move {
        reload_registry
            .configure_from_config_and_workspace_roots_governed(&config, &[], reload_governor)
            .await
            .unwrap();
    });
    tokio::time::timeout(Duration::from_secs(1), async {
        while governor.snapshot().queued_tasks != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("extension reload must queue behind the complete weighted claim");
    tokio::time::timeout(
        Duration::from_millis(50),
        tokio::time::sleep(Duration::from_millis(1)),
    )
    .await
    .expect("queued extension reload must not block the current-thread Tokio reactor");
    assert!(
        !reload.is_finished(),
        "extension reload must not bypass weighted admission"
    );

    holder_release.notify_one();
    holder.await.unwrap();
    reload.await.unwrap();
    let reports = registry.status_reports();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].version.as_deref(), Some("0.1.0-governed"));
    assert_eq!(reports[0].status, "loaded");
    let complete = governor.snapshot();
    assert_eq!(complete.active_tasks, 0);
    assert_eq!(complete.queued_tasks, 0);
    assert_eq!(complete.completed_tasks, 2);
}

#[tokio::test(flavor = "current_thread")]
async fn response_requests_without_loaded_capability_bypass_resource_admission() {
    let registry = ExtensionRegistryHandle::empty();
    let governor = IndexingResourceGovernor::new(
        crate::indexer::scheduling::resources::IndexingResourcePolicy::with_limits(1, 1, 1, 1),
    );

    assert!(registry
        .document_symbols_governed(
            governor.clone(),
            None,
            "file:///workspace/plain.rb".to_string(),
            "class Plain\nend\n".to_string(),
            None,
        )
        .await
        .unwrap()
        .is_empty());
    assert!(registry
        .code_lenses_governed(
            governor.clone(),
            None,
            "file:///workspace/plain.rb".to_string(),
            "class Plain\nend\n".to_string(),
            None,
        )
        .await
        .unwrap()
        .is_empty());

    let snapshot = governor.snapshot();
    assert_eq!(snapshot.active_tasks, 0);
    assert_eq!(snapshot.queued_tasks, 0);
    assert_eq!(snapshot.completed_tasks, 0);
}

#[test]
fn in_place_package_change_reloads_same_configured_path() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    let package = temp_dir.path().join("rspec-ruby");
    copy_rspec_package(&package, "0.1.0");
    let config = RubyFastLspConfig {
        extension_packages: vec![package.to_string_lossy().into_owned()],
        ..RubyFastLspConfig::default()
    };
    let registry = ExtensionRegistryHandle::from_config(&config);
    let previous = registry.extensions()[0].clone();
    assert_eq!(
        registry.status_reports()[0].version.as_deref(),
        Some("0.1.0")
    );

    copy_rspec_package(&package, "0.2.0");
    registry.configure_from_config(&config);

    let reports = registry.status_reports();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].status, "loaded");
    assert_eq!(
        reports[0].version.as_deref(),
        Some("0.2.0"),
        "changing a package in place must reload it even when configured paths are unchanged"
    );
    assert_eq!(
        previous.status_report().status,
        "deactivated",
        "the replaced guest must receive lifecycle.deactivate after the replacement is active"
    );
}

#[test]
fn fresh_registry_process_reuses_exact_persistent_compiled_wasm() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    let package = temp_dir.path().join("cacheable-extension");
    write_cacheable_extension_package(&package);
    let config = RubyFastLspConfig {
        extension_packages: vec![package.to_string_lossy().into_owned()],
        ..RubyFastLspConfig::default()
    };
    let cache_root = temp_dir.path().join("cache");
    let first_cache =
        PersistentDerivedProductCache::with_limits(cache_root.clone(), 8, 16 * 1024 * 1024);
    let first_registry = ExtensionRegistryHandle::empty_with_cache(first_cache.clone());
    first_registry.configure_from_config(&config);
    assert_eq!(first_registry.status_reports()[0].status, "loaded");
    assert_eq!(
        first_cache
            .compiled_wasm_snapshot()
            .get(PersistentProductStat::Producers),
        1
    );
    assert_eq!(
        first_cache
            .compiled_wasm_snapshot()
            .get(PersistentProductStat::Publications),
        1
    );
    first_registry.shutdown();
    drop(first_registry);
    drop(first_cache);

    let second_cache = PersistentDerivedProductCache::with_limits(cache_root, 8, 16 * 1024 * 1024);
    let second_registry = ExtensionRegistryHandle::empty_with_cache(second_cache.clone());
    second_registry.configure_from_config(&config);
    assert_eq!(second_registry.status_reports()[0].status, "loaded");
    assert_eq!(
        second_cache
            .compiled_wasm_snapshot()
            .get(PersistentProductStat::Hits),
        1
    );
    assert_eq!(
        second_cache
            .compiled_wasm_snapshot()
            .get(PersistentProductStat::Producers),
        0
    );
}

#[test]
fn valid_envelope_with_invalid_native_wasm_artifact_is_rebuilt() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    let package = temp_dir.path().join("cacheable-extension");
    let wasm = write_cacheable_extension_package(&package);
    let compiler = ruby_fast_lsp_extension_wasm_host::WasmExtensionCompiler::new().unwrap();
    let key = CompiledWasmProductKey::new(&wasm, compiler.cache_identity());
    let cache_root = temp_dir.path().join("cache");
    let seeding_cache =
        PersistentDerivedProductCache::with_limits(cache_root.clone(), 8, 16 * 1024 * 1024);
    let PersistentCompiledWasmLookup::Reservation(reservation) =
        seeding_cache.lookup_compiled_wasm_or_reserve(&key).unwrap()
    else {
        panic!("test compiled Wasm cache must begin empty");
    };
    reservation
        .publish(&key, b"not a Wasmtime serialized module")
        .unwrap();
    drop(seeding_cache);

    let recovering_cache =
        PersistentDerivedProductCache::with_limits(cache_root, 8, 16 * 1024 * 1024);
    let registry = ExtensionRegistryHandle::empty_with_cache(recovering_cache.clone());
    registry.configure_from_config(&RubyFastLspConfig {
        extension_packages: vec![package.to_string_lossy().into_owned()],
        ..RubyFastLspConfig::default()
    });
    assert_eq!(registry.status_reports()[0].status, "loaded");
    let snapshot = recovering_cache.compiled_wasm_snapshot();
    assert_eq!(snapshot.get(PersistentProductStat::Hits), 1);
    assert_eq!(snapshot.get(PersistentProductStat::Corruptions), 1);
    assert_eq!(snapshot.get(PersistentProductStat::Producers), 1);
    assert_eq!(snapshot.get(PersistentProductStat::Publications), 1);
}

#[test]
fn shutdown_deactivates_loaded_extensions() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    let package = temp_dir.path().join("rspec-ruby");
    copy_rspec_package(&package, "0.1.0");
    let config = RubyFastLspConfig {
        extension_packages: vec![package.to_string_lossy().into_owned()],
        ..RubyFastLspConfig::default()
    };
    let registry = ExtensionRegistryHandle::from_config(&config);
    assert_eq!(registry.status_reports()[0].status, "loaded");

    registry.shutdown();

    assert_eq!(registry.status_reports()[0].status, "deactivated");
}

#[tokio::test]
async fn trusted_workspace_discovers_project_local_extension_package() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    let package = temp_dir.path().join(".ruby-fast-lsp/extensions/rspec-ruby");
    copy_rspec_package(&package, "0.1.0-project");
    let root_uri = Url::from_directory_path(temp_dir.path())
        .expect("test workspace path must convert to a file URI");
    let server = RubyLanguageServer::default();

    server
        .initialize(InitializeParams {
            root_uri: Some(root_uri),
            initialization_options: Some(serde_json::json!({
                "workspaceTrusted": true,
                "projectExtensionsEnabled": true
            })),
            ..InitializeParams::default()
        })
        .await
        .expect("test server initialization must succeed");

    let reports = server.extensions.registry().status_reports();
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].id, "rspec-ruby");
    assert_eq!(reports[0].version.as_deref(), Some("0.1.0-project"));
    assert_eq!(reports[0].status, "loaded");
}

#[tokio::test]
async fn dynamic_workspace_change_reconfigures_project_extensions() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    copy_rspec_package(
        &temp_dir.path().join(".ruby-fast-lsp/extensions/rspec-ruby"),
        "0.1.0-dynamic-lsp",
    );
    let root_uri = Url::from_directory_path(temp_dir.path())
        .expect("test workspace path must convert to a file URI");
    let folder = WorkspaceFolder {
        uri: root_uri,
        name: "dynamic".to_string(),
    };
    let server = RubyLanguageServer::default();
    server
        .initialize(InitializeParams {
            initialization_options: Some(serde_json::json!({
                "workspaceTrusted": true,
                "projectExtensionsEnabled": true
            })),
            ..InitializeParams::default()
        })
        .await
        .expect("test server initialization must succeed");

    server
        .did_change_workspace_folders(DidChangeWorkspaceFoldersParams {
            event: WorkspaceFoldersChangeEvent {
                added: vec![folder.clone()],
                removed: Vec::new(),
            },
        })
        .await;
    assert_eq!(
        server.extensions.registry().status_reports()[0]
            .version
            .as_deref(),
        Some("0.1.0-dynamic-lsp")
    );

    server
        .did_change_workspace_folders(DidChangeWorkspaceFoldersParams {
            event: WorkspaceFoldersChangeEvent {
                added: Vec::new(),
                removed: vec![folder],
            },
        })
        .await;
    assert!(server.extensions.registry().status_reports().is_empty());
}

#[test]
fn untrusted_or_disabled_workspace_does_not_discover_project_extensions() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    let package = temp_dir.path().join(".ruby-fast-lsp/extensions/rspec-ruby");
    copy_rspec_package(&package, "0.1.0-project");

    let untrusted = RubyFastLspConfig {
        workspace_trusted: false,
        project_extensions_enabled: true,
        ..RubyFastLspConfig::default()
    };
    let disabled = RubyFastLspConfig {
        workspace_trusted: true,
        project_extensions_enabled: false,
        ..RubyFastLspConfig::default()
    };

    for config in [&untrusted, &disabled] {
        let load_config = ExtensionLoadConfig::from_config_and_workspace_roots(
            config,
            &[temp_dir.path().to_path_buf()],
        );
        assert!(
            load_config.project_package_paths.is_empty(),
            "project-local Wasm must require both explicit trust and enablement"
        );
        assert!(ExtensionRegistry::load(&load_config)
            .status_reports()
            .is_empty());
    }
}

#[test]
fn conventional_project_tree_is_discovered_recursively() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    let package = temp_dir
        .path()
        .join("ruby_fast_lsp/frameworks/testing/rspec-ruby");
    copy_rspec_package(&package, "0.1.0-conventional");
    let config = RubyFastLspConfig {
        workspace_trusted: true,
        project_extensions_enabled: true,
        ..RubyFastLspConfig::default()
    };

    let registry = ExtensionRegistry::load(&ExtensionLoadConfig::from_config_and_workspace_roots(
        &config,
        &[temp_dir.path().to_path_buf()],
    ));

    assert_eq!(
        registry.status_reports()[0].version.as_deref(),
        Some("0.1.0-conventional")
    );
}

#[test]
fn explicit_package_wins_over_project_local_duplicate() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    let explicit = temp_dir.path().join("explicit-rspec");
    let project = temp_dir
        .path()
        .join(".ruby-fast-lsp/extensions/project-rspec");
    copy_rspec_package(&explicit, "0.1.0-explicit");
    copy_rspec_package(&project, "0.1.0-project");
    let config = RubyFastLspConfig {
        extension_packages: vec![explicit.to_string_lossy().into_owned()],
        workspace_trusted: true,
        project_extensions_enabled: true,
        ..RubyFastLspConfig::default()
    };

    let registry = ExtensionRegistry::load(&ExtensionLoadConfig::from_config_and_workspace_roots(
        &config,
        &[temp_dir.path().to_path_buf()],
    ));

    assert_eq!(registry.status_reports().len(), 1);
    assert_eq!(
        registry.status_reports()[0].version.as_deref(),
        Some("0.1.0-explicit")
    );
}

#[test]
fn project_duplicate_tie_break_is_filesystem_path_not_root_order() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    let root_a = temp_dir.path().join("a-root");
    let root_z = temp_dir.path().join("z-root");
    copy_rspec_package(
        &root_a.join(".ruby-fast-lsp/extensions/rspec-ruby"),
        "0.1.0-a",
    );
    copy_rspec_package(
        &root_z.join(".ruby-fast-lsp/extensions/rspec-ruby"),
        "0.1.0-z",
    );
    let config = RubyFastLspConfig {
        workspace_trusted: true,
        project_extensions_enabled: true,
        ..RubyFastLspConfig::default()
    };

    let registry = ExtensionRegistry::load(&ExtensionLoadConfig::from_config_and_workspace_roots(
        &config,
        &[root_z, root_a],
    ));

    assert_eq!(registry.status_reports().len(), 1);
    assert_eq!(
        registry.status_reports()[0].version.as_deref(),
        Some("0.1.0-a")
    );
}

#[test]
fn workspace_root_changes_add_and_remove_project_extensions() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    copy_rspec_package(
        &temp_dir.path().join(".ruby-fast-lsp/extensions/rspec-ruby"),
        "0.1.0-dynamic",
    );
    let config = RubyFastLspConfig {
        workspace_trusted: true,
        project_extensions_enabled: true,
        ..RubyFastLspConfig::default()
    };
    let registry = ExtensionRegistryHandle::from_config(&config);
    assert!(registry.status_reports().is_empty());

    registry.configure_from_config_and_workspace_roots(&config, &[temp_dir.path().to_path_buf()]);
    assert_eq!(registry.status_reports()[0].status, "loaded");

    registry.configure_from_config_and_workspace_roots(&config, &[]);
    assert!(registry.status_reports().is_empty());
}

#[tokio::test]
async fn matching_watched_file_change_is_routed_to_manifest_extension() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    let package = temp_dir.path().join("watched-file-failure");
    write_watched_file_failure_package(&package);
    let root_uri = Url::from_directory_path(temp_dir.path())
        .expect("test workspace path must convert to a file URI");
    let config = RubyFastLspConfig {
        extension_packages: vec![package.to_string_lossy().into_owned()],
        ..RubyFastLspConfig::default()
    };
    let server = RubyLanguageServer::default();
    server.add_workspace(root_uri);
    server.extensions.registry().configure_from_config(&config);
    assert_eq!(
        server.extensions.registry().status_reports()[0].status,
        "loaded"
    );

    crate::lsp::handlers::notification::handle_did_change_watched_files(
        &server,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent::new(
                Url::from_file_path(temp_dir.path().join("README.md"))
                    .expect("test nonmatching path must convert to URI"),
                FileChangeType::CHANGED,
            )],
        },
    )
    .await;
    assert_eq!(
        server.extensions.registry().status_reports()[0].status,
        "loaded"
    );

    crate::lsp::handlers::notification::handle_did_change_watched_files(
        &server,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent::new(
                Url::from_file_path(temp_dir.path().join("config/routes.rb"))
                    .expect("test matching path must convert to URI"),
                FileChangeType::CHANGED,
            )],
        },
    )
    .await;

    let report = &server.extensions.registry().status_reports()[0];
    assert_eq!(report.status, "failed");
    invariant!(
        report
            .last_error
            .as_deref()
            .is_some_and(|error| error.contains("files.changed")),
        what = "watched-file failure lacks event context",
        why = "extension watcher failures must be diagnosable",
        fix = "retain files.changed in extension status",
    );
}

#[test]
fn watched_file_candidates_use_deepest_root_and_deduplicate() {
    let root = crate::test::harness::fixture_path("/workspace");
    let nested = root.join("engines/payments");
    let uri = Url::from_file_path(nested.join("config/routes.rb"))
        .expect("test watched path must convert to URI");
    let event = FileEvent::new(uri.clone(), FileChangeType::CHANGED);

    let candidates = watched_file_candidates(&[root, nested.clone()], &[event.clone(), event]);

    assert_eq!(candidates.len(), 1);
    assert_eq!(
        candidates[0].workspace_root,
        nested.to_string_lossy().replace('\\', "/")
    );
    assert_eq!(candidates[0].path, "config/routes.rb");
    assert_eq!(candidates[0].uri, uri.to_string());
    assert_eq!(candidates[0].kind, WatchedFileChangeKind::Changed);
}

#[test]
fn watched_file_globs_must_be_valid_workspace_relative_patterns() {
    let matcher = build_watched_file_matcher(
        "watch-test",
        &["config/**/*.rb".to_string(), ".rubocop.yml".to_string()],
    )
    .expect("valid watcher globs must compile");
    assert!(matcher.is_match("config/routes.rb"));
    assert!(matcher.is_match("config/environments/test.rb"));
    assert!(!matcher.is_match("app/models/user.rb"));

    for invalid in [
        "../outside.yml",
        "/absolute.yml",
        "C:/absolute.yml",
        "config\\routes.rb",
        "[",
    ] {
        let err = build_watched_file_matcher("watch-test", &[invalid.to_string()])
            .expect_err("invalid or escaping watcher glob must be rejected");
        assert!(
            err.to_string().contains("watched file glob"),
            "watcher validation error must identify the manifest field"
        );
    }

    let manifest: ExtensionManifest = toml::from_str(
        r#"
id = "missing-capability"
abi_version = 1
runtime = "mruby-wasm"
wasm = "extension.wasm"

[watching]
globs = ["config/routes.rb"]
"#,
    )
    .expect("test watcher manifest must parse");
    let err = validate_manifest(&manifest)
        .expect_err("watching declaration without capability must fail");
    assert!(err.to_string().contains("without `watching` capability"));
}

#[test]
fn initialization_package_wins_duplicate_id_independent_of_path_order() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    let environment_package = temp_dir.path().join("a-environment");
    let initialization_package = temp_dir.path().join("z-initialization");
    copy_rspec_package(&environment_package, "0.1.0-environment");
    copy_rspec_package(&initialization_package, "0.1.0-initialization");
    let registry = ExtensionRegistry::load(&ExtensionLoadConfig {
        package_paths: vec![
            ConfiguredExtensionPath {
                path: environment_package,
                source: ExtensionPathSource::Environment,
            },
            ConfiguredExtensionPath {
                path: initialization_package,
                source: ExtensionPathSource::InitializationOptions,
            },
        ],
        directory_paths: Vec::new(),
        project_package_paths: Vec::new(),
        settings: BTreeMap::new(),
    });

    let reports = registry.status_reports();

    assert_eq!(
        reports.len(),
        1,
        "duplicate extension IDs must not both execute"
    );
    assert_eq!(reports[0].id, "rspec-ruby");
    assert_eq!(reports[0].version.as_deref(), Some("0.1.0-initialization"));
}

#[test]
fn wall_clock_deadline_is_reported_as_slow_status() {
    let status = ExtensionStatus::from_failure(
        "failed to call extension: extension wall-clock deadline exceeded",
    );

    assert!(matches!(status, ExtensionStatus::Slow { .. }));
}

#[test]
fn telemetry_classifies_calls_failures_rejections_and_conflicts_without_dimensions() {
    let telemetry = ExtensionTelemetry::default();
    telemetry.record_call(
        GuestCallKind::Lifecycle,
        Duration::from_nanos(3),
        None,
        None,
    );
    telemetry.record_call(
        GuestCallKind::Index,
        Duration::from_nanos(5),
        None,
        Some("failed to call extension: wasm trap: all fuel consumed"),
    );
    telemetry.record_call(
        GuestCallKind::Event,
        Duration::from_nanos(7),
        None,
        Some("extension output payload 9 bytes exceeds max 8 bytes"),
    );
    telemetry.record_rejected_output();
    telemetry.record_patch_conflict();
    telemetry.record_disablement();

    let report = telemetry.report(2);
    assert_eq!(report.get(ExtensionStat::GuestCalls), 3);
    assert_eq!(report.get(ExtensionStat::LifecycleCalls), 1);
    assert_eq!(report.get(ExtensionStat::IndexCalls), 1);
    assert_eq!(report.get(ExtensionStat::EventCalls), 1);
    assert_eq!(report.get(ExtensionStat::GuestFailures), 2);
    assert_eq!(report.get(ExtensionStat::GuestTraps), 1);
    assert_eq!(report.get(ExtensionStat::ResourceLimitFailures), 2);
    assert_eq!(report.get(ExtensionStat::RejectedOutputs), 1);
    assert_eq!(report.get(ExtensionStat::PatchConflicts), 1);
    assert_eq!(report.get(ExtensionStat::Disablements), 1);
    assert_eq!(report.get(ExtensionStat::TotalGuestTimeNs), 15);
    assert_eq!(report.get(ExtensionStat::MaxGuestTimeNs), 7);
    assert_eq!(report.get(ExtensionStat::ProjectInstances), 2);

    let serialized = serde_json::to_value(&report)
        .expect("extension telemetry must remain serializable through the status contract");
    assert!(serialized.get("guest_calls").is_some());
    assert!(serialized.get("resource_limit_failures").is_some());
    assert!(serialized.get("patch_conflicts").is_some());
}

#[test]
fn initialization_options_do_not_load_direct_wasm_files() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    let wasm_path = temp_dir.path().join("extension.wasm");
    fs::write(&wasm_path, b"not real wasm").expect("test wasm marker must be written");

    let config = ExtensionLoadConfig {
        package_paths: vec![ConfiguredExtensionPath {
            path: wasm_path,
            source: ExtensionPathSource::InitializationOptions,
        }],
        directory_paths: Vec::new(),
        project_package_paths: Vec::new(),
        settings: BTreeMap::new(),
    };

    let extensions = load_wasm_extensions(&config);
    invariant!(
        extensions.is_empty(),
        what = "initialization options loaded a direct wasm file",
        why = "editor-installed extensions must be manifest packages",
        fix = "require extension.toml for initialization option extension paths",
    );
}
