//! Returns built from `||`, `&&`, compound assignments, memoised instance
//! variables, and `return` inside a block.

use crate::test::harness::{check, check_multi_file};

/// `a || b` drops the nil of a proven left side and adds the right side.
#[tokio::test]
async fn or_of_a_nilable_instance_variable_and_a_literal_is_the_non_nil_union() {
    check(
        r#"
class Account
  # @param nickname [String, nil]
  def initialize(nickname)
    @nickname = nickname
  end

  def lab<type label="String">el
    @nickname || "unnamed"
  end

  def explicit_lab<type label="String">el
    return @nickname || "unnamed"
  end
end
"#,
    )
    .await;
}

/// A variable no constructor assigns may still be nil, so the right side of
/// `||` stays a possible result.
#[tokio::test]
async fn or_keeps_the_right_side_when_the_variable_is_assigned_only_by_a_setter() {
    check(
        r#"
class Counter
  def start
    @count = 1
  end

  def sho<type label="(Integer | String)">wn
    @count || "none"
  end
end
"#,
    )
    .await;
}

/// `a && b` is nil when the left side is, and the right side otherwise.
#[tokio::test]
async fn and_of_two_setter_assigned_variables_keeps_nil() {
    check(
        r#"
class Badge
  def enable
    @flag = true
    @name = "n"
  end

  def tit<type label="(NilClass | String)">le
    @flag && @name
  end
end
"#,
    )
    .await;
}

/// Comparisons on both sides of `&&` give true or false.
#[tokio::test]
async fn and_of_a_predicate_and_a_comparison_is_boolean() {
    check(
        r#"
class Account
  # @param balance [Integer]
  def initialize(balance)
    @balance = balance
  end

  def acti<type label="(FalseClass | TrueClass)">ve?
    @balance > 0
  end

  def mod<type label="(FalseClass | TrueClass)">est?
    active? && @balance < 100
  end
end
"#,
    )
    .await;
}

/// An unproven operand leaves the whole expression unknown.
#[tokio::test]
async fn or_with_an_unproven_side_stays_unknown() {
    check(
        r#"
class Account
  def initialize(nickname)
    @nickname = nickname
  end

  def lab<type label="?">el
    @nickname || "unnamed"
  end

  def oth<type label="?">er(value)
    "unnamed" && value
  end
end
"#,
    )
    .await;
}

/// Another file assigning the variable something else withdraws the answer.
#[tokio::test]
async fn or_is_withheld_when_another_file_assigns_the_variable_an_unproven_value() {
    check_multi_file(&[
        (
            "account.rb",
            r#"
class Account
  # @param nickname [String, nil]
  def initialize(nickname)
    @nickname = nickname
  end

  def lab<type label="?">el
    @nickname || "unnamed"
  end
end
"#,
        ),
        (
            "account_rename.rb",
            r#"
class Account
  def rename(other)
    @nickname = other
  end
end
"#,
        ),
    ])
    .await;
}

/// `@x ||= value` returns the value, never the nil it replaces.
#[tokio::test]
async fn memoised_instance_variable_returns_the_assigned_type() {
    check(
        r#"
class Settings; end

class Account
  def setti<type label="Settings">ngs
    @settings ||= Settings.new
  end

  def tit<type label="String">le
    @title ||= "untitled"
  end
end
"#,
    )
    .await;
}

/// The memoised value may itself be nil.
#[tokio::test]
async fn memoised_instance_variable_keeps_a_nilable_assigned_value() {
    check(
        r#"
class Account
  # @return [String, nil]
  def lookup
  end

  def tit<type label="(NilClass | String)">le
    @title ||= lookup
  end
end
"#,
    )
    .await;
}

/// Every other assignment of the variable is a possible memoised result.
#[tokio::test]
async fn memoised_instance_variable_includes_other_assignments() {
    check(
        r#"
class Account
  def reset
    @title = 5
  end

  def tit<type label="(Integer | String)">le
    @title ||= "untitled"
  end
end
"#,
    )
    .await;
}

/// An unproven value, here or in another file, leaves the result unknown.
#[tokio::test]
async fn memoised_instance_variable_with_an_unproven_assignment_stays_unknown() {
    check_multi_file(&[
        (
            "account.rb",
            r#"
class Account
  def tit<type label="?">le
    @title ||= "untitled"
  end

  def own<type label="?">er(value)
    @owner ||= value
  end
end
"#,
        ),
        (
            "account_rename.rb",
            r#"
class Account
  def rename(other)
    @title = other
  end
end
"#,
        ),
    ])
    .await;
}

/// A compound assignment rebinds the local it names.
#[tokio::test]
async fn compound_local_assignments_rebind_the_local() {
    check(
        r#"
def fal<type label="String">lback
  value = nil
  value ||= "x"
  value
end

def kep<type label="Integer">t
  value = 1
  value ||= "x"
  value
end

def repla<type label="String">ced
  value = 1
  value &&= "s"
  value
end

def summ<type label="Integer">ed
  value = 1
  value += 1
  value
end

def unpro<type label="?">ven(other)
  value = 1
  value += other.size
  value &&= other
  value
end
"#,
    )
    .await;
}

/// A `return` inside a block leaves the method, so its value is a result.
#[tokio::test]
async fn return_inside_an_iterator_block_is_not_ignored() {
    check(
        r#"
class Roster
  def fir<type label="?">st
    [1, 2].each { |n| return n.to_s }
    nil
  end

  def nes<type label="?">ted
    [1, 2].each do |n|
      [n].each { |m| return m.to_s if m > 1 }
    end
    nil
  end

  def plai<type label="NilClass">n
    [1, 2].each { |n| n.to_s }
    nil
  end

  def shiel<type label="NilClass">ded
    handler = -> { return 1 }
    nil
  end
end
"#,
    )
    .await;
}

/// An operator assignment is typed only when both operands are the same
/// scalar class; a mixed operation may change the class.
#[tokio::test]
async fn operator_assignment_with_a_different_operand_class_stays_unknown() {
    check(
        r#"
def mix<type label="?">ed
  value = 1
  value += 0.5
  value
end

def joi<type label="String">ned
  value = "a"
  value += "b"
  value
end
"#,
    )
    .await;
}

/// Two methods memoising one variable each see the other's value. The
/// first is read before the second assignment is known, so it is withheld.
#[tokio::test]
async fn two_memoising_methods_never_return_only_their_own_value() {
    check(
        r#"
class Cache
  def fir<type label="?">st
    @value ||= "a"
  end

  def seco<type label="(Integer | String)">nd
    @value ||= 1
  end
end

class Base
  def val<type label="?">ue
    @held ||= "a"
  end
end

class Child < Base
  def oth<type label="?">er
    @held ||= 1
  end
end
"#,
    )
    .await;
}

/// A read placed before a later assignment in the same file is withheld
/// rather than typed from half the evidence.
#[tokio::test]
async fn read_before_a_later_assignment_stays_unknown() {
    check(
        r#"
class Early
  def rea<type label="?">d
    @count || "none"
  end

  def start
    @count = 1
  end
end
"#,
    )
    .await;
}

/// Class-level and module variables have no proven set of assignments.
#[tokio::test]
async fn memoised_class_level_and_module_variables_stay_unknown() {
    check(
        r#"
class Registry
  def self.sha<type label="?">red
    @shared ||= "a"
  end
end

module Mixin
  def mem<type label="?">o
    @memo ||= "a"
  end
end
"#,
    )
    .await;
}

/// A generated writer accepts any value.
#[tokio::test]
async fn generated_writers_leave_memoised_and_fallback_reads_unknown() {
    check(
        r#"
class Writable
  attr_writer :title
  attr_accessor :name

  def tit<type label="?">le
    @title ||= "untitled"
  end

  def nam<type label="?">ed
    @name || "none"
  end
end
"#,
    )
    .await;
}

/// `&:name` on a proven instance variable is typed like the block it
/// abbreviates.
#[tokio::test]
async fn symbol_block_on_a_proven_instance_variable_is_typed() {
    check(
        r#"
class Names
  # @param names [Array<String>]
  def initialize(names)
    @names = names
  end

  def leng<type label="Array<Integer>">ths
    @names.map(&:length)
  end
end
"#,
    )
    .await;
}

/// ...and withheld when another file assigns the variable something else.
#[tokio::test]
async fn symbol_block_is_withheld_when_another_file_reassigns_the_variable() {
    check_multi_file(&[
        (
            "names.rb",
            r#"
class Names
  # @param names [Array<String>]
  def initialize(names)
    @names = names
  end

  def leng<type label="?">ths
    @names.map(&:length)
  end
end
"#,
        ),
        (
            "names_replace.rb",
            r#"
class Names
  def replace(other)
    @names = other
  end
end
"#,
        ),
    ])
    .await;
}

/// An explicit `return` of a conditional value keeps both of its results.
#[tokio::test]
async fn explicit_return_of_a_conditional_value_keeps_both_results() {
    check(
        r#"
class Page
  def sour<type label="(NilClass | Hash<Symbol, ?>)">ces(found)
    if found
      return found.size > 1 ? {:kind => found.last, :from => found.first} : nil
    else
      return nil
    end
  end
end
"#,
    )
    .await;
}

/// Memoising assignments are told apart by file as well as by number.
#[tokio::test]
async fn memoising_methods_in_two_files_never_return_only_their_own_value() {
    check_multi_file(&[
        (
            "cache.rb",
            r#"
class Cache
  def fir<type label="?">st
    @value ||= "a"
  end
end
"#,
        ),
        (
            "cache_more.rb",
            r#"
class Cache
  def seco<type label="?">nd
    @value ||= 1
  end
end
"#,
        ),
    ])
    .await;
}
