//! Opening a file whose disk content was already indexed must give the same
//! local-variable answers as opening it after an edit.

use crate::lsp::lifecycle::indexing::init_workspace_for_run;
use crate::test::harness::FakeEditor;
use std::time::Duration;
use tower_lsp::lsp_types::{HoverContents, Url};

const SOURCE: &str = "class Box\n  def initialize(size:)\n    @size = size\n  end\nend\n\nbox = Box.new(size: 1)\nbox\n";

#[tokio::test]
async fn opening_an_indexed_file_keeps_local_variable_navigation_and_types() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().canonicalize().unwrap().join("project");
    std::fs::create_dir(&root).unwrap();
    let path = root.join("box.rb");
    std::fs::write(&path, SOURCE).unwrap();
    let root_uri = Url::from_directory_path(&root).unwrap();
    let mut editor = FakeEditor::with_cache_root(fixture.path().join("cache")).await;
    let server = editor.server().clone();
    server.set_discovered_runtimes_for_tests(Vec::new());
    let workspace = server.add_workspace(root_uri.clone());
    tokio::time::timeout(
        Duration::from_secs(30),
        init_workspace_for_run(&server, root_uri, workspace.begin_indexing_run()),
    )
    .await
    .expect("cold indexing must finish")
    .expect("cold indexing must succeed");

    let file = path.to_str().unwrap().trim_start_matches('/').to_string();
    editor.open(&file, SOURCE).await;

    let definitions = editor.goto_def_at(&file, 2, 13).await;
    assert_eq!(
        definitions
            .iter()
            .map(|location| (location.range.start.line, location.range.start.character))
            .collect::<Vec<_>>(),
        vec![(1, 17)],
        "the `size` read must navigate to its keyword parameter"
    );
    let hover = editor.hover_at(&file, 7, 0).await.expect("hover on `box`");
    let HoverContents::Markup(markup) = hover.contents else {
        panic!("hover must be markdown: {hover:?}");
    };
    assert!(
        markup.value.contains("box: Box # local variable"),
        "the local must keep its inferred type: {}",
        markup.value
    );
}
