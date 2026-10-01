//! Compact inline labels retain complete semantic types in their tooltips.

use crate::test::harness::{strip_markers, FakeEditor};

/// Hint tags assert both the compact label and the complete tooltip type.
async fn assert_presentation(editor: &FakeEditor, file: &str, fixture: &str) {
    editor.check(file, fixture).await;
}

async fn open_fixture(editor: &mut FakeEditor, file: &str, fixture: &str) {
    let clean = strip_markers(fixture);
    editor.open(file, &clean).await;
}

#[tokio::test]
async fn compact_inlay_hash_shapes_keep_full_hover_details() {
    let fixture = r#"
class Measurements
  def readings(flag)<hint label=" -> Hash<Symbol, Float>" tooltip="```ruby
(
  { }
  | {
    east: Float,
    north: Float,
    west: Float
  }
)
```">
    result<hint label=": Hash<?, ?>" tooltip="```ruby
{ }
```"> = {}
    if flag
      result<hint label=": Hash<Symbol, Float>" tooltip="```ruby
{
  east: Float,
  north: Float,
  west: Float
}
```"> = { north: 1.5, east: 2.5, west: 3.5 }
    end
    result<hover label="({ } | { east: Float, north: Float, west: Float })">
  end
end
rows<hint label=": Array<Hash<Symbol, Integer>>" tooltip="```ruby
Array<{
  key: Integer
}>
```"> = [{ key: 1 }]
"#;
    let mut editor = FakeEditor::new().await;
    open_fixture(&mut editor, "measurements.rb", fixture).await;
    assert_presentation(&editor, "measurements.rb", fixture).await;
}

#[tokio::test]
async fn compact_inlay_names_apply_to_variables_returns_parameters_and_chains() {
    let fixture = r#"
module Workshop
  module Runtime
    module Services
      module Reports
        class Entry
        end
      end
    end
  end
end

class Reader
  # @param value [Workshop::Runtime::Services::Reports::Entry]
  def read(value<hint label=": …::Entry" tooltip="Workshop::Runtime::Services::Reports::Entry">)<hint label=" -> …::Entry" tooltip="Workshop::Runtime::Services::Reports::Entry">
    value
  end
end

item<hint label=": …::Entry" tooltip="Workshop::Runtime::Services::Reports::Entry"> = Workshop::Runtime::Services::Reports::Entry.new
items<hint label=": Array<…::Entry>" tooltip="Array<Workshop::Runtime::Services::Reports::Entry>"> = [Workshop::Runtime::Services::Reports::Entry.new]
Workshop::Runtime::Services::Reports::Entry.new<hint label=": …::Entry" tooltip="Workshop::Runtime::Services::Reports::Entry">
  .to_s
"#;
    let mut editor = FakeEditor::new().await;
    open_fixture(&mut editor, "reader.rb", fixture).await;
    assert_presentation(&editor, "reader.rb", fixture).await;
}

#[tokio::test]
async fn compact_inlay_shape_tooltips_refresh_after_edits() {
    let before = r#"result<hint label=": Hash<Symbol, Integer>" tooltip="```ruby
{
  count: Integer
}
```"> = { count: 1 }
result<hover label="{ count: Integer }">
"#;
    let after = r#"result<hint label=": Hash<Symbol, String>" tooltip="```ruby
{
  label: String
}
```"> = { label: "ready" }
result<hover label="{ label: String }">
"#;
    let mut editor = FakeEditor::new().await;
    open_fixture(&mut editor, "values.rb", before).await;
    assert_presentation(&editor, "values.rb", before).await;
    let clean = strip_markers(after);
    editor.set("values.rb", &clean).await;
    assert_presentation(&editor, "values.rb", after).await;
    editor.close("values.rb").await;
    open_fixture(&mut editor, "values.rb", after).await;
    assert_presentation(&editor, "values.rb", after).await;
}

#[tokio::test]
async fn compact_inlay_shape_summaries_preserve_real_alternatives() {
    let fixture = r#"
class Measurements
  def mixed(flag)<hint label=" -> Hash<Symbol, (Integer | String)>" tooltip="```ruby
(
  {
    label: String
  }
  | {
    north: Integer
  }
)
```">
    flag ? { north: 1 } : { label: "ready" }
  end

  def optional(flag)<hint label=" -> (NilClass | Hash<Symbol, Integer>)" tooltip="```ruby
(
  NilClass
  | {
    north: Integer
  }
)
```">
    flag ? { north: 1 } : nil
  end
end
"#;
    let mut editor = FakeEditor::new().await;
    open_fixture(&mut editor, "variants.rb", fixture).await;
    assert_presentation(&editor, "variants.rb", fixture).await;
}
