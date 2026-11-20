//! Block parameters of methods that document what they yield.

use crate::test::harness::{check, check_multi_file, FakeEditor};

/// A block passed to a method that documents its yielded value receives that
/// type, and the call still returns what the method returns.
#[tokio::test]
async fn block_parameter_has_the_documented_yield_type() {
    check(
        r#"
class Roster
  # @yieldparam name [String]
  # @return [Integer]
  def each_name
    @names.each { |n| yield n }
    0
  end
end

roster = Roster.new
count<hint label="Integer"> = roster.each_name do |na<type label="String">me|
  shout<hint label="String"> = name.upcase
end
"#,
    )
    .await;
}

/// Without a documented yield type the block parameter proves nothing.
#[tokio::test]
async fn block_parameter_of_an_undocumented_yield_stays_unknown() {
    check(
        r#"
class Roster
  def each_name
    @names.each { |n| yield n }
  end
end

roster = Roster.new
roster.each_name do |name|
  shout<hint label=": ?"> = name.upcase
end
"#,
    )
    .await;
}

/// Each documented yielded value types the block parameter in its position.
#[tokio::test]
async fn each_block_parameter_has_its_documented_yield_type() {
    check(
        r#"
class Roster
  # @yieldparam name [String]
  # @yieldparam rank [Integer]
  def each_entry
    @entries.each { |entry| yield entry.name, entry.rank }
  end
end

roster = Roster.new
roster.each_entry do |name, rank|
  shout<hint label="String"> = name.upcase
  following<hint label="Integer"> = rank.succ
end
"#,
    )
    .await;
}

/// A yielded value documented without a type leaves only its own block
/// parameter unknown.
#[tokio::test]
async fn block_parameter_of_an_untyped_documented_yield_stays_unknown() {
    check(
        r#"
class Roster
  # @yieldparam name [String]
  # @yieldparam rank
  def each_entry
    @entries.each { |entry| yield entry.name, entry.rank }
  end
end

roster = Roster.new
roster.each_entry do |name, rank|
  shout<hint label="String"> = name.upcase
  following<hint label=": ?"> = rank.succ
end
"#,
    )
    .await;
}

/// A documented type the project does not define proves nothing.
#[tokio::test]
async fn block_parameter_documented_with_an_undefined_type_stays_unknown() {
    check(
        r#"
class Roster
  # @yieldparam entry [Missing::Entry]
  def each_entry
    @entries.each { |entry| yield entry }
  end
end

roster = Roster.new
roster.each_entry do |entry|
  copy<hint label=": ?"> = entry
end
"#,
    )
    .await;
}

/// A documented project class types the block parameter, so calls on it
/// resolve through that class.
#[tokio::test]
async fn block_parameter_documented_with_a_project_class_resolves_its_methods() {
    check(
        r#"
class Entry
  # @return [String]
  def label
    compute
  end
end

class Roster
  # @yieldparam entry [Entry]
  def each_entry
    @entries.each { |entry| yield entry }
  end
end

roster = Roster.new
roster.each_entry do |entry|
  text<hint label="String"> = entry.label
end
"#,
    )
    .await;
}

/// The documented yield type is found through inheritance.
#[tokio::test]
async fn block_parameter_of_an_inherited_method_has_the_documented_yield_type() {
    check(
        r#"
class Listing
  # @yieldparam name [String]
  def each_name
    @names.each { |n| yield n }
  end
end

class Roster < Listing
end

roster = Roster.new
roster.each_name do |name|
  shout<hint label="String"> = name.upcase
end
"#,
    )
    .await;
}

/// A call made inside the class, with no receiver, reaches the same method.
#[tokio::test]
async fn block_parameter_of_a_receiverless_call_has_the_documented_yield_type() {
    check(
        r#"
class Roster
  # @yieldparam name [String]
  def each_name
    @names.each { |n| yield n }
  end

  def sho<type label="Array<String>">uts
    result = []
    each_name do |name|
      shout<hint label="String"> = name.upcase
    end
    ["a"]
  end
end
"#,
    )
    .await;
}

/// A receiver that may be either of two classes whose methods document
/// different yield types proves neither.
#[tokio::test]
async fn block_parameter_stays_unknown_when_candidates_document_different_types() {
    check(
        r#"
class Names
  # @yieldparam item [String]
  def each_item
    @items.each { |item| yield item }
  end
end

class Ranks
  # @yieldparam item [Integer]
  def each_item
    @items.each { |item| yield item }
  end
end

# @param flag [Boolean]
def pick(flag)
  source = flag ? Names.new : Ranks.new
  source.each_item do |item|
    copy<hint label=": ?"> = item
  end
end
"#,
    )
    .await;
}

/// A method redefined in a reopened class, with one definition undocumented,
/// no longer has one documented yield type.
#[tokio::test]
async fn block_parameter_stays_unknown_when_one_definition_is_undocumented() {
    check_multi_file(&[
        (
            "roster.rb",
            r#"
class Roster
  # @yieldparam name [String]
  def each_name
    @names.each { |n| yield n }
  end
end
"#,
        ),
        (
            "roster_patch.rb",
            r#"
class Roster
  def each_name
    @others.each { |n| yield n }
  end
end
"#,
        ),
        (
            "use.rb",
            r#"
roster = Roster.new
roster.each_name do |name|
  shout<hint label=": ?"> = name.upcase
end
"#,
        ),
    ])
    .await;
}

/// The documented method may be defined in a file indexed after its caller.
#[tokio::test]
async fn block_parameter_uses_a_yield_type_documented_in_a_later_file() {
    check_multi_file(&[
        (
            "a_use.rb",
            r#"
roster = Roster.new
roster.each_name do |name|
  shout<hint label="String"> = name.upcase
end
"#,
        ),
        (
            "z_roster.rb",
            r#"
class Roster
  # @yieldparam name [String]
  def each_name
    @names.each { |n| yield n }
  end
end
"#,
        ),
    ])
    .await;
}

/// The documented method may be defined in a file indexed before its caller.
#[tokio::test]
async fn block_parameter_uses_a_yield_type_documented_in_an_earlier_file() {
    check_multi_file(&[
        (
            "a_roster.rb",
            r#"
class Roster
  # @yieldparam name [String]
  def each_name
    @names.each { |n| yield n }
  end
end
"#,
        ),
        (
            "z_use.rb",
            r#"
roster = Roster.new
roster.each_name do |name|
  shout<hint label="String"> = name.upcase
end
"#,
        ),
    ])
    .await;
}

/// A method ending in a documented yielding call returns what that call's
/// method returns, while its block still sees the documented yield type.
#[tokio::test]
async fn method_ending_in_a_documented_yielding_call_returns_its_documented_return() {
    check(
        r#"
class Roster
  # @yieldparam name [String]
  # @return [Integer]
  def each_name
    @names.each { |n| yield n }
    0
  end
end

class Report
  # @param roster [Roster]
  def to<type label="Integer">tal(roster)
    roster.each_name do |name|
      shout<hint label="String"> = name.upcase
    end
  end
end
"#,
    )
    .await;
}

/// A call on a documented block parameter that the type does not define is
/// reported, and a call it does define is not.
#[tokio::test]
async fn calls_on_a_documented_block_parameter_are_checked() {
    check(
        r#"
class Roster
  # @yieldparam name [String]
  def each_name
    @names.each { |n| yield n }
  end
end

roster = Roster.new
roster.each_name do |name|
  <warn none>name.upcase</warn>
  name.<warn>launch</warn>
end
"#,
    )
    .await;
}

/// Editing the documentation changes what the block parameter proves.
#[tokio::test]
async fn editing_the_documented_yield_type_changes_the_block_parameter() {
    let mut editor = FakeEditor::new().await;
    editor
        .open_and_check_fixture(
            "roster.rb",
            r#"class Roster
  # @yieldparam name [String]
  def each_<type label="?">name
    @names.each { |n| yield n }
  end
end
"#,
        )
        .await;
    editor
        .open_and_check_fixture(
            "use.rb",
            r#"roster = Roster.new
roster.each_name do |name|
  size<hint label="Integer"> = name.length
end
"#,
        )
        .await;
    editor
        .set_and_check_fixture(
            "roster.rb",
            r#"class Roster
  def each_<type label="?">name
    @names.each { |n| yield n }
  end
end
"#,
        )
        .await;
    editor
        .check(
            "use.rb",
            r#"roster = Roster.new
roster.each_name do |name|
  size<hint label=": ?"> = name.length
end
"#,
        )
        .await;
}

/// A block declaring more parameters than the method documents receives the
/// yielded values in a way the documentation does not describe.
#[tokio::test]
async fn block_with_more_parameters_than_documented_yields_stays_unknown() {
    check(
        r#"
class Roster
  # @yieldparam name [String]
  def each_name
    @names.each { |n| yield n }
  end
end

roster = Roster.new
roster.each_name do |name, extra|
  first<hint label=": ?"> = name
  second<hint label=": ?"> = extra
end
"#,
    )
    .await;
}

/// A rest parameter collects the yielded values; no documented type is its
/// type.
#[tokio::test]
async fn rest_block_parameter_does_not_take_a_documented_yield_type() {
    check(
        r#"
class Roster
  # @yieldparam name [String]
  def each_name
    @names.each { |n| yield n }
  end
end

roster = Roster.new
roster.each_name do |*names|
  copy<hint label=": ?"> = names
end
"#,
    )
    .await;
}

/// A destructuring parameter takes parts of the yielded value, not the
/// documented type.
#[tokio::test]
async fn destructured_block_parameter_does_not_take_a_documented_yield_type() {
    check(
        r#"
class Roster
  # @yieldparam pair [Array]
  # @yieldparam rank [Integer]
  def each_pair
    @pairs.each { |pair| yield pair, 1 }
  end
end

roster = Roster.new
roster.each_pair do |(left, right), rank|
  first<hint label=": ?"> = left
  position<hint label=": ?"> = rank
end
"#,
    )
    .await;
}

/// A documented type name is resolved from the namespace of the method that
/// documents it, not from the caller's.
#[tokio::test]
async fn documented_yield_type_is_resolved_from_the_documenting_namespace() {
    check(
        r#"
module Directory
  class Entry
    # @return [String]
    def label
      compute
    end
  end

  class Roster
    # @yieldparam entry [Entry]
    def each_entry
      @entries.each { |entry| yield entry }
    end
  end
end

roster = Directory::Roster.new
roster.each_entry do |entry|
  text<hint label="String"> = entry.label
end
"#,
    )
    .await;
}

/// Documentation outranks what the body yields, wherever the definition sits
/// relative to the call.
#[tokio::test]
async fn documented_yield_type_outranks_the_yielded_value() {
    check(
        r#"
class Early
  # @yieldparam name [String]
  def each_name
    yield 1
  end
end

Early.new.each_name do |name|
  first<hint label="String"> = name
end

Late.new.each_name do |name|
  second<hint label="String"> = name
end

class Late
  # @yieldparam name [String]
  def each_name
    yield 1
  end
end
"#,
    )
    .await;
}

/// A receiver that may be a class without the method proves nothing, even
/// when the other class documents its yield.
#[tokio::test]
async fn block_parameter_stays_unknown_when_one_receiver_lacks_the_method() {
    check(
        r#"
class Names
  # @yieldparam item [String]
  def each_item
    @items.each { |item| yield item }
  end
end

class Ranks
end

# @param flag [Boolean]
def pick(flag)
  source = flag ? Names.new : Ranks.new
  source.each_item do |item|
    copy<hint label=": ?"> = item
  end
end
"#,
    )
    .await;
}

/// The call's result stays what the method returns: a block whose value is
/// unknown does not decide it, and an undocumented return stays unknown.
#[tokio::test]
async fn documented_yield_leaves_the_call_result_to_the_method_return() {
    check_multi_file(&[
        (
            "a_use.rb",
            r#"
roster = Roster.new
count<hint label="Integer"> = roster.each_name do |name|
  name.unknown_step
end
plain<hint label=": ?"> = roster.each_plain do |name|
  name.upcase
end
"#,
        ),
        (
            "z_roster.rb",
            r#"
class Roster
  # @yieldparam name [String]
  # @return [Integer]
  def each_name
    @names.each { |n| yield n }
    0
  end

  # @yieldparam name [String]
  def each_plain
    @names.each { |n| yield n }
  end
end
"#,
        ),
    ])
    .await;
}

/// The type may be written before the name; it types the same position.
#[tokio::test]
async fn documented_yield_type_written_before_the_name_types_its_position() {
    check(
        r#"
class Roster
  # @yieldparam [String] name
  # @yieldparam rank [Integer]
  def each_entry
    @entries.each { |entry| yield entry.name, entry.rank }
  end
end

roster = Roster.new
roster.each_entry do |name, rank|
  shout<hint label="String"> = name.upcase
  following<hint label="Integer"> = rank.succ
end
"#,
    )
    .await;
}

/// A yield documented as any object keeps its position, so the next
/// documented type does not move onto the wrong block parameter.
#[tokio::test]
async fn yield_documented_as_any_object_keeps_later_positions_in_place() {
    check(
        r#"
class Roster
  # @yieldparam [Object] entry
  # @yieldparam [Integer] rank
  def each_entry
    @entries.each { |entry| yield entry, entry.rank }
  end
end

roster = Roster.new
roster.each_entry do |entry, rank|
  label<hint label=": ?"> = entry.name
  following<hint label="Integer"> = rank.succ
end
"#,
    )
    .await;
}

/// An instance method's documented yield says nothing about a call on the
/// class itself.
#[tokio::test]
async fn instance_yield_documentation_does_not_type_a_call_on_the_class() {
    check(
        r#"
class Roster
  # @yieldparam name [String]
  def each_name
    @names.each { |n| yield n }
  end
end

Roster.each_name do |name|
  shout<hint label=": ?"> = name.upcase
end
"#,
    )
    .await;
}

/// A class method's documented yield types a call on the class and never a
/// call on an instance.
#[tokio::test]
async fn class_method_yield_documentation_types_only_calls_on_the_class() {
    check(
        r#"
class Roster
  # @yieldparam name [String]
  def self.each_default_name
    yield "anonymous"
  end
end

Roster.each_default_name do |name|
  shout<hint label="String"> = name.upcase
end

roster = Roster.new
roster.each_default_name do |name|
  quiet<hint label=": ?"> = name.downcase
end
"#,
    )
    .await;
}

/// A documented yielding method provided by an included module types the
/// block of a call on the including class.
#[tokio::test]
async fn block_parameter_of_a_mixed_in_method_has_the_documented_yield_type() {
    check(
        r#"
module Named
  # @yieldparam name [String]
  def each_name
    names.each { |n| yield n }
  end
end

class Roster
  include Named
end

roster = Roster.new
roster.each_name do |name|
  shout<hint label="String"> = name.upcase
end
"#,
    )
    .await;
}

/// Yields documented per overload do not say which overload a call selects.
#[tokio::test]
async fn yield_documented_under_overloads_stays_unknown() {
    check(
        r#"
class Roster
  # @overload each_name
  #   @yieldparam name [String]
  # @overload each_name(with_rank)
  #   @yieldparam rank [Integer]
  #   @yieldparam name [String]
  def each_name(with_rank = false)
    @names.each { |n| yield n }
  end
end

roster = Roster.new
roster.each_name do |name|
  shout<hint label=": ?"> = name.upcase
end
"#,
    )
    .await;
}
