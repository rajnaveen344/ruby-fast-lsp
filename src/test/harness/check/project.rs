//! Inline fixtures written to disk and cold-indexed as a real project.

use std::time::Duration;

use tower_lsp::lsp_types::Url;

use super::{run_fixture_checks, FixtureFile};
use crate::indexer::scheduling::status::IndexingPhase;
use crate::lsp::capabilities::indexing::init_workspace_for_run;
use crate::test::harness::fake_editor::FakeEditor;
use crate::test::harness::fixture::parse_fixture;

/// Write every fixture under a temporary project root, cold-index that root
/// the way a workspace folder is indexed (bundled core included, no Ruby
/// runtime), open every file, then run the assertions of all of them as one
/// scenario. Names are relative to the project root.
///
/// Use this instead of `check_multi_file()` when the behavior depends on core
/// classes and modules such as `StandardError` or `Enumerable`, which an
/// in-memory fixture outside a workspace does not see.
pub async fn check_project(files: &[(&str, &str)]) {
    assert!(
        !files.is_empty(),
        "check_project requires at least one file"
    );
    let fixture_dir = tempfile::tempdir().expect("create project fixture directory");
    let base = fixture_dir
        .path()
        .canonicalize()
        .expect("canonicalize project fixture directory");
    let root = base.join("project");
    let parsed = files
        .iter()
        .map(|(name, text)| {
            let path = root.join(name);
            let fixture = parse_fixture(text);
            std::fs::create_dir_all(path.parent().expect("fixture files live under the root"))
                .expect("create fixture subdirectory");
            std::fs::write(&path, &fixture.source).expect("write project fixture");
            (path, fixture)
        })
        .collect::<Vec<_>>();

    let mut editor = FakeEditor::with_cache_root(base.join("cache")).await;
    let server = editor.server().clone();
    server.set_discovered_runtimes_for_tests(Vec::new());
    let root_uri = Url::from_directory_path(&root).expect("project root is an absolute directory");
    let workspace = server.add_workspace(root_uri.clone());
    let run = workspace.begin_indexing_run();
    tokio::time::timeout(
        Duration::from_secs(30),
        init_workspace_for_run(&server, root_uri, run.clone()),
    )
    .await
    .expect("cold indexing must finish")
    .expect("cold indexing must succeed");
    workspace
        .indexing_status
        .transition(run.generation(), IndexingPhase::Ready, None, None)
        .expect("mark the indexed project ready");

    let mut fixtures = Vec::new();
    for (path, fixture) in parsed {
        let name = path.to_string_lossy().replace('\\', "/");
        editor.open(&name, &fixture.source).await;
        fixtures.push(FixtureFile {
            uri: FakeEditor::filename_to_uri(&name),
            fixture,
        });
    }
    run_fixture_checks(editor.server(), &fixtures).await;
}
