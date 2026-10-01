//! Method targets follow ancestor edits, partial open order, and closed-buffer
//! delivery. Every goto observation compares the complete target set, so a
//! stale ancestor, a missing target, or a duplicate fails.

use crate::test::harness::{fixture_uri, FakeEditor};
use tower_lsp::lsp_types::{Location, Position, Range};

const CALLER: &str = "caller.rb";
/// `child.resolve_token` in [`CALLER`]: line 3, inside `resolve_token`.
const CALL: (u32, u32) = (3, 12);

fn method_file(owner_kind: &str, owner: &str, header_extra: &str, value: &str) -> String {
    format!(
        "{owner_kind} {owner}{header_extra}\n  def resolve_token\n    \"{value}\"\n  end\nend\n"
    )
}

fn caller_source(receiver: &str) -> String {
    format!("class Caller\n  def run\n    child = {receiver}.new\n    child.resolve_token\n  end\nend\n")
}

/// The `resolve_token` definition in a file produced by [`method_file`].
fn method_target(file: &str) -> Location {
    Location::new(
        fixture_uri(format!("/{file}")),
        Range::new(Position::new(1, 2), Position::new(3, 5)),
    )
}

async fn assert_call_targets(editor: &FakeEditor, expected: &[&str], state: &str) {
    let mut actual = editor.goto_def_at(CALLER, CALL.0, CALL.1).await;
    let mut expected = expected
        .iter()
        .map(|file| method_target(file))
        .collect::<Vec<_>>();
    let key = |location: &Location| (location.uri.to_string(), location.range.start);
    actual.sort_by_key(key);
    expected.sort_by_key(key);
    assert_eq!(
        actual, expected,
        "complete resolve_token targets after {state}"
    );
}

async fn open_mixin_project(editor: &mut FakeEditor, mixin: &str, child_body: &str) {
    editor
        .open("first.rb", &method_file("module", "First", "", "first"))
        .await;
    editor
        .open("second.rb", &method_file("module", "Second", "", "second"))
        .await;
    editor
        .open("base.rb", &method_file("class", "Base", "", "base"))
        .await;
    editor
        .open(
            "child.rb",
            &format!("class Child < Base\n  {mixin} First\n  {mixin} Second\n{child_body}end\n"),
        )
        .await;
    editor.open(CALLER, &caller_source("Child")).await;
}

#[tokio::test]
async fn removing_an_include_drops_its_method_target() {
    let mut editor = FakeEditor::new().await;
    open_mixin_project(&mut editor, "include", "").await;
    assert_call_targets(&editor, &["second.rb"], "the initial includes").await;

    editor
        .set("child.rb", "class Child < Base\n  include First\nend\n")
        .await;
    assert_call_targets(&editor, &["first.rb"], "removing include Second").await;
}

#[tokio::test]
async fn removing_a_prepend_drops_its_method_target() {
    let mut editor = FakeEditor::new().await;
    let own = "  def resolve_token\n    \"child\"\n  end\n";
    open_mixin_project(&mut editor, "prepend", own).await;
    assert_call_targets(&editor, &["second.rb"], "the initial prepends").await;

    editor
        .set(
            "child.rb",
            &format!("class Child < Base\n  prepend First\n{own}end\n"),
        )
        .await;
    assert_call_targets(&editor, &["first.rb"], "removing prepend Second").await;
}

#[tokio::test]
async fn switching_a_superclass_drops_the_old_method_target() {
    let mut editor = FakeEditor::new().await;
    editor
        .open("old_base.rb", &method_file("class", "OldBase", "", "old"))
        .await;
    editor
        .open("new_base.rb", &method_file("class", "NewBase", "", "new"))
        .await;
    editor
        .open("child.rb", "class Child < OldBase\nend\n")
        .await;
    editor.open(CALLER, &caller_source("Child")).await;
    assert_call_targets(&editor, &["old_base.rb"], "the initial superclass").await;

    editor.set("child.rb", "class Child < NewBase\nend\n").await;
    assert_call_targets(&editor, &["new_base.rb"], "switching the superclass").await;
}

#[tokio::test]
async fn ancestors_opened_later_complete_the_method_target() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "child.rb",
            "class Child < Base\n  include First\n  include Second\nend\n",
        )
        .await;
    editor.open(CALLER, &caller_source("Child")).await;
    assert_call_targets(&editor, &[], "opening only the child and caller").await;

    editor
        .open("base.rb", &method_file("class", "Base", "", "base"))
        .await;
    editor
        .open("second.rb", &method_file("module", "Second", "", "second"))
        .await;
    editor
        .open("first.rb", &method_file("module", "First", "", "first"))
        .await;
    assert_call_targets(&editor, &["second.rb"], "opening every ancestor").await;
}

/// Method and constant targets observed from the reader in
/// [`closed_buffer_keeps_delivered_definitions_until_reopened`].
async fn observe(editor: &FakeEditor) -> (Vec<Location>, Vec<Location>) {
    (
        editor.goto_def_at(CALLER, CALL.0, CALL.1).await,
        editor.goto_def_at(CALLER, 4, 15).await,
    )
}

#[tokio::test]
async fn closed_buffer_keeps_delivered_definitions_until_reopened() {
    let delivered = "class Provider\n  TOKEN = 1\n  def resolve_token\n    \"value\"\n  end\nend\n";
    let edited = "class Provider\nend\n";
    let reader = "class Reader\n  def read\n    child = Provider.new\n    child.resolve_token\n    Provider::TOKEN\n  end\nend\n";
    let constant_target = Location::new(
        fixture_uri("/provider.rb"),
        Range::new(Position::new(1, 2), Position::new(1, 11)),
    );
    let method_target = Location::new(
        fixture_uri("/provider.rb"),
        Range::new(Position::new(2, 2), Position::new(4, 5)),
    );
    let mut editor = FakeEditor::new().await;
    editor.open("provider.rb", delivered).await;
    editor.open(CALLER, reader).await;
    let resolved = (vec![method_target.clone()], vec![constant_target.clone()]);
    assert_eq!(observe(&editor).await, resolved, "delivered definitions");

    // Closing delivers no new content; an undelivered disk change is invisible.
    editor.close("provider.rb").await;
    assert_eq!(
        observe(&editor).await,
        resolved,
        "a closed buffer keeps its delivered definitions"
    );

    editor.open("provider.rb", edited).await;
    assert_eq!(
        observe(&editor).await,
        (Vec::new(), Vec::new()),
        "reopening with removed definitions"
    );

    editor.set("provider.rb", delivered).await;
    assert_eq!(
        observe(&editor).await,
        resolved,
        "restoring the delivered definitions"
    );
}
