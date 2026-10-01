//! JRuby runtime, Java catalog, and decompiled navigation coordination.

use super::*;

#[test]
fn cached_java_artifact_metadata_is_reused_without_cross_project_path_leakage() {
    let fixture = TempDir::new().unwrap();
    let digits = include_str!("../../../../crates/jvm-metadata/fixtures/minimal_class.hex")
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    let class = digits
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect::<Vec<_>>();
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file("com/example/Demo.class", SimpleFileOptions::default())
        .unwrap();
    writer.write_all(&class).unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    let artifact = |path: PathBuf| {
        fs::write(&path, &bytes).unwrap();
        let metadata = fs::metadata(&path).unwrap();
        crate::runtime::jruby::classpath::ClasspathArtifact {
            path,
            origin: crate::runtime::jruby::classpath::ArtifactOrigin::Explicit,
            kind: crate::runtime::jruby::classpath::ArtifactKind::Jar,
            fingerprint_sha256: format!("{:x}", Sha256::digest(&bytes)),
            byte_length: bytes.len() as u64,
            file_identity: crate::runtime::jruby::classpath::SourceFileIdentity {
                byte_length: metadata.len(),
                modified: metadata.modified().unwrap(),
            },
        }
    };
    let first_artifact = artifact(fixture.path().join("project-one.jar"));
    let second_artifact = artifact(fixture.path().join("project-two.jar"));
    let classpath = |root: PathBuf, artifact: ClasspathArtifact| {
        crate::runtime::jruby::classpath::ProjectClasspath {
            project_root: root,
            artifacts: vec![artifact],
            sources: Vec::new(),
            unresolved: Vec::new(),
            fingerprint_sha256: "fixture-classpath".to_string(),
        }
    };
    let first_classpath = classpath(fixture.path().join("project-one"), first_artifact.clone());
    let second_classpath = classpath(fixture.path().join("project-two"), second_artifact.clone());
    let cache =
        PersistentDerivedProductCache::with_limits(fixture.path().join("cache"), 8, 1024 * 1024);
    let process_cache =
        crate::runtime::jruby::java_catalog::JavaArtifactProductCache::new(8, 1024 * 1024);

    let first = build_cached_project_java_catalog(
        &first_classpath,
        17,
        ArchiveLimits::default(),
        &cache,
        &process_cache,
    )
    .unwrap();
    let second = build_cached_project_java_catalog(
        &second_classpath,
        17,
        ArchiveLimits::default(),
        &cache,
        &process_cache,
    )
    .unwrap();

    assert_eq!(cache.java_artifact_snapshot().producers, 1);
    assert_eq!(cache.java_artifact_snapshot().hits, 0);
    assert_eq!(process_cache.snapshot().lookups, 2);
    assert_eq!(process_cache.snapshot().producers, 1);
    assert_eq!(process_cache.snapshot().hits, 1);
    assert_eq!(process_cache.snapshot().entries, 1);
    assert!(process_cache.retained_weight_bytes() > 0);
    assert!(process_cache.retained_weight_bytes() <= 1024 * 1024);
    assert_eq!(
        first.classes["com/example/Demo"].artifact_path,
        first_artifact.path
    );
    assert_eq!(
        second.classes["com/example/Demo"].artifact_path,
        second_artifact.path
    );
    assert!(Arc::ptr_eq(
        &first.classes["com/example/Demo"].class,
        &second.classes["com/example/Demo"].class,
    ));
}

#[test]
fn parallel_cached_java_products_preserve_classpath_winner_order() {
    let fixture = TempDir::new().unwrap();
    let digits = include_str!("../../../../crates/jvm-metadata/fixtures/minimal_class.hex")
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    let class = digits
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect::<Vec<_>>();
    let artifact = |name: &str, marker: &str| {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("com/example/Demo.class", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&class).unwrap();
        writer
            .start_file(format!("META-INF/{marker}"), SimpleFileOptions::default())
            .unwrap();
        writer.write_all(marker.as_bytes()).unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        let path = fixture.path().join(name);
        fs::write(&path, &bytes).unwrap();
        let metadata = fs::metadata(&path).unwrap();
        ClasspathArtifact {
            path,
            origin: crate::runtime::jruby::classpath::ArtifactOrigin::Explicit,
            kind: crate::runtime::jruby::classpath::ArtifactKind::Jar,
            fingerprint_sha256: format!("{:x}", Sha256::digest(&bytes)),
            byte_length: bytes.len() as u64,
            file_identity: crate::runtime::jruby::classpath::SourceFileIdentity {
                byte_length: metadata.len(),
                modified: metadata.modified().unwrap(),
            },
        }
    };
    let winner = artifact("winner.jar", "winner");
    let shadowed = artifact("shadowed.jar", "shadowed");
    let classpath = crate::runtime::jruby::classpath::ProjectClasspath {
        project_root: fixture.path().join("project"),
        artifacts: vec![winner.clone(), shadowed.clone()],
        sources: Vec::new(),
        unresolved: Vec::new(),
        fingerprint_sha256: "ordered-fixture-classpath".to_string(),
    };
    let cache =
        PersistentDerivedProductCache::with_limits(fixture.path().join("cache"), 8, 1024 * 1024);
    let process_cache = JavaArtifactProductCache::new(8, 1024 * 1024);

    let catalog = build_cached_project_java_catalog(
        &classpath,
        17,
        ArchiveLimits::default(),
        &cache,
        &process_cache,
    )
    .unwrap();

    assert_eq!(
        catalog.classes["com/example/Demo"].artifact_path,
        winner.path
    );
    assert_eq!(
        catalog.duplicates,
        vec![crate::runtime::jruby::java_catalog::DuplicateJavaClass {
            name: "com/example/Demo".to_string(),
            winner: winner.path,
            shadowed: shadowed.path,
        }]
    );
    assert_eq!(cache.java_artifact_snapshot().producers, 2);
}

#[test]
fn mri_runtime_does_not_materialize_a_jruby_import_provider() {
    let fixture = TempDir::new().unwrap();
    let root = fixture.path().join("tool");
    fs::create_dir_all(&root).unwrap();
    let executable = root.join("ruby");
    fs::write(&executable, b"fixture").unwrap();
    let runtime = SelectedRuntimeDescriptor {
        implementation: RuntimeImplementation::Mri,
        family: "3.3".to_string(),
        engine_version: "3.3.11".to_string(),
        compatibility_version: "3.3".to_string(),
        executable,
        discovery_source: RuntimeDiscoverySource::Path,
        java_home: None,
    };
    let (provider, archive) = build_jruby_import_provider(
        root,
        RubyFastLspConfig::default(),
        Some(runtime),
        None,
        None,
        None,
        None,
    )
    .unwrap();
    assert!(
        provider.is_none(),
        "MRI Gemfile roots must skip JRuby classpath and Java catalog work"
    );
    assert!(archive.is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn jruby_runtime_companion_overlaps_the_active_project_with_exact_resource_claims() {
    let root = PathBuf::from("/workspace/server");
    let mut server = RubyLanguageServer::default();
    server
        .indexing
        .set_resources(crate::indexing_resources::IndexingResourceGovernor::new(
            crate::indexing_resources::IndexingResourcePolicy::with_limits(6, 2, 512 * MIB, 2),
        ));
    server
        .indexing
        .resources()
        .prioritize_active_project_with_navigation_pending(&root, true);
    let server = Arc::new(server);
    let (started_tx, mut started_rx) = tokio::sync::mpsc::channel(2);
    let (runtime_release_tx, runtime_release_rx) = std::sync::mpsc::channel();
    let (project_release_tx, project_release_rx) = std::sync::mpsc::channel();

    let runtime = {
        let server = server.clone();
        let root = root.clone();
        let started_tx = started_tx.clone();
        tokio::spawn(async move {
            run_cpu_indexing_task(
                &server,
                Some(root),
                None,
                IndexingWorkClass::RuntimeCompanionParallelIo,
                "fixture JRuby catalog",
                move || {
                    started_tx.blocking_send("runtime").unwrap();
                    runtime_release_rx.recv().unwrap();
                },
            )
            .await
            .unwrap();
        })
    };
    let project = {
        let server = server.clone();
        let root = root.clone();
        tokio::spawn(async move {
            run_cpu_indexing_task(
                &server,
                Some(root),
                None,
                IndexingWorkClass::ProjectParallelIo,
                "fixture project pass",
                move || {
                    started_tx.blocking_send("project").unwrap();
                    project_release_rx.recv().unwrap();
                },
            )
            .await
            .unwrap();
        })
    };

    let first = tokio::time::timeout(Duration::from_secs(1), started_rx.recv())
        .await
        .expect("one indexing phase must start")
        .expect("start channel must remain open");
    let second = tokio::time::timeout(Duration::from_secs(1), started_rx.recv())
        .await
        .expect("the companion and project pass must overlap")
        .expect("start channel must remain open");
    assert_ne!(first, second);
    let snapshot = server.indexing.resources().snapshot();
    assert_eq!(snapshot.active_tasks, 2);
    assert_eq!(snapshot.active_cpu_lanes, 6);
    assert_eq!(snapshot.active_transient_memory_bytes, 512 * MIB);
    assert_eq!(snapshot.active_io_slots, 2);

    runtime_release_tx.send(()).unwrap();
    project_release_tx.send(()).unwrap();
    runtime.await.unwrap();
    project.await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn active_navigation_reservation_blocks_a_sibling_runtime_companion() {
    let active_root = PathBuf::from("/workspace/server");
    let sibling_root = PathBuf::from("/workspace/admin");
    let mut server = RubyLanguageServer::default();
    server
        .indexing
        .set_resources(crate::indexing_resources::IndexingResourceGovernor::new(
            crate::indexing_resources::IndexingResourcePolicy::with_limits(6, 2, 512 * MIB, 2),
        ));
    server
        .indexing
        .resources()
        .prioritize_active_project_with_navigation_pending(&active_root, true);
    let server = Arc::new(server);
    let (active_started_tx, active_started_rx) = tokio::sync::oneshot::channel();
    let (active_release_tx, active_release_rx) = std::sync::mpsc::channel();
    let active = {
        let server = server.clone();
        let active_root = active_root.clone();
        tokio::spawn(async move {
            run_cpu_indexing_task(
                &server,
                Some(active_root),
                None,
                IndexingWorkClass::RuntimeCompanionParallelIo,
                "active runtime companion",
                move || {
                    active_started_tx.send(()).unwrap();
                    active_release_rx.recv().unwrap();
                },
            )
            .await
            .unwrap();
        })
    };
    active_started_rx.await.unwrap();

    let (sibling_started_tx, sibling_started_rx) = tokio::sync::oneshot::channel();
    let sibling = {
        let server = server.clone();
        tokio::spawn(async move {
            run_cpu_indexing_task(
                &server,
                Some(sibling_root),
                None,
                IndexingWorkClass::RuntimeCompanionParallelIo,
                "sibling runtime companion",
                move || {
                    sibling_started_tx.send(()).unwrap();
                },
            )
            .await
            .unwrap();
        })
    };
    let mut sibling_started_rx = sibling_started_rx;
    assert!(
        tokio::time::timeout(Duration::from_millis(50), &mut sibling_started_rx)
            .await
            .is_err(),
        "a sibling runtime companion must stay queued while active-project navigation is pending"
    );

    server
        .indexing
        .resources()
        .prioritize_active_project_with_navigation_pending(&active_root, false);
    tokio::time::timeout(Duration::from_secs(1), sibling_started_rx)
        .await
        .expect("the sibling runtime companion must start after reservation release")
        .expect("the sibling runtime companion must signal");
    active_release_tx.send(()).unwrap();
    active.await.unwrap();
    sibling.await.unwrap();
}

#[test]
fn reads_modern_and_legacy_jdk_release_features_without_guessing() {
    let modern = TempDir::new().unwrap();
    fs::write(
        modern.path().join("release"),
        "JAVA_VERSION=\"17.0.12\"\nIMPLEMENTOR=\"fixture\"\n",
    )
    .unwrap();
    assert_eq!(read_jdk_feature(modern.path()).unwrap(), 17);

    let legacy = TempDir::new().unwrap();
    fs::write(
        legacy.path().join("release"),
        "JAVA_VERSION=\"1.8.0_442\"\n",
    )
    .unwrap();
    assert_eq!(read_jdk_feature(legacy.path()).unwrap(), 8);

    let malformed = TempDir::new().unwrap();
    fs::write(
        malformed.path().join("release"),
        "IMPLEMENTOR=\"fixture\"\n",
    )
    .unwrap();
    assert!(read_jdk_feature(malformed.path()).is_err());
}

fn decode_hex(source: &str) -> Vec<u8> {
    let digits = source
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    digits
        .chunks_exact(2)
        .map(|pair| {
            u8::from_str_radix(
                std::str::from_utf8(pair).expect("fixture hex must be ASCII"),
                16,
            )
            .expect("fixture byte must be valid hex")
        })
        .collect()
}

fn write_jar(path: &Path, entry: &str, contents: &[u8]) {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file(entry, SimpleFileOptions::default())
        .expect("fixture JAR entry must start");
    writer
        .write_all(contents)
        .expect("fixture JAR entry must write");
    let bytes = writer
        .finish()
        .expect("fixture JAR must finish")
        .into_inner();
    fs::write(path, bytes).expect("fixture JAR must be written");
}

#[cfg(unix)]
fn real_java_executable() -> PathBuf {
    for candidate in [
        std::env::var_os("JAVA_HOME")
            .map(PathBuf::from)
            .map(|home| home.join("bin/java")),
        Some(PathBuf::from("/opt/homebrew/opt/openjdk/bin/java")),
        Some(PathBuf::from("/usr/local/opt/openjdk/bin/java")),
    ]
    .into_iter()
    .flatten()
    {
        if candidate.is_file() {
            return candidate;
        }
    }
    panic!("a real JDK java executable is required for JRuby decompiler acceptance");
}

#[cfg(unix)]
#[test]
fn source_less_jruby_import_navigates_to_verified_decompiled_implementation() {
    let _decompiler_budget = crate::test::harness::isolate_decompiler_budget();
    let fixture = TempDir::new().unwrap();
    let root = fixture.path().join("admin");
    let jruby_home = fixture.path().join("jruby-9.2.21.0");
    fs::create_dir_all(root.join("lib")).unwrap();
    fs::create_dir_all(jruby_home.join("bin")).unwrap();
    fs::write(jruby_home.join("bin/jruby"), b"fixture").unwrap();
    let java = real_java_executable().canonicalize().unwrap();
    let java_home = java
        .parent()
        .and_then(Path::parent)
        .expect("real JDK java executable must live below JAVA_HOME/bin")
        .to_path_buf();
    let rich_class = decode_hex(include_str!(
        "../../../../crates/jvm-metadata/fixtures/rich_fixture.class.hex"
    ));
    write_jar(
        &root.join("lib/rich.jar"),
        "fixtures/RichFixture.class",
        &rich_class,
    );

    let root_string = format!("{}/", root.to_string_lossy());
    let mut config = RubyFastLspConfig {
        runtime: RuntimeSelectionConfig {
            mode: RuntimeMode::Auto,
            projects: vec![ProjectRuntimeSelection {
                root: root_string.clone(),
                selection: RuntimeSelection::Explicit(SelectedRuntimeDescriptor {
                    implementation: RuntimeImplementation::Jruby,
                    family: "9.2".to_string(),
                    engine_version: "9.2.21.0".to_string(),
                    compatibility_version: "2.5".to_string(),
                    executable: jruby_home.join("bin/jruby"),
                    discovery_source: RuntimeDiscoverySource::Rvm,
                    java_home: Some(java_home),
                }),
            }],
        },
        ..RubyFastLspConfig::default()
    };
    config.jruby.projects = vec![ProjectJrubyConfig {
        root: root_string,
        additional_classpath: vec!["lib/rich.jar".to_string()],
        additional_sources: Vec::new(),
    }];

    let server = RubyLanguageServer::default();
    server.add_workspace(Url::from_directory_path(&root).unwrap());
    let mut coordinator = IndexingCoordinator::new(root.clone(), config);
    let cache = fixture.path().join("user-cache");
    coordinator.set_cache_root(cache.clone());
    coordinator.detect_ruby_version();
    coordinator.setup_jruby_import_provider().unwrap();
    coordinator.setup_file_processor(&server);
    let source = "java_import fixtures.RichFixture\n\
                      RICH = RichFixture.new(nil)\n\
                      VALUE = RICH.java_send(:combine, [java.lang.String, Java::int[]], 'x', [1])\n";
    let source_path = root.join("imports.rb");
    fs::write(&source_path, source).unwrap();
    let mut project_indexer = IndexerProject::new(
        root.clone(),
        coordinator
            .file_processor
            .as_ref()
            .expect("fixture FileProcessor must be configured")
            .clone(),
        coordinator.config.indexing.clone(),
    );
    project_indexer.collect_project_facts(&server).unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();
    coordinator
        .file_processor
        .as_ref()
        .unwrap()
        .process_file(&uri, source, &server)
        .unwrap();

    let engine = server.analysis_engine_for_uri(&uri);
    let engine = engine.read();
    let source_file = AnalysisQuery::new(&engine)
        .file_id(&source_path)
        .expect("project source must be registered");
    let offset = u32::try_from(source.find(":combine").unwrap() + 1).unwrap();
    let targets =
        AnalysisQuery::new(&engine).resolved_reference_definition_ranges_at(source_file, offset);
    let (implementation, target_range) = targets
        .iter()
        .find_map(|target| {
            let file = AnalysisQuery::new(&engine).file(target.file_id)?;
            (file.kind == ruby_analysis::core::SourceKind::External)
                .then(|| (file.path.clone(), *target))
        })
        .unwrap_or_else(|| {
            panic!(
                "source-less Java method must target decompiled external implementation; \
                     targets: {targets:?}"
            )
        });
    assert!(implementation.starts_with(&cache));
    let implementation_source = fs::read_to_string(&implementation).unwrap();
    assert!(
        implementation_source[target_range.start_byte as usize..target_range.end_byte as usize]
            .contains("return List.of(prefix + values.length);"),
        "Go to Definition must select the verified decompiled `combine` declaration and body; \
             target: {target_range:?}"
    );
    assert!(
        targets.iter().all(|target| {
            AnalysisQuery::new(&engine)
                .file(target.file_id)
                .is_none_or(|file| file.kind != ruby_analysis::core::SourceKind::Signature)
        }),
        "metadata-backed decompiled implementation must outrank generated signatures; \
             targets: {targets:?}"
    );
}

#[test]
fn selected_jruby_catalog_contributes_import_facts_to_the_owning_project() {
    let fixture = TempDir::new().unwrap();
    let root = fixture.path().join("admin");
    let jruby_home = fixture.path().join("jruby-9.2.21.0");
    let java_home = fixture.path().join("jdk");
    fs::create_dir_all(root.join("lib")).unwrap();
    fs::create_dir_all(jruby_home.join("bin")).unwrap();
    fs::create_dir_all(java_home.join("jmods")).unwrap();
    fs::write(jruby_home.join("bin/jruby"), b"fixture").unwrap();
    fs::write(java_home.join("release"), "JAVA_VERSION=\"17.0.12\"\n").unwrap();
    let demo_class = decode_hex(include_str!(
        "../../../../crates/jvm-metadata/fixtures/minimal_class.hex"
    ));
    write_jar(
        &root.join("lib/runtime.jar"),
        "com/example/Demo.class",
        &demo_class,
    );
    let rich_class = decode_hex(include_str!(
        "../../../../crates/jvm-metadata/fixtures/rich_fixture.class.hex"
    ));
    write_jar(
        &root.join("lib/rich.jar"),
        "fixtures/RichFixture.class",
        &rich_class,
    );
    let rich_source =
        include_str!("../../../../crates/jvm-metadata/fixtures/sources/RichFixture.java");
    write_jar(
        &root.join("lib/rich-sources.jar"),
        "fixtures/RichFixture.java",
        rich_source.as_bytes(),
    );

    let root_string = format!("{}/", root.to_string_lossy());
    let mut config = RubyFastLspConfig {
        runtime: RuntimeSelectionConfig {
            mode: RuntimeMode::Auto,
            projects: vec![ProjectRuntimeSelection {
                root: root_string.clone(),
                selection: RuntimeSelection::Explicit(SelectedRuntimeDescriptor {
                    implementation: RuntimeImplementation::Jruby,
                    family: "9.2".to_string(),
                    engine_version: "9.2.21.0".to_string(),
                    compatibility_version: "2.5".to_string(),
                    executable: jruby_home.join("bin/jruby"),
                    discovery_source: RuntimeDiscoverySource::Rvm,
                    java_home: Some(java_home),
                }),
            }],
        },
        ..RubyFastLspConfig::default()
    };
    config.jruby.projects = vec![ProjectJrubyConfig {
        root: root_string,
        additional_classpath: vec!["lib/runtime.jar".to_string(), "lib/rich.jar".to_string()],
        additional_sources: Vec::new(),
    }];

    let server = RubyLanguageServer::default();
    server.add_workspace(Url::from_directory_path(&root).unwrap());
    let mut coordinator = IndexingCoordinator::new(root.clone(), config);
    coordinator.set_cache_root(fixture.path().join("user-cache"));
    assert_eq!(
        coordinator.detect_ruby_version(),
        Some(RubyVersion::new_with_implementation(
            2,
            5,
            RubyImplementation::JRuby
        ))
    );
    coordinator.setup_jruby_import_provider().unwrap();
    assert!(coordinator.jruby_import_provider.is_some());
    let signature_cache_root = coordinator
        .jruby_signature_cache_root(
            coordinator
                .jruby_import_provider
                .as_ref()
                .expect("fixture JRuby provider must exist"),
        )
        .unwrap();
    coordinator.setup_file_processor(&server);

    let source = "module Admin\n\
                          java_import fixtures.RichFixture\n\
                          class RichFixture\n\
                            java_alias :merged, :combine\n\
                          end\n\
                          INSTANCE = com.example.Demo.new\n\
                          CANONICAL = Java::Fixtures::RichFixture.new(nil)\n\
                          RICH = RichFixture.new(nil)\n\
                          RESULT = RICH.merged('value', 1)\n\
                          RUN_RESULT = RICH.java_send(:run, [])\n\
                          RUN_HANDLE = RICH.java_method(:run, [])\n\
                          UNBOUND_RUN = RichFixture.java_method(:run, [])\n\
                          DIRECT = RICH.combine('value', [1])\n\
                        end\n";
    let source_path = root.join("imports.rb");
    fs::write(&source_path, source).unwrap();
    let mut project_indexer = IndexerProject::new(
        root.clone(),
        coordinator
            .file_processor
            .as_ref()
            .expect("fixture FileProcessor must be configured")
            .clone(),
        coordinator.config.indexing.clone(),
    );
    project_indexer.collect_project_facts(&server).unwrap();
    let proxy = FullyQualifiedName::namespace(
        ["Java", "ComExample", "Demo"]
            .into_iter()
            .map(|part| ruby_analysis::core::RubyConstant::new(part).unwrap())
            .collect::<Vec<_>>(),
    );
    {
        let engine = server.analysis_engine_for_uri(
            &Url::from_file_path(signature_cache_root.join("com/example/Demo.rb")).unwrap(),
        );
        let engine = engine.read();
        let symbols = AnalysisQuery::new(&engine).all_symbol_facts();
        assert_eq!(
            symbols.iter().filter(|fact| fact.fqn == proxy).count(),
            1,
            "the ordinary cold project pass must materialize generated metadata signatures \
                 from the same parsed sources before its final engine resolution; \
                 indexed symbols: {symbols:?}"
        );
    }
    assert!(
        !root.join(".ruby-fast-lsp").exists(),
        "generated JRuby signatures must never write cache state into the Ruby project"
    );

    let uri = Url::from_file_path(&source_path).unwrap();
    coordinator
        .file_processor
        .as_ref()
        .unwrap()
        .process_file(&uri, source, &server)
        .unwrap();

    let alias = FullyQualifiedName::try_from("Admin::RichFixture").unwrap();
    let engine = server.analysis_engine_for_uri(&uri);
    let engine = engine.read();
    assert_eq!(
        AnalysisQuery::new(&engine).symbols_for_fqn(&alias).len(),
        1,
        "the selected project's Java catalog must flow through ordinary engine facts"
    );
    let source_file = AnalysisQuery::new(&engine)
        .file_id(&source_path)
        .expect("project source must be registered");
    let import_target_offset = u32::try_from(
        source
            .find("fixtures.RichFixture")
            .expect("fixture Java import target must exist")
            + "fixtures.".len(),
    )
    .unwrap();
    let import_targets = AnalysisQuery::new(&engine)
        .resolved_reference_definition_ranges_at(source_file, import_target_offset);
    assert!(
        import_targets.iter().any(|target| {
            AnalysisQuery::new(&engine)
                .file(target.file_id)
                .is_some_and(|file| {
                    file.kind == ruby_analysis::core::SourceKind::External
                        && file.path.ends_with("fixtures/RichFixture.java")
                })
        }),
        "a catalog-proven java_import target must resolve to its exact Java source; \
             targets: {import_targets:?}"
    );
    assert!(
        !AnalysisQuery::new(&engine).navigation_must_fail_closed_at(
            source_file,
            import_target_offset,
            !import_targets.is_empty(),
        ),
        "an exact runtime-owned java_import reference must outrank an overlapping generic \
             dotted-call candidate; targets: {import_targets:?}"
    );
    let new_offset = u32::try_from(
        source
            .find("new")
            .expect("fixture constructor call must exist"),
    )
    .unwrap();
    let constructor_targets = AnalysisQuery::new(&engine)
        .resolved_reference_definition_ranges_at(source_file, new_offset);
    assert!(
        constructor_targets.iter().any(|target| {
            AnalysisQuery::new(&engine)
                .file(target.file_id)
                .is_some_and(|file| file.kind == ruby_analysis::core::SourceKind::Signature)
        }),
        "constructor navigation must resolve to the generated read-only signature document; \
            targets: {constructor_targets:?}"
    );
    let rich_constructor_offset = u32::try_from(
        source
            .find("RichFixture.new")
            .expect("fixture rich constructor call must exist")
            + "RichFixture.".len(),
    )
    .unwrap();
    let rich_constructor_targets = AnalysisQuery::new(&engine)
        .resolved_reference_definition_ranges_at(source_file, rich_constructor_offset);
    assert!(
        rich_constructor_targets.iter().any(|target| {
            AnalysisQuery::new(&engine)
                .file(target.file_id)
                .is_some_and(|file| {
                    file.kind == ruby_analysis::core::SourceKind::External
                        && file.path.ends_with("fixtures/RichFixture.java")
                })
        }),
        "a source-backed constructor must navigate to the exact Java implementation source, \
             not its generated signature; targets: {rich_constructor_targets:?}"
    );
    let alias_call_offset = u32::try_from(
        source
            .rfind("merged")
            .expect("fixture alias call must exist"),
    )
    .unwrap();
    let alias_targets = AnalysisQuery::new(&engine)
        .resolved_reference_definition_ranges_at(source_file, alias_call_offset);
    assert!(
        alias_targets
            .iter()
            .any(|target| target.file_id == source_file),
        "java_alias calls must resolve to the file-owned alias declaration; \
             targets: {alias_targets:?}"
    );
    let run_offset = u32::try_from(
        source
            .find(":run")
            .expect("fixture java_send method symbol must exist")
            + 1,
    )
    .unwrap();
    let run_targets = AnalysisQuery::new(&engine)
        .resolved_reference_definition_ranges_at(source_file, run_offset);
    assert!(
        run_targets.iter().any(|target| {
            AnalysisQuery::new(&engine)
                .file(target.file_id)
                .is_some_and(|file| {
                    file.kind == ruby_analysis::core::SourceKind::External
                        && file.path.ends_with("fixtures/RichFixture.java")
                })
        }),
        "java_send method-name navigation must resolve to the exact Java implementation source; \
             targets: {run_targets:?}"
    );
    for (constant, expected) in [
        ("Admin::RUN_RESULT", RubyType::nil_class()),
        (
            "Admin::RUN_HANDLE",
            RubyType::Class(FullyQualifiedName::try_from("Method").unwrap()),
        ),
        (
            "Admin::UNBOUND_RUN",
            RubyType::Class(FullyQualifiedName::try_from("UnboundMethod").unwrap()),
        ),
    ] {
        let constant = FullyQualifiedName::try_from(constant).unwrap();
        let indexed_types = AnalysisQuery::new(&engine).type_facts_in_file(source_file);
        assert!(
            indexed_types.iter().any(|fact| {
                fact.subject == TypeSubject::Constant(constant.clone())
                    && fact.ruby_type == expected
            }),
            "{constant} must retain the selected JRuby dispatch type {expected}; \
                 indexed types: {indexed_types:?}"
        );
    }
    let document = server
        .documents
        .read()
        .get(&uri)
        .cloned()
        .expect("processed JRuby document must exist");
    let query = crate::query::EngineQuery::with_doc_and_engine(
        document,
        server.analysis_engine_for_uri(&uri),
    );
    let indexed_types = AnalysisQuery::new(&engine).type_facts_in_file(source_file);
    let rich_constant = FullyQualifiedName::try_from("Admin::RICH").unwrap();
    let rich_proxy = FullyQualifiedName::try_from("Java::Fixtures::RichFixture").unwrap();
    let rich_proxy_namespace = FullyQualifiedName::namespace(rich_proxy.namespace_parts().to_vec());
    assert!(
        indexed_types.iter().any(|fact| {
            fact.subject == TypeSubject::Constant(rich_constant.clone())
                && fact.ruby_type == RubyType::Class(rich_proxy.clone())
        }),
        "the imported Java constructor assignment must retain its canonical proxy type; \
             indexed types: {indexed_types:?}"
    );
    let combine_method = ruby_analysis::core::RubyMethod::new("combine").unwrap();
    let combine_return = AnalysisQuery::new(&engine)
        .method_return_type_for_receiver(&rich_proxy_namespace, &combine_method);
    assert_eq!(
        combine_return,
        Some(RubyType::Class(
            FullyQualifiedName::try_from("Java::JavaUtil::List").unwrap()
        )),
        "the exact classfile-derived method return must be queryable on the Java proxy"
    );
    let direct_line = source
        .lines()
        .position(|line| line.contains("DIRECT = RICH.combine"))
        .unwrap();
    let direct_character = source
        .lines()
        .nth(direct_line)
        .unwrap()
        .find("combine")
        .unwrap();
    let hover = query
        .get_hover_at_position(
            &uri,
            tower_lsp::lsp_types::Position::new(
                u32::try_from(direct_line).unwrap(),
                u32::try_from(direct_character + 1).unwrap(),
            ),
            source,
        )
        .expect("a Java proxy method call must produce hover information");
    assert!(
        hover.content.contains("Java::JavaUtil::List"),
        "Java proxy hover must expose the classfile-derived return type, got: {}",
        hover.content
    );
    drop(engine);

    coordinator
        .file_processor
        .as_ref()
        .unwrap()
        .process_file_current_file_resolution_forced(&uri, "module Admin\nend\n", &server)
        .unwrap();
    let engine = server.analysis_engine_for_uri(&uri);
    let engine = engine.read();
    assert!(
        AnalysisQuery::new(&engine)
            .symbols_for_fqn(&alias)
            .is_empty(),
        "removing java_import must remove its file-owned alias through ordinary replacement"
    );
    let java_alias = FullyQualifiedName::method(
        ["Java", "Fixtures", "RichFixture"]
            .into_iter()
            .map(|part| ruby_analysis::core::RubyConstant::new(part).unwrap())
            .collect::<Vec<_>>(),
        ruby_analysis::core::RubyMethod::new("merged").unwrap(),
    );
    assert!(
        AnalysisQuery::new(&engine)
            .methods_for_fqn(&java_alias)
            .is_empty(),
        "removing java_alias must remove its proxy-owned method through ordinary replacement"
    );
    assert!(
        AnalysisQuery::new(&engine)
            .resolved_reference_definition_ranges_at(source_file, run_offset)
            .is_empty(),
        "removing JRuby dispatch calls must remove their file-owned Java method candidates"
    );
}

#[tokio::test]
async fn adding_a_java_import_after_cold_index_materializes_navigation_inputs_on_demand() {
    let fixture = TempDir::new().unwrap();
    let root = fixture.path().join("admin");
    let jruby_home = fixture.path().join("jruby-9.2.21.0");
    let java_home = fixture.path().join("jdk");
    fs::create_dir_all(root.join("lib")).unwrap();
    fs::create_dir_all(jruby_home.join("bin")).unwrap();
    fs::create_dir_all(java_home.join("jmods")).unwrap();
    fs::write(jruby_home.join("bin/jruby"), b"fixture").unwrap();
    fs::write(java_home.join("release"), "JAVA_VERSION=\"17.0.12\"\n").unwrap();
    write_jar(
        &root.join("lib/rich.jar"),
        "fixtures/RichFixture.class",
        &decode_hex(include_str!(
            "../../../../crates/jvm-metadata/fixtures/rich_fixture.class.hex"
        )),
    );
    write_jar(
        &root.join("lib/rich-sources.jar"),
        "fixtures/RichFixture.java",
        include_str!("../../../../crates/jvm-metadata/fixtures/sources/RichFixture.java")
            .as_bytes(),
    );

    let root_string = format!("{}/", root.to_string_lossy());
    let mut config = RubyFastLspConfig {
        runtime: RuntimeSelectionConfig {
            mode: RuntimeMode::Auto,
            projects: vec![ProjectRuntimeSelection {
                root: root_string.clone(),
                selection: RuntimeSelection::Explicit(SelectedRuntimeDescriptor {
                    implementation: RuntimeImplementation::Jruby,
                    family: "9.2".to_string(),
                    engine_version: "9.2.21.0".to_string(),
                    compatibility_version: "2.5".to_string(),
                    executable: jruby_home.join("bin/jruby"),
                    discovery_source: RuntimeDiscoverySource::Rvm,
                    java_home: Some(java_home),
                }),
            }],
        },
        ..RubyFastLspConfig::default()
    };
    config.jruby.projects = vec![ProjectJrubyConfig {
        root: root_string,
        additional_classpath: vec!["lib/rich.jar".to_string()],
        additional_sources: Vec::new(),
    }];

    let server = RubyLanguageServer::default();
    server.add_workspace(Url::from_directory_path(&root).unwrap());
    let mut coordinator = IndexingCoordinator::new(root.clone(), config);
    coordinator.set_cache_root(fixture.path().join("user-cache"));
    coordinator.detect_ruby_version();
    coordinator.setup_jruby_import_provider().unwrap();
    let signature_cache = coordinator
        .jruby_signature_cache_root(
            coordinator
                .jruby_import_provider
                .as_ref()
                .expect("fixture JRuby provider must exist"),
        )
        .unwrap();
    server.set_jruby_import_provider(&root, coordinator.jruby_import_provider.clone());

    let source_path = root.join("imports.rb");
    let uri = Url::from_file_path(&source_path).unwrap();
    let initial = "VALUE = 1\n";
    fs::write(&source_path, initial).unwrap();
    crate::capabilities::indexing::handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: initial.to_string(),
            },
        },
    )
    .await;
    assert!(
        !signature_cache.join("fixtures/RichFixture.rb").exists(),
        "cold indexing without a Java dependency must not eagerly materialize its signature"
    );

    let added = "java_import fixtures.RichFixture\nRICH = RichFixture.new(nil)\n";
    crate::capabilities::indexing::handle_did_change(
        &server,
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version: 2,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: added.to_string(),
            }],
        },
    )
    .await;
    assert!(
        signature_cache.join("fixtures/RichFixture.rb").is_file(),
        "adding a static import must materialize its signature without restarting the project"
    );
    let engine = server.analysis_engine_for_uri(&uri);
    let engine = engine.read();
    let source_file = AnalysisQuery::new(&engine).file_id(&source_path).unwrap();
    let constructor_offset =
        u32::try_from(added.find("RichFixture.new").unwrap() + "RichFixture.".len()).unwrap();
    let targets = AnalysisQuery::new(&engine)
        .resolved_reference_definition_ranges_at(source_file, constructor_offset);
    assert!(
        targets.iter().any(|target| {
            AnalysisQuery::new(&engine)
                .file(target.file_id)
                .is_some_and(|file| {
                    file.kind == ruby_analysis::core::SourceKind::External
                        && file.path.ends_with("fixtures/RichFixture.java")
                })
        }),
        "a newly added import must navigate to its exact Java source in the same edit pass; \
             targets: {targets:?}"
    );
    let rich = FullyQualifiedName::try_from("RICH").unwrap();
    let expected_rich_type =
        RubyType::Class(FullyQualifiedName::try_from("Java::Fixtures::RichFixture").unwrap());
    let indexed_types = AnalysisQuery::new(&engine).type_facts_in_file(source_file);
    assert!(
        indexed_types.iter().any(|fact| {
            fact.subject == TypeSubject::Constant(rich.clone())
                && fact.ruby_type == expected_rich_type
        }),
        "an imported Java constructor assignment must retain the canonical proxy instance \
             type during the same edit pass; indexed types: {indexed_types:?}"
    );
    drop(engine);

    crate::capabilities::indexing::handle_did_change(
        &server,
        DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: uri.clone(),
                version: 3,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: initial.to_string(),
            }],
        },
    )
    .await;
    let engine = server.analysis_engine_for_uri(&uri);
    let engine = engine.read();
    assert!(
        AnalysisQuery::new(&engine)
            .resolved_reference_definition_ranges_at(source_file, constructor_offset)
            .is_empty(),
        "removing the newly added import and constructor call must clear their reference facts"
    );
    let resources = server.indexing.resources().snapshot();
    assert_eq!(
        resources.completed_tasks, 3,
        "didOpen plus two didChange passes must each own exactly one outer resource lease; \
             interactive JRuby signature/source materialization must not acquire nested admission"
    );
    assert_eq!(resources.active_tasks, 0);
    assert_eq!(resources.queued_tasks, 0);
}
