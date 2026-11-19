//! Declared types (signature files and documentation) must never turn into a
//! claim the source does not support: a name that cannot be resolved, a splat
//! parameter, or a result the declaration leaves open stays unknown.

use crate::test::harness::check_project;

/// A splat parameter holds the collection of its declared element type.
#[tokio::test]
async fn signature_splat_parameters_are_collections_in_the_body() {
    check_project(&[
        (
            "sig/joiner.rbs",
            r#"class Joiner
  def join: (*String parts, **Integer weights) -> String
end
"#,
        ),
        (
            "joiner.rb",
            r#"class Joiner
  def join(*parts, **weights)
    parts.<warn none>each</warn> { |part| part }
    weights.<warn none>key?</warn>(:a)
    parts.<warn none>join</warn>(",")
  end
end
"#,
        ),
    ])
    .await;
}

/// `void` says the result must not be used, not that it is nil.
#[tokio::test]
async fn signature_void_result_is_not_read_as_nil() {
    check_project(&[
        (
            "sig/journal.rbs",
            r#"class Journal
  def record: (String line) -> void
end
"#,
        ),
        (
            "journal.rb",
            r#"class Journal
  def record(line)
    lines << line
  end

  def lines
    @lines ||= []
  end

  def run
    record("a").<warn none>size</warn>
  end
end
"#,
        ),
    ])
    .await;
}

/// A nested name the signature leaves relative is unresolved; it is not the
/// core class that happens to share its last segment.
#[tokio::test]
async fn signature_nested_name_is_not_matched_to_a_core_class_by_last_segment() {
    check_project(&[
        (
            "sig/shop.rbs",
            r#"module Shop
  class Finder
    def entry: () -> Entry::Method
  end
end
"#,
        ),
        (
            "lib/shop.rb",
            r#"module Shop
  module Entry
    class Method
      def label
        "entry"
      end
    end
  end

  class Finder
    def entry
      Entry::Method.new
    end

    def run
      entry.<warn none>label</warn>
    end
  end
end
"#,
        ),
    ])
    .await;
}

/// A block call that cannot be solved must not take the result the same
/// method has without a block.
#[tokio::test]
async fn unsolved_block_call_does_not_take_the_blockless_result() {
    check_project(&[(
        "lib/table.rb",
        r#"def sizes(table)
  values = (table || {}).transform_values { |value| value.to_s }
  <warn none>values[:a]</warn>
end

def names
  keys = {}.transform_keys { |key| key.to_s }
  <warn none>keys["a"]</warn>
end
"#,
    )])
    .await;
}

/// The same holds when the receiver is one of several collections.
#[tokio::test]
async fn unsolved_block_call_on_a_nilable_declared_collection_stays_unknown() {
    check_project(&[
        (
            "sig/shop.rbs",
            r#"module Shop
  class Table
    def rows: (Tree::Node node) -> Hash[Symbol, Tree::Node]?
  end
end
"#,
        ),
        (
            "lib/shop.rb",
            r#"module Shop
  class Table
    def rows(node)
      nil
    end

    def run(node)
      sizes = (rows(node) || {}).transform_values { |value| value.to_s }
      <warn none>sizes[:a]</warn>
    end
  end
end
"#,
        ),
    ])
    .await;
}

/// Attribute declarations sit beside method signatures in most shipped
/// signature files; they must not switch the file's other contracts off.
#[tokio::test]
async fn signature_attributes_keep_the_files_contracts() {
    check_project(&[
        (
            "sig/space.rbs",
            r#"module Outer
  class Space
    attr_reader path: String
    attr_accessor depth: Integer
    def self.parse: (String text) -> Space
  end
end
"#,
        ),
        (
            "space.rb",
            r#"module Outer
  class Space
    attr_reader :path
    attr_accessor :depth

    def self.parse(text)
      new
    end
  end
end

Outer::Space.parse(<warn code="argument-type-mismatch">42</warn>)
<warn none code="argument-type-mismatch">Outer::Space.parse("a")</warn>
"#,
        ),
    ])
    .await;
}

/// A method body runs after its file loads, so a nested class declared
/// below the method still wins over a same-named outer class that a
/// signature file declares.
#[tokio::test]
async fn nested_class_declared_below_its_use_wins_over_a_signature_outer_class() {
    check_project(&[
        (
            "sig/sorter.rbs",
            r#"module Shop
  class Sorter
    def run: () -> void
  end
end
"#,
        ),
        (
            "lib/builder.rb",
            r#"module Shop
  class Builder
    class Methods
      def each
        <warn none>Sorter.new(1).each_part</warn>
        sorter = Sorter.new(1)
        <warn none>sorter.each_part</warn>
      end

      class Sorter
        def initialize(methods)
        end

        def each_part
        end
      end
    end
  end
end
"#,
        ),
    ])
    .await;
}
