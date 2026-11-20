//! Instance variables assigned from parameters with a declared type.

use crate::test::harness::{check, check_multi_file, FakeEditor};

/// An instance variable assigned only from a documented parameter has the
/// documented type wherever the class reads it.
#[tokio::test]
async fn instance_variable_assigned_from_a_documented_parameter_has_its_type() {
    check(
        r#"
class Greeter
  # @param name [String]
  def initialize(name)
    @name = name
  end

  def na<type label="String">me
    @na<type label="String">me
  end

  def sho<type label="String">ut
    @name.upcase
  end
end
"#,
    )
    .await;
}

/// A parameter with no declared type proves nothing about the variable.
#[tokio::test]
async fn instance_variable_assigned_from_an_undocumented_parameter_stays_unknown() {
    check(
        r#"
class Greeter
  def initialize(name)
    @name = name
  end

  def na<type label="?">me
    @name
  end
end
"#,
    )
    .await;
}

/// A second assignment nothing proves withholds the declared type.
#[tokio::test]
async fn instance_variable_also_assigned_an_unproven_value_stays_unknown() {
    check(
        r#"
class Greeter
  # @param name [String]
  def initialize(name)
    @name = name
  end

  def rename(other)
    @name = other
  end

  def na<type label="?">me
    @name
  end
end
"#,
    )
    .await;
}

/// A required keyword with a declared type proves the variable too.
#[tokio::test]
async fn instance_variable_assigned_from_a_documented_keyword_has_its_type() {
    check(
        r#"
class Greeter
  # @param name [String]
  def initialize(name:)
    @name = name
  end

  def na<type label="String">me
    @name
  end
end
"#,
    )
    .await;
}

/// An optional parameter may hold its default, which the declared type does not describe.
#[tokio::test]
async fn instance_variable_assigned_from_an_optional_parameter_stays_unknown() {
    check(
        r#"
class Greeter
  # @param name [String]
  def initialize(name = nil)
    @name = name
  end

  def na<type label="?">me
    @name
  end
end
"#,
    )
    .await;
}

/// A parameter the method reassigns no longer holds its declared type.
#[tokio::test]
async fn instance_variable_assigned_from_a_reassigned_parameter_stays_unknown() {
    check(
        r#"
class Greeter
  # @param name [String]
  def initialize(name)
    name = name.to_sym if name.empty?
    @name = name
  end

  def na<type label="?">me
    @name
  end
end
"#,
    )
    .await;
}

/// A block parameter that shadows the documented parameter is a different variable.
#[tokio::test]
async fn instance_variable_assigned_from_a_block_local_of_the_same_name_stays_unknown() {
    check(
        r#"
class Greeter
  # @param name [String]
  def initialize(name, others)
    others.each { |name| @name = name }
  end

  def na<type label="?">me
    @name
  end
end
"#,
    )
    .await;
}

/// An operator assignment stores whatever the operator returns.
#[tokio::test]
async fn instance_variable_changed_by_an_operator_stays_unknown() {
    check(
        r#"
class Greeter
  # @param name [String]
  def initialize(name)
    @name = name
  end

  def extend_name
    @name += 1
  end

  def na<type label="?">me
    @name
  end
end
"#,
    )
    .await;
}

/// A destructuring assignment stores an element nothing proves.
#[tokio::test]
async fn instance_variable_assigned_by_destructuring_stays_unknown() {
    check(
        r#"
class Greeter
  # @param name [String]
  def initialize(name)
    @name = name
  end

  def swap(pair)
    @name, @other = pair
  end

  def na<type label="?">me
    @name
  end
end
"#,
    )
    .await;
}

/// Another variable's type is not carried over, so a later change to it cannot leak.
#[tokio::test]
async fn instance_variable_assigned_from_another_variable_stays_unknown() {
    check(
        r#"
class Greeter
  def initialize
    @raw = "plain"
    @name = @raw
  end

  def na<type label="?">me
    @name
  end
end
"#,
    )
    .await;
}

/// A generated writer stores whatever its caller passes.
#[tokio::test]
async fn instance_variable_with_a_generated_writer_stays_unknown() {
    check(
        r#"
class Greeter
  # @param name [String]
  def initialize(name)
    @name = name
  end

  attr_writer :name

  def na<type label="?">me
    @name
  end
end
"#,
    )
    .await;
}

/// A generated accessor includes a writer.
#[tokio::test]
async fn instance_variable_with_a_generated_accessor_stays_unknown() {
    check(
        r#"
class Greeter
  # @param name [String]
  def initialize(name)
    @name = name
  end

  attr_accessor :name

  def cur<type label="?">rent
    @name
  end
end
"#,
    )
    .await;
}

/// A variable the constructor does not assign reads as nil until another method assigns it.
#[tokio::test]
async fn instance_variable_never_assigned_by_the_constructor_may_be_nil() {
    check(
        r#"
class Greeter
  def prepare
    @name = "plain"
  end

  def na<type label="(NilClass | String)">me
    @name
  end
end
"#,
    )
    .await;
}

/// A constructor that may return first leaves the variable unassigned.
#[tokio::test]
async fn instance_variable_assigned_after_an_early_return_may_be_nil() {
    check(
        r#"
class Greeter
  # @param name [String]
  def initialize(name)
    return if name.empty?
    @name = name
  end

  def na<type label="(NilClass | String)">me
    @name
  end
end
"#,
    )
    .await;
}

/// Every proven assignment contributes its type.
#[tokio::test]
async fn instance_variable_assigned_differently_by_two_methods_has_both_types() {
    check(
        r#"
class Greeter
  # @param name [String]
  def initialize(name)
    @name = name
  end

  def anonymize
    @name = :anonymous
  end

  def na<type label="(String | Symbol)">me
    @name
  end
end
"#,
    )
    .await;
}

/// A singleton method reads the class object's own variable, which is not modeled.
#[tokio::test]
async fn instance_variable_of_a_class_object_stays_unknown() {
    check(
        r#"
class Greeter
  def self.configure
    @name = "plain"
  end

  def self.na<type label="?">me
    @name
  end
end
"#,
    )
    .await;
}

/// A module's method runs on every object that includes it, and their assignments are not all visible.
#[tokio::test]
async fn instance_variable_read_by_a_module_method_stays_unknown() {
    check(
        r#"
module Named
  def assign
    @name = "plain"
  end

  def na<type label="?">me
    @name
  end
end
"#,
    )
    .await;
}

/// An ancestor this project cannot read may assign the variable.
#[tokio::test]
async fn instance_variable_of_a_class_with_an_unresolved_ancestor_stays_unknown() {
    check(
        r#"
class Greeter < MissingBase
  def initialize
    @name = "plain"
  end

  def na<type label="?">me
    @name
  end
end
"#,
    )
    .await;
}

/// The declared type does not depend on the order of methods in the file.
#[tokio::test]
async fn instance_variable_reader_defined_before_the_constructor_has_its_type() {
    check(
        r#"
class Greeter
  def na<type label="String">me
    @name
  end

  # @param name [String]
  def initialize(name)
    @name = name
  end
end
"#,
    )
    .await;
}

/// A call on a variable that may be nil is not proven and raises no warning.
#[tokio::test]
async fn returned_call_on_a_nilable_instance_variable_reports_nothing() {
    check(
        r#"
class Greeter
  def prepare
    @name = "plain"
  end

  def sho<type label="?">ut
    <warn none>@name.upcase</warn>
  end
end
"#,
    )
    .await;
}

/// An instance may be of a subclass, whose methods assign the same variable.
#[tokio::test]
async fn instance_variable_assigned_an_unproven_value_by_a_subclass_stays_unknown() {
    check_multi_file(&[
        (
            "greeter.rb",
            r#"
class Greeter
  # @param name [String]
  def initialize(name)
    @name = name
  end

  def na<type label="?">me
    @name
  end

  def sho<type label="?">ut
    @name.upcase
  end
end
"#,
        ),
        (
            "loud_greeter.rb",
            r#"
class LoudGreeter < Greeter
  def rename(other)
    @name = other
  end
end
"#,
        ),
    ])
    .await;
}

/// Another file may reopen the class and assign the variable.
#[tokio::test]
async fn instance_variable_assigned_an_unproven_value_in_a_reopened_class_stays_unknown() {
    check_multi_file(&[
        (
            "greeter.rb",
            r#"
class Greeter
  # @param name [String]
  def initialize(name)
    @name = name
  end

  def na<type label="?">me
    @name
  end

  def sho<type label="?">ut
    @name.upcase
  end
end
"#,
        ),
        (
            "greeter_extras.rb",
            r#"
class Greeter
  def rename(other)
    @name = other
  end
end
"#,
        ),
    ])
    .await;
}

/// A subclass constructor that does not assign the variable leaves it nil for those instances.
#[tokio::test]
async fn instance_variable_of_a_subclass_with_its_own_constructor_may_be_nil() {
    check_multi_file(&[
        (
            "greeter.rb",
            r#"
class Greeter
  # @param name [String]
  def initialize(name)
    @name = name
  end

  def na<type label="(NilClass | String)">me
    @name
  end
end
"#,
        ),
        (
            "quiet_greeter.rb",
            r#"
class QuietGreeter < Greeter
  def initialize
  end
end
"#,
        ),
    ])
    .await;
}

/// A module the class includes assigns the variable on the same object.
#[tokio::test]
async fn instance_variable_assigned_by_an_included_module_contributes_its_type() {
    check_multi_file(&[
        (
            "greeter.rb",
            r#"
class Greeter
  include Renaming

  # @param name [String]
  def initialize(name)
    @name = name
  end

  def na<type label="(String | Symbol)">me
    @name
  end
end
"#,
        ),
        (
            "renaming.rb",
            r#"
module Renaming
  def anonymize
    @name = :anonymous
  end
end
"#,
        ),
    ])
    .await;
}

const GREETER: &str = r#"
class Greeter
  # @param name [String]
  def initialize(name)
    @name = name
  end

  def na<type label="String">me
    @name
  end

  def sho<type label="String">ut
    @name.upcase
  end
end
"#;

const GREETER_UNKNOWN: &str = r#"
class Greeter
  # @param name [String]
  def initialize(name)
    @name = name
  end

  def na<type label="?">me
    @name
  end

  def sho<type label="?">ut
    @name.upcase
  end
end
"#;

/// An assignment added in another file withholds the type; removing it
/// restores the type, without touching the reader's file.
#[tokio::test]
async fn editing_another_file_changes_what_the_instance_variable_proves() {
    let mut editor = FakeEditor::new().await;
    editor.open_and_check_fixture("greeter.rb", GREETER).await;

    editor
        .open(
            "greeter_extras.rb",
            "class Greeter\n  def rename(other)\n    @name = other\n  end\nend\n",
        )
        .await;
    editor.check("greeter.rb", GREETER_UNKNOWN).await;

    editor
        .set(
            "greeter_extras.rb",
            "class Greeter\n  def rename(other)\n  end\nend\n",
        )
        .await;
    editor.check("greeter.rb", GREETER).await;
}

/// Changing an assignment in the reader's own file changes the proven type.
#[tokio::test]
async fn editing_an_assignment_changes_what_the_instance_variable_proves() {
    let mut editor = FakeEditor::new().await;
    editor.open_and_check_fixture("greeter.rb", GREETER).await;

    editor
        .set_and_check_fixture(
            "greeter.rb",
            &GREETER_UNKNOWN.replace("@name = name", "@name = name.strip"),
        )
        .await;

    editor.set_and_check_fixture("greeter.rb", GREETER).await;
}

/// A call with a block is not evaluated with the provisional type.
#[tokio::test]
async fn returned_block_call_on_an_instance_variable_stays_unknown() {
    check(
        r#"
class Greeter
  # @param name [String]
  def initialize(name)
    @name = name
  end

  def let<type label="?">ters
    @name.each_char { |letter| letter }
  end
end
"#,
    )
    .await;
}

/// A prepended constructor that does not assign the variable runs first.
#[tokio::test]
async fn instance_variable_behind_a_prepended_constructor_may_be_nil() {
    check_multi_file(&[
        (
            "greeter.rb",
            r#"
class Greeter
  prepend Quiet

  # @param name [String]
  def initialize(name)
    @name = name
  end

  def na<type label="(NilClass | String)">me
    @name
  end
end
"#,
        ),
        (
            "quiet.rb",
            r#"
module Quiet
  def initialize(*)
  end
end
"#,
        ),
    ])
    .await;
}

/// A constructor inherited from an included module is not yet proven to run
/// for every instance, so the variable keeps nil as a possibility.
#[tokio::test]
async fn instance_variable_assigned_by_an_included_constructor_may_be_nil() {
    check(
        r#"
module Named
  # @param name [String]
  def initialize(name)
    @name = name
  end
end

class Greeter
  include Named

  def na<type label="(NilClass | String)">me
    @name
  end
end
"#,
    )
    .await;
}

/// A class that replaces `new` may build instances without running its
/// constructor, so the variable may never be assigned.
#[tokio::test]
async fn instance_variable_of_a_class_with_a_replaced_new_may_be_nil() {
    check(
        r#"
class Greeter
  def self.new(*)
    allocate
  end

  # @param name [String]
  def initialize(name)
    @name = name
  end

  def na<type label="(NilClass | String)">me
    @name
  end
end
"#,
    )
    .await;
}

/// A returned call evaluated before any assignment was read, on a variable
/// another file leaves unproven, publishes no claim about the receiver.
#[tokio::test]
async fn returned_call_on_an_unproven_instance_variable_reports_nothing() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "greeter_extras.rb",
            "class Greeter\n  def rename(other)\n    @name = other\n  end\nend\n",
        )
        .await;
    editor
        .open(
            "greeter.rb",
            "class Greeter\n  def total\n    @name.fetch_total\n  end\n\n  def initialize\n    @name = \"plain\"\n  end\nend\n",
        )
        .await;

    assert_eq!(editor.diagnostics("greeter.rb").await, Vec::new());
}

/// A constructor assignment that may be skipped leaves nil possible.
#[tokio::test]
async fn instance_variable_assigned_conditionally_by_the_constructor_may_be_nil() {
    check(
        r#"
class Greeter
  # @param name [String]
  def initialize(name, flag)
    @name = name if flag
  end

  def na<type label="(NilClass | String)">me
    @name
  end
end
"#,
    )
    .await;
}

/// A block in the constructor may never run its assignment.
#[tokio::test]
async fn instance_variable_assigned_inside_a_constructor_block_may_be_nil() {
    check(
        r#"
class Greeter
  # @param name [String]
  def initialize(name, items)
    items.each { @name = name }
  end

  def na<type label="(NilClass | String)">me
    @name
  end
end
"#,
    )
    .await;
}

/// A rescued constructor body may complete without assigning.
#[tokio::test]
async fn instance_variable_assigned_inside_a_rescued_constructor_body_may_be_nil() {
    check(
        r#"
class Greeter
  # @param name [String]
  def initialize(name)
    prepare
    @name = name
  rescue StandardError
    nil
  end

  def na<type label="(NilClass | String)">me
    @name
  end
end
"#,
    )
    .await;
}

/// A method defined from a block assigns the instance's variable too.
#[tokio::test]
async fn instance_variable_assigned_by_a_block_defined_method_is_not_proven_a_string() {
    check(
        r#"
class Greeter
  # @param name [String]
  def initialize(name)
    @name = name
  end

  define_method(:reset) { @name = compute }

  def na<type label="?">me
    @name
  end
end
"#,
    )
    .await;
}

/// A variable assigned by a class method belongs to the class object.
#[tokio::test]
async fn class_method_variable_of_the_same_name_does_not_change_the_instance_variable() {
    check(
        r#"
class Greeter
  def self.configure
    @name = :configured
  end

  # @param name [String]
  def initialize(name)
    @name = name
  end

  def na<type label="String">me
    @name
  end
end
"#,
    )
    .await;
}

/// A class-body assignment is not placed on one side, so it proves nothing
/// about the instance's variable of the same name.
#[tokio::test]
async fn class_body_variable_of_the_same_name_keeps_the_instance_variable_unknown() {
    check(
        r#"
class Greeter
  @name = 5

  # @param name [String]
  def initialize(name)
    @name = name
  end

  def na<type label="?">me
    @name
  end
end
"#,
    )
    .await;
}
