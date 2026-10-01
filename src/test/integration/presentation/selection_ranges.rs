use crate::server::RubyLanguageServer;
use crate::test::harness::FakeEditor;
use tower_lsp::lsp_types::{InitializeParams, Position, Range, SelectionRangeProviderCapability};
use tower_lsp::LanguageServer;

#[tokio::test]
async fn initialization_advertises_selection_ranges() {
    let server = RubyLanguageServer::default();
    let initialized = server
        .initialize(InitializeParams::default())
        .await
        .expect("server initialization should succeed");

    assert_eq!(
        initialized.capabilities.selection_range_provider,
        Some(SelectionRangeProviderCapability::Simple(true))
    );
}

#[tokio::test]
async fn selection_range_expands_from_call_name_to_expression_and_scopes() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "main.rb",
            "def label(user)\n  result = user.profile.name\nend\n",
        )
        .await;

    let selections = editor
        .selection_ranges("main.rb", &[Position::new(1, 25)])
        .await;
    assert_eq!(selections.len(), 1);

    let ranges = flatten_ranges(&selections[0]);
    assert_eq!(
        ranges,
        vec![
            Range::new(Position::new(1, 24), Position::new(1, 28)),
            Range::new(Position::new(1, 11), Position::new(1, 28)),
            Range::new(Position::new(1, 2), Position::new(1, 28)),
            Range::new(Position::new(0, 0), Position::new(2, 3)),
        ]
    );
}

#[tokio::test]
async fn selection_ranges_preserve_multiple_position_order_and_nesting() {
    let mut editor = FakeEditor::new().await;
    editor
        .open("main.rb", "items.map { |item| item.name }\n")
        .await;
    let positions = [Position::new(0, 13), Position::new(0, 27)];

    let selections = editor.selection_ranges("main.rb", &positions).await;

    assert_eq!(selections.len(), positions.len());
    for (selection, position) in selections.iter().zip(positions) {
        assert_nested_chain(selection, position);
    }
    assert_eq!(
        selections[0].range,
        Range::new(Position::new(0, 13), Position::new(0, 17))
    );
    assert_eq!(
        selections[1].range,
        Range::new(Position::new(0, 24), Position::new(0, 28))
    );
}

#[tokio::test]
async fn selection_ranges_handle_empty_and_malformed_buffers() {
    let mut editor = FakeEditor::new().await;
    editor.open("empty.rb", "").await;
    editor.open("broken.rb", "if user.\n").await;

    let empty = editor
        .selection_ranges("empty.rb", &[Position::new(0, 0)])
        .await;
    let broken = editor
        .selection_ranges("broken.rb", &[Position::new(0, 4)])
        .await;

    assert_eq!(empty.len(), 1);
    assert_eq!(
        empty[0].range,
        Range::new(Position::new(0, 0), Position::new(0, 0))
    );
    assert_eq!(broken.len(), 1);
    assert_nested_chain(&broken[0], Position::new(0, 4));
}

#[tokio::test]
async fn selection_ranges_use_utf16_and_refresh_after_change() {
    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", "\"😀\"; user.name\n").await;

    let before = editor
        .selection_ranges("main.rb", &[Position::new(0, 12)])
        .await;
    assert_eq!(
        before[0].range,
        Range::new(Position::new(0, 11), Position::new(0, 15))
    );

    editor.set("main.rb", "\"😀😀\"; user.profile.name\n").await;
    let after = editor
        .selection_ranges("main.rb", &[Position::new(0, 23)])
        .await;

    assert_eq!(
        after[0].range,
        Range::new(Position::new(0, 21), Position::new(0, 25))
    );
    assert_nested_chain(&after[0], Position::new(0, 23));
}

fn flatten_ranges(selection: &tower_lsp::lsp_types::SelectionRange) -> Vec<Range> {
    let mut ranges = Vec::new();
    let mut current = Some(selection);
    while let Some(selection) = current {
        ranges.push(selection.range);
        current = selection.parent.as_deref();
    }
    ranges
}

fn assert_nested_chain(selection: &tower_lsp::lsp_types::SelectionRange, position: Position) {
    let ranges = flatten_ranges(selection);
    assert!(!ranges.is_empty());
    assert!(ranges[0].start <= position && position <= ranges[0].end);
    for pair in ranges.windows(2) {
        assert!(pair[1].start <= pair[0].start);
        assert!(pair[1].end >= pair[0].end);
        assert_ne!(pair[0], pair[1]);
    }
}
