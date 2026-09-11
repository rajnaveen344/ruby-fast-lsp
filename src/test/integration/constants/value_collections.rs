//! Collections of value constants must not acquire synthetic class-object types.

use crate::test::harness::{
    extract_tags_with_attributes, get_hint_label, get_hint_tooltip, FakeEditor,
};

const SYMBOLS: &str = r#"
module OptionKinds
  ADDED = :added
  REMOVED = :removed
end
"#;

const CONSUMER: &str = r#"
class FilterCatalog
  OPTIONS<hint label=": Array<Symbol>"> = [OptionKinds::ADDED, ::OptionKinds::REMOVED].freeze

  def self.options<hint label=" -> Array<Symbol>">
    return OPTIONS
  end

  def self.implicit_options<hint label=" -> Array<Symbol>">
    OPTIONS
  end

  def self.local_options<hint label=" -> Array<Symbol>">
    result = OPTIONS
    result
  end

  def self.optional(flag)<hint label=" -> (NilClass | Array<Symbol>)">
    return nil if flag
    return OPTIONS
  end
end

items<hint label=": Array<Symbol>"> = FilterCatalog.options
items.each do |item|
  item<hover label="Symbol">.to_s
end
"#;

fn clean(fixture: &str) -> String {
    extract_tags_with_attributes(fixture, &["hint", "hover", "warn"]).1
}

async fn assert_exact_hints(editor: &FakeEditor, file: &str, fixture: &str) {
    editor.check(file, fixture).await;
    let (tags, _) = extract_tags_with_attributes(fixture, &["hint", "hover", "warn"]);
    let hints = editor.inlay_hints(file).await;
    for tag in tags.into_iter().filter(|tag| tag.kind == "hint") {
        let matching = hints
            .iter()
            .filter(|hint| hint.position == tag.range.start)
            .collect::<Vec<_>>();
        assert_eq!(
            matching
                .iter()
                .map(|hint| get_hint_label(hint))
                .collect::<Vec<_>>(),
            vec![tag.attributes["label"].clone()],
            "constant collection hint at {:?} must contain exactly the proven value type",
            tag.range.start
        );
        let expected_type = tag
            .attributes
            .get("tooltip")
            .map(String::as_str)
            .unwrap_or_else(|| {
                tag.attributes["label"]
                    .strip_prefix(": ")
                    .or_else(|| tag.attributes["label"].strip_prefix(" -> "))
                    .expect("type hint fixture must have a type prefix")
            });
        assert_eq!(
            get_hint_tooltip(matching[0]),
            Some(expected_type),
            "compact hints must preserve exactly the proven type in their tooltip"
        );
    }
}

#[tokio::test]
async fn symbol_constant_collection_return_has_only_value_types() {
    let fixture = r#"
class FilterCatalog
  ADDED = :added
  REMOVED = :removed
  OPTIONS<hint label=": Array<Symbol>"> = [ADDED, REMOVED].freeze

  def self.options<hint label=" -> Array<Symbol>">
    return OPTIONS
  end

  def self.implicit_options<hint label=" -> Array<Symbol>">
    OPTIONS
  end
end

items<hint label=": Array<Symbol>"> = FilterCatalog.options
items.each do |item|
  item<hover label="Symbol">.to_s
end
"#;
    let mut editor = FakeEditor::new().await;
    editor.open("catalog.rb", &clean(fixture)).await;
    assert_exact_hints(&editor, "catalog.rb", fixture).await;
}

#[tokio::test]
async fn symbol_constant_collection_refreshes_across_files_and_edits() {
    for declarations_first in [true, false] {
        let mut editor = FakeEditor::new().await;
        let mut files = [("kinds.rb", SYMBOLS), ("catalog.rb", CONSUMER)];
        if !declarations_first {
            files.reverse();
        }
        for (file, fixture) in files {
            editor.open(file, &clean(fixture)).await;
        }
        assert_exact_hints(&editor, "catalog.rb", CONSUMER).await;

        let strings = SYMBOLS
            .replace(":added", "\"added\"")
            .replace(":removed", "\"removed\"");
        editor.set("kinds.rb", &strings).await;
        assert_exact_hints(&editor, "catalog.rb", &CONSUMER.replace("Symbol", "String")).await;

        editor.set("kinds.rb", SYMBOLS).await;
        assert_exact_hints(&editor, "catalog.rb", CONSUMER).await;
        editor.close("catalog.rb").await;
        editor.open("catalog.rb", &clean(CONSUMER)).await;
        assert_exact_hints(&editor, "catalog.rb", CONSUMER).await;
    }
}

#[tokio::test]
async fn symbol_constant_collection_is_correct_after_cold_indexing() {
    let (_fixture, editor, _, consumer_file) =
        super::collection_receivers::cold_editor(SYMBOLS, CONSUMER).await;
    assert_exact_hints(&editor, &consumer_file, CONSUMER).await;
}

#[tokio::test]
async fn symbol_constant_collection_preserves_unknown_and_real_class_elements() {
    let fixture = r#"
class FilterCatalog
  class Token
  end
  ADDED = :added
  ALIAS = ADDED
  NESTED<hint label=": Array<Array<Symbol>>"> = [[ALIAS]].freeze
  CLASSES<hint label=": Array<Class<FilterCatalog::Token>>"> = [Token].freeze
  MIXED<hint label=": Array<Symbol | Class<…::Token>>" tooltip="Array<Symbol | Class<FilterCatalog::Token>>"> = [ADDED, Token].freeze
  PARTIAL<hint label=": Array<?>"> = [ADDED, MISSING].freeze

  def self.nested<hint label=" -> Array<Array<Symbol>>">
    return NESTED
  end
  def self.classes<hint label=" -> Array<Class<FilterCatalog::Token>>">
    return CLASSES
  end
  def self.partial<hint label=" -> Array<?>">
    return PARTIAL
  end
end
"#;
    let mut editor = FakeEditor::new().await;
    editor.open("catalog.rb", &clean(fixture)).await;
    assert_exact_hints(&editor, "catalog.rb", fixture).await;
}
