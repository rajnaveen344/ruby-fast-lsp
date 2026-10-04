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

#[tokio::test]
async fn position_queries_wait_for_the_in_flight_open_of_an_indexed_file() {
    use crate::loader::scheduling::test_schedule::Point;
    use crate::lsp::lifecycle::indexing::handle_did_open;
    use tower_lsp::lsp_types::{
        DidOpenTextDocumentParams, GotoDefinitionParams, GotoDefinitionResponse, HoverParams,
        Position, SignatureHelpParams, TextDocumentIdentifier, TextDocumentItem,
        TextDocumentPositionParams,
    };
    use tower_lsp::LanguageServer;

    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().canonicalize().unwrap().join("project");
    std::fs::create_dir(&root).unwrap();
    let path = root.join("box.rb");
    std::fs::write(&path, SOURCE).unwrap();
    let root_uri = Url::from_directory_path(&root).unwrap();
    let editor = FakeEditor::with_cache_root(fixture.path().join("cache")).await;
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

    let uri = Url::from_file_path(&path).unwrap();
    let mut pause = server
        .test_schedule()
        .arm(Point::DocumentSourceUpdated, path.clone());
    let opening = {
        let server = server.clone();
        let uri = uri.clone();
        tokio::spawn(async move {
            handle_did_open(
                &server,
                DidOpenTextDocumentParams {
                    text_document: TextDocumentItem {
                        uri,
                        language_id: "ruby".to_string(),
                        version: 1,
                        text: SOURCE.to_string(),
                    },
                },
            )
            .await;
        })
    };
    pause.wait().await;

    let at = |line, character| TextDocumentPositionParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        position: Position::new(line, character),
    };
    let definition = server.goto_definition(GotoDefinitionParams {
        text_document_position_params: at(2, 13),
        work_done_progress_params: Default::default(),
        partial_result_params: Default::default(),
    });
    let hover = server.hover(HoverParams {
        text_document_position_params: at(7, 0),
        work_done_progress_params: Default::default(),
    });
    let signature = server.signature_help(SignatureHelpParams {
        text_document_position_params: at(6, 14),
        work_done_progress_params: Default::default(),
        context: None,
    });
    tokio::pin!(definition, hover, signature);
    assert!(
        futures::poll!(&mut definition).is_pending(),
        "definition must wait while the opened file's facts are being replaced"
    );
    assert!(
        futures::poll!(&mut hover).is_pending(),
        "hover must wait while the opened file's facts are being replaced"
    );
    assert!(
        futures::poll!(&mut signature).is_pending(),
        "signature help must wait while the opened file's facts are being replaced"
    );
    pause.release();
    opening.await.unwrap();

    let Some(GotoDefinitionResponse::Array(definitions)) = definition.await.unwrap() else {
        panic!("the `size` read must navigate to one location list");
    };
    assert_eq!(
        definitions
            .iter()
            .map(|location| (location.range.start.line, location.range.start.character))
            .collect::<Vec<_>>(),
        vec![(1, 17)],
        "the `size` read must navigate to its keyword parameter"
    );
    let hover = hover.await.unwrap().expect("hover on `box`");
    let HoverContents::Markup(markup) = hover.contents else {
        panic!("hover must be markdown: {hover:?}");
    };
    assert!(
        markup.value.contains("box: Box # local variable"),
        "the local must keep its inferred type: {}",
        markup.value
    );
    let signature = signature
        .await
        .unwrap()
        .expect("signature help in `Box.new(`");
    assert!(
        signature
            .signatures
            .iter()
            .any(|candidate| candidate.label.contains("size:")),
        "`Box.new` must offer the keyword of `initialize`: {signature:?}"
    );
}
