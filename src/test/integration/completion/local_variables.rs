use crate::test::harness::{check, FakeEditor};
use tower_lsp::lsp_types::CompletionItemKind;

#[tokio::test]
async fn local_variable_name_is_suggested_at_a_bare_call() {
    check(
        r#"
dimensions = { width: 12.5, height: 8.0 }
height = dimensions[:height]
hei$0
<complete items="height">
"#,
    )
    .await;
}

#[tokio::test]
async fn local_variable_completion_does_not_require_a_known_type() {
    check(
        r#"
answer = read_input
ans$0
<complete items="answer">
"#,
    )
    .await;
}

#[tokio::test]
async fn local_variable_completion_follows_unsaved_typing() {
    let mut editor = FakeEditor::new().await;
    let source = "dimensions = { width: 12.5, height: 8.0 }\nheight = dimensions[:height]\n";
    editor.open("dimensions.rb", source).await;

    for partial in ["h", "he", "hei"] {
        editor
            .set("dimensions.rb", &format!("{source}{partial}"))
            .await;
        let items = editor
            .complete_at("dimensions.rb", 2, partial.len() as u32)
            .await;
        assert!(
            items
                .iter()
                .any(|item| item.label == "height"
                    && item.kind == Some(CompletionItemKind::VARIABLE)),
            "typing {partial:?} must offer the visible local variable: {items:?}"
        );
    }
}

#[tokio::test]
async fn completion_waits_for_the_in_flight_document_edit() {
    use crate::capabilities::indexing::handle_did_change;
    use crate::indexer::test_schedule::Point;
    use tower_lsp::lsp_types::{
        DidChangeTextDocumentParams, TextDocumentContentChangeEvent,
        VersionedTextDocumentIdentifier,
    };

    let mut editor = FakeEditor::new().await;
    editor.open("locals.rb", "previous = 1\npre").await;
    let uri = crate::test::harness::fixture_uri("/locals.rb");
    let mut pause = editor
        .server()
        .indexing
        .schedule
        .arm(Point::DocumentSourceUpdated, uri.to_file_path().unwrap());
    let server = editor.server().clone();
    let edit = tokio::spawn(async move {
        handle_did_change(
            &server,
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier { uri, version: 2 },
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: "current = read_input\ncur".to_string(),
                }],
            },
        )
        .await;
    });
    pause.wait().await;

    let completion = editor.complete_at("locals.rb", 1, 3);
    tokio::pin!(completion);
    assert!(
        futures::poll!(&mut completion).is_pending(),
        "completion must wait while an accepted edit is replacing its source and local scopes"
    );
    pause.release();
    edit.await.unwrap();

    let mut locals = completion
        .await
        .into_iter()
        .filter(|item| item.kind == Some(CompletionItemKind::VARIABLE))
        .map(|item| item.label)
        .collect::<Vec<_>>();
    locals.sort();
    assert_eq!(
        locals,
        ["current"],
        "completion must use only the edited scope, even when the variable type is unknown"
    );
}
