//! Value-constant collection receivers must preserve their element types.

use crate::capabilities::indexing::init_workspace_for_run;
use crate::test::harness::{check_multi_file, extract_tags_with_attributes, FakeEditor};
use std::time::Duration;
use tower_lsp::lsp_types::Url;

const DECLARATION: &str = r#"
module Toolkit
  module Reports
    class ColumnSet
      ENTRIES<hint label="Array<Symbol>"> = [:title, :rank, :active].freeze
    end
  end
end
"#;

const CONSUMER: &str = r#"
module Toolkit::Reports
  class Renderer
    def render(row)
      output = {}
      Toolkit::Reports::ColumnSet::ENTRIES.<warn none code="unresolved-method">each</warn> do |entry|
        output[entry<hover label="Symbol">] = row[entry]
        entry.<warn none code="unresolved-method">to_s</warn>
        entry.<warn code="unresolved-method">missing_symbol_operation</warn>
      end
      output
    end
  end
end
"#;

#[tokio::test]
async fn frozen_symbol_array_constant_propagates_to_each_across_files() {
    check_multi_file(&[("column_set.rb", DECLARATION), ("renderer.rb", CONSUMER)]).await;
}

#[tokio::test]
async fn late_frozen_symbol_array_constant_propagates_to_each_across_files() {
    check_multi_file(&[("renderer.rb", CONSUMER), ("column_set.rb", DECLARATION)]).await;
}

#[tokio::test]
async fn cold_frozen_symbol_array_constant_propagates_to_each_across_files() {
    let (_fixture, editor, declaration_file, consumer_file) =
        cold_editor(DECLARATION, CONSUMER).await;
    editor.check(&consumer_file, CONSUMER).await;
    editor.check(&declaration_file, DECLARATION).await;
}

#[tokio::test]
async fn cold_symbol_array_block_completion_uses_the_element_type() {
    let consumer = clean(CONSUMER).replace("entry.to_s", "entry.i");
    let (_fixture, editor, _, consumer_file) = cold_editor(DECLARATION, &consumer).await;
    let (line, text) = consumer
        .lines()
        .enumerate()
        .find(|(_, text)| text.contains("entry.i"))
        .unwrap();
    let column = text.find("entry.i").unwrap() + "entry.i".len();
    let completions = editor
        .complete_at(&consumer_file, line as u32, column as u32)
        .await;
    assert!(
        completions.iter().any(|item| item.label == "id2name"),
        "cold block completion must expose Symbol methods: {completions:?}"
    );
    assert!(
        !completions.iter().any(|item| item.label == "index"),
        "cold block completion must not borrow String methods"
    );
}

fn clean(fixture: &str) -> String {
    extract_tags_with_attributes(fixture, &["hint", "warn", "hover"]).1
}

async fn cold_editor(
    declaration_fixture: &str,
    consumer_fixture: &str,
) -> (tempfile::TempDir, FakeEditor, String, String) {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let declaration = clean(declaration_fixture);
    let consumer = clean(consumer_fixture);
    let declaration_path = root.join("z_column_set.rb");
    let consumer_path = root.join("a_renderer.rb");
    std::fs::write(&declaration_path, &declaration).unwrap();
    std::fs::write(&consumer_path, &consumer).unwrap();
    let root_uri = Url::from_directory_path(&root).unwrap();
    let mut editor = FakeEditor::new().await;
    let server = editor.server().clone();
    server.set_discovered_runtimes_for_tests(Vec::new());
    server.set_user_cache_root_for_tests(fixture.path().join("cache"));
    let workspace = server.add_workspace(root_uri.clone());
    tokio::time::timeout(
        Duration::from_secs(30),
        init_workspace_for_run(&server, root_uri, workspace.begin_indexing_run()),
    )
    .await
    .expect("cold indexing must finish")
    .expect("cold indexing must succeed");

    let consumer_file = consumer_path
        .to_str()
        .unwrap()
        .trim_start_matches('/')
        .to_string();
    editor.open(&consumer_file, &consumer).await;
    let declaration_file = declaration_path
        .to_str()
        .unwrap()
        .trim_start_matches('/')
        .to_string();
    editor.open(&declaration_file, &declaration).await;
    (fixture, editor, declaration_file, consumer_file)
}

#[tokio::test]
async fn cold_symbol_array_constant_supports_generic_map_and_chained_calls() {
    let consumer = r#"
class Formatter
  def labels
    result<hint label="Array<String>"> = ::Toolkit::Reports::ColumnSet::ENTRIES.map do |entry|
      entry<hover label="Symbol">.to_s.upcase
    end
    result
  end
end
"#;
    let (_fixture, editor, _, consumer_file) = cold_editor(DECLARATION, consumer).await;
    editor.check(&consumer_file, consumer).await;
    assert!(editor.diagnostics(&consumer_file).await.is_empty());
}

#[tokio::test]
async fn cold_partial_array_constant_does_not_invent_a_symbol_element_type() {
    let declaration = DECLARATION
        .replace("<hint label=\"Array<Symbol>\">", "")
        .replace("[:title, :rank, :active]", "[:title, runtime_entry]");
    let consumer = CONSUMER
        .replace("entry<hover label=\"Symbol\">", "en<hover label=\"?\">try")
        .replace(
            "<warn code=\"unresolved-method\">",
            "<warn none code=\"unresolved-method\">",
        );
    let (_fixture, editor, _, consumer_file) = cold_editor(&declaration, &consumer).await;
    editor.check(&consumer_file, &consumer).await;
}

#[tokio::test]
async fn cold_symbol_array_block_reads_respect_shadowing_and_reassignment() {
    let consumer = r#"
class Formatter
  def labels
    Toolkit::Reports::ColumnSet::ENTRIES.each do |entry|
      entry<hover label="Symbol">.to_s
      ["inner"].each do |entry|
        entry<hover label="String">.upcase
      end
      entry<hover label="Symbol">.to_s
      entry = 42
      entry<hover label="Integer">.to_s
    end
  end

  def unrelated(entry)
    en<hover label="?">try.to_s
  end
end
"#;
    let (_fixture, editor, _, consumer_file) = cold_editor(DECLARATION, consumer).await;
    editor.check(&consumer_file, consumer).await;
    assert!(editor.diagnostics(&consumer_file).await.is_empty());
}

#[tokio::test]
async fn cold_symbol_array_constant_updates_element_types_after_edit_and_reopen() {
    let (_fixture, mut editor, declaration_file, consumer_file) =
        cold_editor(DECLARATION, CONSUMER).await;
    editor.check(&consumer_file, CONSUMER).await;
    let strings = DECLARATION
        .replace("Array<Symbol>", "Array<String>")
        .replace(
            "[:title, :rank, :active]",
            "[\"title\", \"rank\", \"active\"]",
        );
    editor.set(&declaration_file, &clean(&strings)).await;
    editor.check(&declaration_file, &strings).await;
    editor
        .check(
            &consumer_file,
            &CONSUMER.replace("label=\"Symbol\"", "label=\"String\""),
        )
        .await;

    editor.set(&declaration_file, &clean(DECLARATION)).await;
    editor.check(&consumer_file, CONSUMER).await;
    editor.close(&consumer_file).await;
    editor.open(&consumer_file, &clean(CONSUMER)).await;
    editor.check(&consumer_file, CONSUMER).await;
}
