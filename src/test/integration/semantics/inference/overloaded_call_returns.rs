//! The type of a call whose signature has several overloads.
//!
//! The arguments select the overload. When the overloads return different
//! types and no single one is proven to apply, the call has no proven type.

use crate::test::harness::check;

/// Operands of one class select the overload that returns that class.
#[tokio::test]
async fn operands_of_one_class_select_their_overload() {
    check(
        r#"
class Report
  def counted
    sum = 1 + 2
    su<type label="Integer">m
  end

  def joined
    text = "a" + "b"
    tex<type label="String">t
  end
end
"#,
    )
    .await;
}

/// A mixed operation returns the class its argument selects, never the first
/// overload's.
#[tokio::test]
async fn mixed_operands_follow_the_argument_in_a_method() {
    check(
        r#"
class Report
  def widened
    total = 1 + 0.5
    tot<type label="Float">al
  end

  def scaled
    area = 2.0 * 3
    are<type label="Float">a
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn mixed_operands_follow_the_argument_at_the_top_level() {
    check(
        r#"
total = 1 + 0.5
tot<type label="Float">al

sum = 1 + 2
su<type label="Integer">m
"#,
    )
    .await;
}

/// A method returning a mixed operation is typed like the operation.
#[tokio::test]
async fn method_returning_a_mixed_operation() {
    check(
        r#"
class Report
  def ratio
    1 + 0.5
  end

  def shown
    value = ratio
    val<type label="Float">ue
  end
end

outside = Report.new.ratio
outsi<type label="Float">de
"#,
    )
    .await;
}

/// An argument of unknown type selects no overload.
#[tokio::test]
async fn unknown_argument_leaves_differing_overloads_unknown() {
    check(
        r#"
class Report
  def grown(given)
    count = 1
    total = count + given
    tot<type label="?">al
  end
end
"#,
    )
    .await;
}
