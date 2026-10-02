//! Server-driven extension workspace lifecycle: trusted project-local
//! discovery at initialization, reconfiguration on workspace-folder changes,
//! and routing of matching watched-file changes to a manifest extension.

use tempfile::TempDir;
use tower_lsp::lsp_types::{
    DidChangeWatchedFilesParams, DidChangeWorkspaceFoldersParams, FileChangeType, FileEvent,
    InitializeParams, Url, WorkspaceFolder, WorkspaceFoldersChangeEvent,
};
use tower_lsp::LanguageServer;

use crate::environment::config::RubyFastLspConfig;
use crate::environment::extensions::tests::{
    copy_rspec_package, write_watched_file_failure_package,
};
use crate::server::RubyLanguageServer;

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

    crate::lsp::lifecycle::notification::handle_did_change_watched_files(
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

    crate::lsp::lifecycle::notification::handle_did_change_watched_files(
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

fn project_holds_rspec_describe(server: &RubyLanguageServer, root_uri: &Url) -> bool {
    server.project_for_uri(root_uri).view(|view| {
        view.all_method_facts().iter().any(|fact| {
            matches!(
                &fact.fqn,
                ruby_analysis::core::FullyQualifiedName::Method(namespace, method)
                    if namespace.len() == 1
                        && namespace[0].to_string() == "RSpec"
                        && method.as_str() == "describe"
            )
        })
    })
}

#[tokio::test]
async fn runtime_rebuild_restores_extension_semantic_seed() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    std::fs::write(temp_dir.path().join("Gemfile"), "gem \"rspec-core\"\n")
        .expect("test Gemfile must be written");
    std::fs::write(
        temp_dir.path().join("Gemfile.lock"),
        "GEM\n  specs:\n    rspec-core (3.13.1)\n",
    )
    .expect("test lockfile must be written");
    std::fs::create_dir_all(temp_dir.path().join("spec")).expect("spec dir must be created");
    let spec_path = temp_dir.path().join("spec/example_spec.rb");
    let spec_source = "RSpec.describe \"example\" do\nend\n";
    std::fs::write(&spec_path, spec_source).expect("test spec must be written");
    let root_uri = Url::from_directory_path(temp_dir.path())
        .expect("test workspace path must convert to a file URI");
    let package = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("extensions/rspec-ruby");
    let config = RubyFastLspConfig {
        extension_packages: vec![package.to_string_lossy().into_owned()],
        ..RubyFastLspConfig::default()
    };
    let server = RubyLanguageServer::default();
    *server.config.lock() = config.clone();
    server.add_workspace(root_uri.clone());
    server.extensions.registry().configure_from_config(&config);
    crate::lsp::lifecycle::notification::handle_did_open(
        &server,
        tower_lsp::lsp_types::DidOpenTextDocumentParams {
            text_document: tower_lsp::lsp_types::TextDocumentItem {
                uri: Url::from_file_path(&spec_path).expect("spec path must convert to URI"),
                language_id: "ruby".to_string(),
                version: 1,
                text: spec_source.to_string(),
            },
        },
    )
    .await;
    assert!(
        project_holds_rspec_describe(&server, &root_uri),
        "an applicable project must hold the extension semantic seed after a file pass"
    );

    crate::lsp::lifecycle::notification::handle_did_change_watched_files(
        &server,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent::new(
                Url::from_file_path(temp_dir.path().join("Gemfile.lock"))
                    .expect("test lockfile path must convert to URI"),
                FileChangeType::CHANGED,
            )],
        },
    )
    .await;

    assert!(
        project_holds_rspec_describe(&server, &root_uri),
        "a runtime rebuild clears the project engine, so it must seed the extension semantics again"
    );
}
