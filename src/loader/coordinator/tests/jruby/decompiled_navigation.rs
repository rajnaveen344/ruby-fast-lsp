//! Verified decompiled navigation for source-less JRuby imports.

use super::*;

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
#[tokio::test]
async fn source_less_jruby_import_navigates_to_verified_decompiled_implementation() {
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
        "../../../../../crates/jvm-metadata/fixtures/rich_fixture.class.hex"
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
    select_ruby_version(&mut coordinator, &server).await;
    coordinator.setup_jruby_import_provider().unwrap();
    coordinator
        .setup_file_processor(&server.load_context_for_project(coordinator.workspace_root()));
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
    project_indexer
        .collect_project_facts(
            &server.load_context_for_project(project_indexer.workspace_root()),
            &server,
        )
        .unwrap();
    let uri = Url::from_file_path(&source_path).unwrap();
    coordinator
        .file_processor
        .as_ref()
        .unwrap()
        .process_file(&uri, source, &server.load_context_for_uri(&uri), &server)
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
