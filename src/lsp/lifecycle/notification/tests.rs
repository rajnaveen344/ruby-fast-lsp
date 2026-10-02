use super::configuration::extension_watch_registration;
use super::watched_files::project_input_change_requires_rebuild;
use super::*;
use crate::environment::config::runtime::{
    ProjectRuntimeSelection, RuntimeMode, RuntimeSelection, RuntimeSelectionConfig,
    SelectedRuntimeDescriptor,
};
use crate::environment::config::RubyFastLspConfig;
use crate::environment::runtime::catalog::RuntimeDiscoverySource;
use crate::environment::runtime::catalog::RuntimeImplementation;
use ruby_analysis::core::{FullyQualifiedName, RubyConstant, SourceKind};
use ruby_analysis::engine::{AnalysisQuery, SourceFileInput};
use std::io::{Cursor, Write};
use std::path::PathBuf;
use zip::write::SimpleFileOptions;

fn decode_hex(source: &str) -> Vec<u8> {
    let digits = source
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect::<Vec<_>>();
    digits
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[tokio::test]
async fn shutdown_cancels_every_project_indexing_generation() {
    let fixture = tempfile::tempdir().unwrap();
    let server = RubyLanguageServer::default();
    let first =
        server.add_workspace(Url::from_directory_path(fixture.path().join("admin")).unwrap());
    let second =
        server.add_workspace(Url::from_directory_path(fixture.path().join("server")).unwrap());
    let first_run = first.indexing_status.begin_run();
    let second_run = second.indexing_status.begin_run();
    let pending_watcher_generation = server.queue_watched_file_changes(vec![FileEvent {
        uri: Url::from_file_path(fixture.path().join("pending.rb")).unwrap(),
        typ: FileChangeType::CHANGED,
    }]);

    handle_shutdown(&server)
        .await
        .expect("test server shutdown must succeed");

    assert!(first_run.is_cancelled());
    assert!(second_run.is_cancelled());
    assert_eq!(
        first.indexing_status.snapshot().phase,
        crate::loader::scheduling::status::IndexingPhase::Cancelled
    );
    assert_eq!(
        second.indexing_status.snapshot().phase,
        crate::loader::scheduling::status::IndexingPhase::Cancelled
    );
    assert!(
        server
            .take_watched_file_changes(pending_watcher_generation)
            .is_none(),
        "shutdown must invalidate pending watcher work"
    );
}

#[tokio::test]
async fn watcher_storm_processes_only_the_newest_complete_batch() {
    let fixture = tempfile::tempdir().unwrap();
    let project = fixture.path().join("server");
    let source_path = project.join("lib/service.rb");
    std::fs::create_dir_all(source_path.parent().unwrap()).unwrap();
    std::fs::write(&source_path, "class StaleService\nend\n").unwrap();
    let source_uri = Url::from_file_path(&source_path).unwrap();
    let server = RubyLanguageServer::default();
    let workspace = server.add_workspace(Url::from_directory_path(&project).unwrap());

    let first_server = server.clone();
    let first_uri = source_uri.clone();
    let first = tokio::spawn(async move {
        handle_did_change_watched_files(
            &first_server,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent {
                    uri: first_uri,
                    typ: FileChangeType::CREATED,
                }],
            },
        )
        .await;
    });
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    std::fs::write(&source_path, "class CurrentService\nend\n").unwrap();
    handle_did_change_watched_files(
        &server,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: source_uri,
                typ: FileChangeType::CHANGED,
            }],
        },
    )
    .await;
    first.await.unwrap();

    let stale = FullyQualifiedName::namespace(vec![RubyConstant::new("StaleService").unwrap()]);
    let current = FullyQualifiedName::namespace(vec![RubyConstant::new("CurrentService").unwrap()]);
    let engine = workspace.handle().test_read();
    let query = AnalysisQuery::new(&engine);
    assert!(query.symbol_facts_for(&stale).is_empty());
    assert_eq!(query.symbol_facts_for(&current).len(), 1);
}

fn write_jar(path: &std::path::Path, entry: &str, contents: &[u8]) {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file(entry, SimpleFileOptions::default())
        .unwrap();
    writer.write_all(contents).unwrap();
    let bytes = writer.finish().unwrap().into_inner();
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn extension_watch_registration_is_sorted_typed_lsp_registration() {
    let registration = extension_watch_registration(&[
        "config/**/*.yml".to_string(),
        ".rubocop.yml".to_string(),
        "config/**/*.yml".to_string(),
    ]);

    assert_eq!(registration.id, "ruby-fast-lsp-extension-watchers");
    assert_eq!(registration.method, "workspace/didChangeWatchedFiles");
    assert_eq!(
        registration.register_options,
        Some(serde_json::json!({
            "watchers": [
                {"globPattern": ".rubocop.yml"},
                {"globPattern": "config/**/*.yml"}
            ]
        }))
    );
}

#[test]
fn project_inputs_trigger_only_the_owning_project_rebuild() {
    let project = PathBuf::from("/repo/admin");
    let mut config = RubyFastLspConfig {
        runtime: RuntimeSelectionConfig {
            mode: RuntimeMode::Auto,
            projects: vec![ProjectRuntimeSelection {
                root: project.to_string_lossy().to_string(),
                selection: RuntimeSelection::Explicit(SelectedRuntimeDescriptor {
                    implementation: RuntimeImplementation::Jruby,
                    family: "9.2".to_string(),
                    engine_version: "9.2.21.0".to_string(),
                    compatibility_version: "2.5".to_string(),
                    executable: PathBuf::from("/runtimes/jruby-9.2.21.0/bin/jruby"),
                    discovery_source: RuntimeDiscoverySource::Rvm,
                    java_home: Some(PathBuf::from("/jdk/17")),
                }),
            }],
        },
        ..RubyFastLspConfig::default()
    };

    for changed in [
        "Gemfile",
        "Gemfile.lock",
        "Jarfile",
        "Jars.lock",
        "lib/jars/runtime.jar",
        "src/main/java/com/example/Runtime.java",
    ] {
        assert!(
            project_input_change_requires_rebuild(&project, &project.join(changed), &config),
            "{changed} must rebuild the owning project state"
        );
    }
    assert!(!project_input_change_requires_rebuild(
        &project,
        PathBuf::from("/repo/server/lib/jars/runtime.jar").as_path(),
        &config
    ));
    assert!(!project_input_change_requires_rebuild(
        &project,
        &project.join("lib/application.rb"),
        &config
    ));
    assert!(!project_input_change_requires_rebuild(
        &project,
        &project.join(".ruby-version"),
        &config
    ));

    config.runtime.projects[0].selection = RuntimeSelection::Explicit(SelectedRuntimeDescriptor {
        implementation: RuntimeImplementation::Mri,
        family: "3.3".to_string(),
        engine_version: "3.3.11".to_string(),
        compatibility_version: "3.3".to_string(),
        executable: PathBuf::from("/runtimes/ruby-3.3.11/bin/ruby"),
        discovery_source: RuntimeDiscoverySource::Rvm,
        java_home: None,
    });
    for changed in ["Gemfile", "Gemfile.lock"] {
        assert!(project_input_change_requires_rebuild(
            &project,
            &project.join(changed),
            &config
        ));
    }
    assert!(!project_input_change_requires_rebuild(
        &project,
        &project.join("lib/jars/runtime.jar"),
        &config
    ));
    assert!(!project_input_change_requires_rebuild(
        &project,
        &project.join(".ruby-version"),
        &config
    ));

    config.runtime.projects[0].selection =
        RuntimeSelection::Mode(crate::environment::config::runtime::RuntimeSelectionMode::Auto);
    for marker in [".ruby-version", ".tool-versions"] {
        assert!(project_input_change_requires_rebuild(
            &project,
            &project.join(marker),
            &config
        ));
    }
    assert!(!project_input_change_requires_rebuild(
        &project,
        &project.join("config/.ruby-version"),
        &config
    ));
    assert!(!project_input_change_requires_rebuild(
        &project,
        &project.join(".ruby-fast-lsp/extensions/custom/extension.wasm"),
        &config
    ));
    config.workspace_trusted = true;
    for extension_input in [
        ".ruby-fast-lsp/extensions/custom/extension.toml",
        ".ruby-fast-lsp/extensions/custom/extension.wasm",
        "ruby_fast_lsp/frameworks/custom/extension.toml",
    ] {
        assert!(project_input_change_requires_rebuild(
            &project,
            &project.join(extension_input),
            &config
        ));
    }
    assert!(!project_input_change_requires_rebuild(
        &project,
        &project.join(".ruby-fast-lsp/config.toml"),
        &config
    ));
}

#[tokio::test]
async fn classpath_change_clears_external_facts_and_reopens_project_documents_on_failure() {
    let fixture = tempfile::tempdir().unwrap();
    let project = fixture.path().join("admin");
    std::fs::create_dir_all(project.join("lib/jars")).unwrap();
    std::fs::write(project.join("Gemfile"), "").unwrap();
    let source_path = project.join("app.rb");
    let source = "VALUE = Java::ComExample::Runtime.new\n";
    std::fs::write(&source_path, source).unwrap();
    let project_uri = Url::from_directory_path(&project).unwrap();
    let source_uri = Url::from_file_path(&source_path).unwrap();
    let external_path = fixture.path().join("cache/Runtime.java");
    std::fs::create_dir_all(external_path.parent().unwrap()).unwrap();
    std::fs::write(&external_path, "package com.example; class Runtime {}\n").unwrap();
    let external_uri = Url::from_file_path(&external_path).unwrap();

    let server = RubyLanguageServer::default();
    let workspace = server.add_workspace(project_uri);
    *server.config.lock() = RubyFastLspConfig {
        runtime: RuntimeSelectionConfig {
            mode: RuntimeMode::Auto,
            projects: vec![ProjectRuntimeSelection {
                root: project.to_string_lossy().to_string(),
                selection: RuntimeSelection::Explicit(SelectedRuntimeDescriptor {
                    implementation: RuntimeImplementation::Jruby,
                    family: "9.2".to_string(),
                    engine_version: "9.2.21.0".to_string(),
                    compatibility_version: "2.5".to_string(),
                    executable: fixture.path().join("missing-jruby/bin/jruby"),
                    discovery_source: RuntimeDiscoverySource::Rvm,
                    java_home: Some(fixture.path().join("missing-jdk")),
                }),
            }],
        },
        ..RubyFastLspConfig::default()
    };
    handle_did_open(
        &server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: source_uri.clone(),
                language_id: "ruby".to_string(),
                version: 1,
                text: source.to_string(),
            },
        },
    )
    .await;
    workspace
        .handle()
        .test_write()
        .register_file(SourceFileInput {
            path: external_path.clone(),
            content: "package com.example; class Runtime {}\n".to_string(),
            kind: SourceKind::External,
        });
    server.retain_external_document_project(&external_uri, &workspace);
    assert!(workspace
        .handle()
        .test_read()
        .view()
        .file_id(&external_path)
        .is_some());

    let generation_before_rebuild = workspace.indexing_status.snapshot().generation;
    let jar_uri = Url::from_file_path(project.join("lib/jars/runtime.jar")).unwrap();
    handle_did_change_watched_files(
        &server,
        DidChangeWatchedFilesParams {
            changes: vec![
                FileEvent {
                    uri: jar_uri.clone(),
                    typ: FileChangeType::CHANGED,
                },
                FileEvent {
                    uri: jar_uri,
                    typ: FileChangeType::DELETED,
                },
            ],
        },
    )
    .await;

    let engine = workspace.handle().test_read();
    assert!(
        engine.view().file_id(&external_path).is_none(),
        "runtime rebuild must remove stale external implementation facts"
    );
    assert!(
        engine.view().file_id(&source_path).is_some(),
        "open project documents must be restored even when runtime setup fails closed"
    );
    drop(engine);
    assert!(
        server.analysis_workspace_for_uri(&external_uri).is_none(),
        "runtime rebuild must release retained provenance for stale external documents"
    );
    assert_eq!(
        workspace.indexing_status.snapshot().phase,
        crate::loader::scheduling::status::IndexingPhase::Failed
    );
    assert_eq!(
        workspace.indexing_status.snapshot().generation,
        generation_before_rebuild + 1,
        "duplicate watcher events for one runtime input must create one replacement generation"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn changed_winning_jar_replaces_decompiled_navigation_without_stale_facts() {
    use crate::environment::config::runtime::ProjectJrubyConfig;
    use std::os::unix::fs::symlink;

    let _decompiler_budget = crate::test::harness::isolate_decompiler_budget();
    let fixture = tempfile::tempdir().unwrap();
    let project = fixture.path().join("admin");
    let jruby_home = fixture.path().join("jruby-9.2.21.0");
    let java_home = fixture.path().join("jdk");
    let jar_path = project.join("lib/rich.jar");
    let source_path = project.join("imports.rb");
    std::fs::create_dir_all(project.join("lib")).unwrap();
    std::fs::create_dir_all(jruby_home.join("bin")).unwrap();
    std::fs::create_dir_all(java_home.join("bin")).unwrap();
    std::fs::create_dir_all(java_home.join("jmods")).unwrap();
    std::fs::write(project.join("Gemfile"), "").unwrap();
    std::fs::write(
        jruby_home.join("bin/jruby"),
        "#!/bin/sh\nprintf 'RUBY_FAST_LSP_GEM_DISCOVERY={\"source\":\"global\",\"gems\":[]}\\n'\n",
    )
    .unwrap();
    let permissions = std::os::unix::fs::PermissionsExt::from_mode(0o755);
    std::fs::set_permissions(jruby_home.join("bin/jruby"), permissions).unwrap();
    std::fs::write(java_home.join("release"), "JAVA_VERSION=\"17.0.12\"\n").unwrap();
    let real_java = [
        std::env::var_os("JAVA_HOME")
            .map(PathBuf::from)
            .map(|home| home.join("bin/java")),
        Some(PathBuf::from("/opt/homebrew/opt/openjdk/bin/java")),
        Some(PathBuf::from("/usr/local/opt/openjdk/bin/java")),
    ]
    .into_iter()
    .flatten()
    .find(|candidate| candidate.is_file())
    .expect("JRuby decompiler lifecycle test requires a real JDK java executable");
    symlink(real_java, java_home.join("bin/java")).unwrap();
    let rich_class = decode_hex(include_str!(
        "../../../../crates/jvm-metadata/fixtures/rich_fixture.class.hex"
    ));
    write_jar(&jar_path, "fixtures/RichFixture.class", &rich_class);
    let source = "java_import fixtures.RichFixture\n\
                      RICH = RichFixture.new(nil)\n\
                      VALUE = RICH.java_send(:run, [])\n";
    std::fs::write(&source_path, source).unwrap();

    let project_root = format!("{}/", project.to_string_lossy());
    let mut config = RubyFastLspConfig {
        runtime: RuntimeSelectionConfig {
            mode: RuntimeMode::Auto,
            projects: vec![ProjectRuntimeSelection {
                root: project_root.clone(),
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
        root: project_root,
        additional_classpath: vec!["lib/rich.jar".to_string()],
        additional_sources: Vec::new(),
    }];

    let server = RubyLanguageServer::with_user_cache_root(fixture.path().join("cache"))
        .expect("construct isolated cache server");
    *server.config.lock() = config;
    let project_uri = Url::from_directory_path(&project).unwrap();
    let workspace = server.add_workspace(project_uri.clone());
    indexing::init_workspace(&server, project_uri)
        .await
        .expect("initial JRuby fixture workspace must index");
    let initial_files = workspace
        .handle()
        .test_read()
        .view()
        .files()
        .map(|file| (file.kind, file.path.clone()))
        .collect::<Vec<_>>();
    assert!(
        initial_files.iter().any(|(kind, path)| {
            *kind == SourceKind::External
                && std::fs::read_to_string(path)
                    .is_ok_and(|source| source.contains("return List.of(prefix + values.length);"))
        }),
        "initial runtime index must contain the decompiled implementation: {initial_files:?}"
    );

    let replacement = decode_hex(include_str!(
        "../../../../crates/jvm-metadata/fixtures/minimal_class.hex"
    ));
    write_jar(&jar_path, "com/example/Demo.class", &replacement);
    handle_did_change_watched_files(
        &server,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: Url::from_file_path(&jar_path).unwrap(),
                typ: FileChangeType::CHANGED,
            }],
        },
    )
    .await;

    let engine = workspace.handle().test_read();
    assert!(
        !engine.view().files().any(|file| {
            matches!(file.kind, SourceKind::External | SourceKind::Signature)
                && file.path.to_string_lossy().contains("RichFixture")
        }),
        "changing the winning artifact must remove stale source, decompiled, and signature facts"
    );
    assert!(
        engine.view().file_id(&source_path).is_some(),
        "the project source must be reindexed after the runtime rebuild"
    );
    assert!(workspace.indexing_status.snapshot().is_ready());
}
