//! Self-tests: each runner accepts the exact result and rejects near misses.

use super::*;

#[tokio::test]
async fn hints_match_exact_type_text_with_or_without_prefix() {
    check(r#"x<hint label="String"> = "hello""#).await;
    check(r#"x<hint label=": String"> = "hello""#).await;
}

#[tokio::test]
#[should_panic(expected = "expected inlay hint")]
async fn hint_labels_do_not_match_substrings() {
    // `Str` is a substring of `String`.
    check(r#"x<hint label="Str"> = "hello""#).await;
}

#[tokio::test]
#[should_panic(expected = "expected inlay hint")]
async fn hints_must_be_at_the_exact_position() {
    check(r#"x <hint label="String">= "hello""#).await;
}

#[tokio::test]
async fn goto_definition_matches_exact_ranges() {
    check(
        r#"
<def>class Foo
end</def>

Foo$0.new
"#,
    )
    .await;
}

#[tokio::test]
async fn location_tags_may_live_in_another_file() {
    check_multi_file(&[
        ("definition.rb", "<def>class Target\nend</def>\n"),
        ("usage.rb", "Target$0.new\n"),
    ])
    .await;
}

#[tokio::test]
#[should_panic(expected = "Def locations mismatch")]
async fn missing_definitions_in_another_file_fail() {
    check_multi_file(&[
        (
            "definition.rb",
            "<def>class Target\nend</def>\n<def>OTHER = 1</def>\n",
        ),
        ("usage.rb", "Target$0.new\n"),
    ])
    .await;
}

#[tokio::test]
#[should_panic(expected = "fixture asserts nothing")]
async fn fixtures_without_assertions_fail() {
    check("class Foo; end").await;
}

#[tokio::test]
#[should_panic(expected = "need a `$0` cursor")]
async fn cursor_tags_without_cursor_fail() {
    check("<def>class Foo; end</def>\nFoo.new").await;
}

#[tokio::test]
#[should_panic(expected = "no tag that uses it")]
async fn unused_cursor_fails() {
    check("class Foo; end\nFoo$0.new\n<err none>Foo</err>").await;
}

#[tokio::test]
async fn valid_code_can_assert_absence_of_errors() {
    check(
        r#"
<err none>
class Foo
  def bar
    "hello"
  end
end
</err>
"#,
    )
    .await;
}

#[tokio::test]
async fn code_lens_title_matches_exactly() {
    check(
        r#"
module MyModule <lens title="1 include">
end

class MyClass
  include MyModule
end
"#,
    )
    .await;
}

#[tokio::test]
#[should_panic(expected = "expected code lens")]
async fn code_lens_title_does_not_match_substrings() {
    check(
        r#"
module MyModule <lens title="include">
end

class MyClass
  include MyModule
end
"#,
    )
    .await;
}

#[tokio::test]
async fn absent_code_lenses_and_hints() {
    check("<lens none>\nmodule UnusedModule\nend\n</lens>").await;
    check("<hint none>\nFOO = some_unknown_method\n</hint>").await;
}

#[tokio::test]
async fn type_hierarchy_names_match_exactly() {
    check(
        r#"
class Animal
end

<th supertypes="Animal" subtypes="Poodle">
class Dog$0 < Animal
end
</th>

class Poodle < Dog
end
"#,
    )
    .await;
}

#[tokio::test]
#[should_panic(expected = "subtypes of `Dog` mismatch")]
async fn type_hierarchy_rejects_missing_names() {
    check(
        r#"
class Animal
end

<th subtypes="">
class Dog$0 < Animal
end
</th>

class Poodle < Dog
end
"#,
    )
    .await;
}

#[tokio::test]
async fn completion_labels_match_exactly() {
    check(
        r#"
class Greeter
end

class GreetHelper
end

Greet$0
<complete items="Greeter,GreetHelper">
"#,
    )
    .await;
}

#[tokio::test]
#[should_panic(expected = "completion `Greet` missing")]
async fn completion_labels_do_not_match_substrings() {
    check("class Greeter\nend\nGreet$0\n<complete items=\"Greet\">").await;
}

#[tokio::test]
async fn diagnostics_match_range_code_and_message() {
    check(r#"<err code="unresolved-constant">UnknownThing</err>.new"#).await;
    check(r#"<err message="Unresolved constant">UnknownThing</err>.new"#).await;
    check(r#"<err code="unresolved-constant" message="Unresolved">UnknownThing</err>.new"#).await;
    check(r#"<err>UnknownThing</err>.new"#).await;
}

#[tokio::test]
#[should_panic(expected = "expected error")]
async fn diagnostics_reject_wrong_code() {
    check(r#"<err code="bogus-code-name">UnknownThing</err>.new"#).await;
}

#[tokio::test]
#[should_panic(expected = "expected error")]
async fn diagnostics_reject_wrong_message() {
    check(r#"<err message="this text not in diag">UnknownThing</err>.new"#).await;
}

#[tokio::test]
#[should_panic(expected = "expected error")]
async fn diagnostics_reject_wrong_range() {
    check(r#"<err>UnknownThin</err>g.new"#).await;
}

#[tokio::test]
async fn diagnostics_in_secondary_files_are_checked() {
    check_multi_file(&[
        ("main.rb", "class Main\nend\n"),
        (
            "other.rb",
            "<err code=\"unresolved-constant\">DoesNotExist</err>.new\n",
        ),
    ])
    .await;
}

#[tokio::test]
async fn hover_label_matches_a_whole_type() {
    check("value = 1\nvalue<hover label=\"Integer\">\n").await;
}

#[tokio::test]
#[should_panic(expected = "does not show")]
async fn hover_label_does_not_match_substrings() {
    check("value = [1]\nvalue<hover label=\"Integer\">\n").await;
}

#[tokio::test]
async fn type_tags_match_exactly() {
    check("value<type label=\"Integer\" kind=\"var\"> = 1\n").await;
}

#[tokio::test]
#[should_panic(expected = "type mismatch")]
async fn type_tags_do_not_match_substrings() {
    check("value<type label=\"Integer\" kind=\"var\"> = [1]\n").await;
}

#[tokio::test]
async fn rename_matches_every_edit() {
    check("<rename to=\"total\">count</rename> = 1\nputs <rename>count</rename>\n").await;
}

#[tokio::test]
#[should_panic(expected = "rename edits mismatch")]
async fn rename_rejects_missing_edits() {
    check("<rename to=\"total\">count</rename> = 1\nputs count\n").await;
}

#[tokio::test]
async fn literal_dot_completion() {
    let mut editor = FakeEditor::new().await;

    editor.open("a.rb", "a = [1,2,3].").await;
    let items = editor.complete_with_trigger("a.rb", 0, 12, ".").await;
    assert!(
        items.iter().any(|i| i.label == "first"),
        "Array completions: {items:?}"
    );

    editor.open("b.rb", r#"b = "hello"."#).await;
    let items = editor.complete_with_trigger("b.rb", 0, 12, ".").await;
    assert!(
        items.iter().any(|i| i.label == "upcase"),
        "String completions: {items:?}"
    );

    editor.open("c.rb", "c = {a: 1}.").await;
    let items = editor.complete_with_trigger("c.rb", 0, 11, ".").await;
    assert!(
        items.iter().any(|i| i.label == "keys"),
        "Hash completions: {items:?}"
    );

    editor.open("d.rb", "d = 42.").await;
    let items = editor.complete_with_trigger("d.rb", 0, 7, ".").await;
    assert!(
        items.iter().any(|i| i.label == "abs"),
        "Integer completions: {items:?}"
    );
}

#[tokio::test]
async fn fake_editor_assert_no_errors_passes_on_clean_code() {
    let mut editor = FakeEditor::new().await;
    editor
        .open("clean.rb", "class Foo\n  def bar\n    1\n  end\nend")
        .await;
    editor.assert_no_errors("clean.rb").await;
}

#[tokio::test]
#[should_panic(expected = "Expected no errors")]
async fn fake_editor_assert_no_errors_fails_when_error_present() {
    let mut editor = FakeEditor::new().await;
    editor.open("dirty.rb", "UnknownThing.new").await;
    editor.assert_no_errors("dirty.rb").await;
}

#[tokio::test]
async fn fake_editor_assert_error_code_finds_match() {
    let mut editor = FakeEditor::new().await;
    editor.open("err.rb", "UnknownThing.new").await;
    let diagnostic = editor
        .assert_error_code("err.rb", "unresolved-constant")
        .await;
    assert!(diagnostic.message.contains("UnknownThing"));
}

#[tokio::test]
#[should_panic(expected = "Expected error with code")]
async fn fake_editor_assert_error_code_fails_when_missing() {
    let mut editor = FakeEditor::new().await;
    editor.open("err.rb", "UnknownThing.new").await;
    editor.assert_error_code("err.rb", "nonexistent-code").await;
}

#[tokio::test]
#[should_panic(expected = "does not match the open buffer")]
async fn fake_editor_check_requires_the_exact_buffer() {
    let mut editor = FakeEditor::new().await;
    editor.open("buffer.rb", "x = 1\n").await;
    editor
        .check("buffer.rb", "\nx<hint label=\"Integer\"> = 1\n")
        .await;
}
