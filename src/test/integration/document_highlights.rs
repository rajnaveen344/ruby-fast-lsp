use crate::server::RubyLanguageServer;
use crate::test::harness::FakeEditor;
use tower_lsp::lsp_types::{InitializeParams, OneOf, Position, Range};
use tower_lsp::LanguageServer;

#[tokio::test]
async fn initialization_advertises_document_highlights() {
    let server = RubyLanguageServer::default();
    let initialized = server
        .initialize(InitializeParams::default())
        .await
        .expect("server initialization should succeed");

    assert_eq!(
        initialized.capabilities.document_highlight_provider,
        Some(OneOf::Left(true))
    );
}

#[tokio::test]
async fn highlights_constant_occurrences_in_current_document_only() {
    let mut editor = FakeEditor::new().await;
    editor
        .open("main.rb", "class User\nend\nUser.new\nUser.new\n")
        .await;
    editor.open("other.rb", "User.new\n").await;

    let highlights = editor.document_highlights_at("main.rb", 2, 1).await;
    let ranges = highlights
        .into_iter()
        .map(|highlight| highlight.range)
        .collect::<Vec<_>>();

    assert_eq!(
        ranges,
        vec![
            Range::new(Position::new(0, 6), Position::new(0, 10)),
            Range::new(Position::new(2, 0), Position::new(2, 4)),
            Range::new(Position::new(3, 0), Position::new(3, 4)),
        ]
    );
}

#[tokio::test]
async fn highlights_local_and_method_occurrences() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "main.rb",
            "class User\n  def name\n    value = 1\n    value.to_s\n  end\nend\nUser.new.name\n",
        )
        .await;

    let local = editor.document_highlights_at("main.rb", 3, 5).await;
    assert_eq!(local.len(), 2);
    assert_eq!(local[0].range.start, Position::new(2, 4));
    assert_eq!(local[1].range.start, Position::new(3, 4));

    let method = editor.document_highlights_at("main.rb", 1, 7).await;
    assert_eq!(method.len(), 1);
    assert_eq!(method[0].range.start, Position::new(6, 9));
}

#[tokio::test]
async fn highlights_refresh_after_document_change() {
    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", "VALUE = 1\nputs VALUE\n").await;
    assert_eq!(
        editor.document_highlights_at("main.rb", 1, 6).await.len(),
        1
    );

    editor
        .set("main.rb", "VALUE = 1\nputs VALUE\nwarn VALUE\n")
        .await;
    let highlights = editor.document_highlights_at("main.rb", 2, 6).await;

    assert_eq!(highlights.len(), 2);
    assert_eq!(highlights[1].range.start, Position::new(2, 5));
}
