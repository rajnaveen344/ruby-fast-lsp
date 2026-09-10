//! Small independent contracts complement the generated graph oracle. Compare
//! complete values: never erase extra targets, duplicate entries, or edit ranges.

use crate::test::harness::FakeEditor;
use serde::Serialize;
use std::collections::HashMap;
use tower_lsp::lsp_types::{Location, Position, Range, TextEdit, Url, WorkspaceEdit};

pub(super) fn exact_values<T: Serialize>(values: Vec<T>) -> Vec<String> {
    let mut values = values.into_iter().map(|value| {
        serde_json::to_string(&value).expect(
            "INVARIANT VIOLATED: simulation observation cannot be serialized. This is a bug because replay requires complete LSP values. Fix: retain serializable observations."
        )
    }).collect::<Vec<_>>();
    // LSP location arrays are unordered; duplicates remain observable.
    values.sort();
    values
}

fn assert_exact<T: Serialize>(feature: &str, actual: Vec<T>, expected: Vec<T>) {
    assert_eq!(
        exact_values(actual),
        exact_values(expected),
        "exact observation mismatch: {feature}"
    );
}

fn range(line: u32, start: u32, end: u32) -> Range {
    Range::new(Position::new(line, start), Position::new(line, end))
}

fn uri(file: &str) -> Url {
    Url::parse(&format!("file:///{file}")).expect("neutral fixture URI must be valid")
}

fn location(file: &str, line: u32, start: u32, end: u32) -> Location {
    Location::new(uri(file), range(line, start, end))
}

#[test]
fn complete_location_contract_detects_every_result_corruption() {
    let expected = vec![
        location("caller.rb", 1, 2, 7),
        location("caller.rb", 1, 11, 16),
    ];
    assert_eq!(
        exact_values(expected.clone()),
        exact_values(expected.iter().cloned().rev().collect())
    );
    let mut duplicate = expected.clone();
    duplicate.push(expected[0].clone());
    let mut extra = expected.clone();
    extra.push(location("unrelated.rb", 1, 2, 7));
    let mut wrong_start = expected.clone();
    wrong_start[0].range.start.character = 3;
    let mut wrong_end = expected.clone();
    wrong_end[0].range.end.character = 8;
    let mut wrong_uri = expected.clone();
    wrong_uri[0].uri = uri("nested/caller.rb");
    for (fault, actual) in [
        ("missing", vec![expected[0].clone()]),
        ("duplicate", duplicate),
        ("extra", extra),
        ("wrong start", wrong_start),
        ("wrong end", wrong_end),
        ("wrong URI with same suffix", wrong_uri),
        ("empty", Vec::new()),
    ] {
        let failure = std::panic::catch_unwind(|| assert_exact(fault, actual, expected.clone()))
            .expect_err("each injected result corruption must fail the exact comparison");
        let message = failure
            .downcast_ref::<String>()
            .expect("assertion payload must be a string");
        assert!(
            message.contains(&format!("exact observation mismatch: {fault}")),
            "wrong failure class: {message}"
        );
    }
}

const DECLARATION: &str = "class Vessel\n  def title = \"ok\"\nend\n";
const CALLER: &str = "v = Vessel.new\nv.title; v.title\n";

async fn assert_navigation_and_references(editor: &FakeEditor) {
    for column in [3, 12] {
        assert_exact(
            "method definition",
            editor.goto_def_at("caller.rb", 1, column).await,
            vec![location("vessel.rb", 1, 2, 18)],
        );
    }
    assert_exact(
        "all method calls without declarations",
        editor
            .references_with_declaration_at("vessel.rb", 1, 7, false)
            .await,
        vec![
            location("caller.rb", 1, 2, 7),
            location("caller.rb", 1, 11, 16),
        ],
    );
}

#[tokio::test]
async fn exact_method_results_and_rename_edits_survive_edit_recovery() {
    let mut editor = FakeEditor::new().await;
    editor.open("vessel.rb", DECLARATION).await;
    editor
        .open(
            "other.rb",
            "class Other\n  def title = 1\nend\nOther.new.title\n",
        )
        .await;
    editor.open("caller.rb", CALLER).await;
    assert_navigation_and_references(&editor).await;

    let expected_edit = WorkspaceEdit {
        changes: Some(HashMap::from([
            (
                uri("vessel.rb"),
                vec![TextEdit::new(range(1, 6, 11), "label".to_string())],
            ),
            (
                uri("caller.rb"),
                vec![
                    TextEdit::new(range(1, 2, 7), "label".to_string()),
                    TextEdit::new(range(1, 11, 16), "label".to_string()),
                ],
            ),
        ])),
        document_changes: None,
        change_annotations: None,
    };
    let actual_edit = editor
        .rename_at("vessel.rb", 1, 7, "label")
        .await
        .expect("method rename must be available");
    assert_eq!(
        actual_edit, expected_edit,
        "exact rename edit must include every target once and preserve the unrelated method"
    );
    editor.apply_edit(&actual_edit).await;
    assert_eq!(
        editor.content("vessel.rb"),
        DECLARATION.replace("title", "label")
    );
    assert_eq!(
        editor.content("caller.rb"),
        CALLER.replace("title", "label")
    );
    assert_navigation_and_references(&editor).await;

    editor.set("caller.rb", "v = Vessel.new\nv.missing\n").await;
    assert_exact(
        "missing method definition",
        editor.goto_def_at("caller.rb", 1, 3).await,
        Vec::<Location>::new(),
    );
    assert_exact(
        "removed references",
        editor
            .references_with_declaration_at("vessel.rb", 1, 7, false)
            .await,
        Vec::<Location>::new(),
    );
    editor
        .set("caller.rb", &CALLER.replace("title", "label"))
        .await;
    assert_navigation_and_references(&editor).await;
    editor.close("caller.rb").await;
    editor
        .open("caller.rb", &CALLER.replace("title", "label"))
        .await;
    assert_navigation_and_references(&editor).await;
}
