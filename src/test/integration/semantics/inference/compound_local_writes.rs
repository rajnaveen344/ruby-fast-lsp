//! The type a local holds after `||=`, `&&=`, and an operator assignment.
//!
//! Each of these reads the binding it replaces, so the result is proven only
//! from that binding together with the assigned value.

use crate::test::harness::check;

/// A truthy local keeps its value through `||=`; a truthy one is replaced by
/// `&&=`.
#[tokio::test]
async fn short_circuit_writes_follow_the_previous_binding_in_a_method() {
    check(
        r#"
class Report
  def kept
    value = 1
    value ||= "x"
    val<type label="Integer">ue
  end

  def replaced
    third = 1
    third &&= "s"
    thi<type label="String">rd
  end

  def filled
    blank = nil
    blank ||= "x"
    bla<type label="String">nk
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn short_circuit_writes_follow_the_previous_binding_at_the_top_level() {
    check(
        r#"
value = 1
value ||= "x"
val<type label="Integer">ue

third = 1
third &&= "s"
thi<type label="String">rd
"#,
    )
    .await;
}

/// An operator assignment is typed only when both operands share one scalar
/// class: a mixed operation may return the argument's class.
#[tokio::test]
async fn operator_writes_are_typed_only_for_operands_of_one_class() {
    check(
        r#"
class Report
  def same
    count = 1
    count += 2
    cou<type label="Integer">nt
  end

  def mixed
    other = 1
    other += 0.5
    oth<type label="?">er
  end
end

total = 1
total += 0.5
tot<type label="?">al
"#,
    )
    .await;
}

/// The binding that reaches the write may come from a branch, not from the
/// assignment written last.
#[tokio::test]
async fn short_circuit_write_after_a_branch_keeps_every_reaching_value() {
    check(
        r#"
class Report
  def label(flag)
    value = 1
    value = "s" if flag
    value ||= 2.5
    val<type label="(Integer | String)">ue
  end
end
"#,
    )
    .await;
}

/// A constant on the right of `||=` is never assigned to a truthy local.
#[tokio::test]
async fn short_circuit_write_of_a_constant_does_not_retype_a_truthy_local() {
    check(
        r#"
FALLBACK = "none"

class Report
  def label
    value = 1
    value ||= FALLBACK
    val<type label="Integer">ue
  end
end
"#,
    )
    .await;
}

/// A parameter with no documented type may hold anything, so `||=` proves
/// nothing about it.
#[tokio::test]
async fn short_circuit_write_of_an_undocumented_parameter_stays_unknown() {
    check(
        r#"
class Report
  def options(given = nil)
    given ||= {}
    giv<type label="?">en
  end
end
"#,
    )
    .await;
}

/// A read inside a later block sees the compound result.
#[tokio::test]
async fn read_inside_a_block_after_a_compound_write() {
    check(
        r#"
class Report
  def inside
    value = 1
    value ||= "x"
    [1].each do |item|
      val<type label="Integer">ue
    end
  end
end
"#,
    )
    .await;
}

/// A loop runs its operator assignment any number of times; a block reads the
/// outer local it has just updated.
#[tokio::test]
async fn operator_writes_repeated_by_a_loop_or_a_block_keep_their_class() {
    check(
        r#"
class Report
  def counted(limit)
    count = 0
    while count < limit
      count += 1
    end
    cou<type label="Integer">nt
  end

  def summed
    total = 0
    [1, 2].each do |item|
      total += 1
      tot<type label="Integer">al
    end
  end
end
"#,
    )
    .await;
}
