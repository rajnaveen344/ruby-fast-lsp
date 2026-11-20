//! Reads of a local after a block that reassigns it.
//!
//! A block may run any number of times, including zero, so the local has no
//! single reaching assignment after it. The read must stay unknown instead of
//! showing the type assigned before the block.

use crate::test::harness::check;

#[tokio::test]
async fn read_after_block_that_reassigns_the_local_is_unknown() {
    check(
        r#"
class Report
  def plain(items)
    value = 1
    items.each do |item|
      value = "s"
    end
    val<type label="?">ue
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn read_after_block_that_compound_assigns_the_local_is_unknown() {
    check(
        r#"
class Report
  def compound(items)
    value = 1
    items.each do |item|
      value += 0.5
    end
    val<type label="?">ue
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn read_after_block_that_leaves_the_local_alone_keeps_its_type() {
    check(
        r#"
class Report
  def untouched(items)
    value = 1
    items.each do |item|
      item
    end
    val<type label="Integer">ue
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn documented_parameter_reassigned_in_a_block_reads_unknown() {
    check(
        r#"
class Report
  # @param value [Integer]
  def documented(value, items)
    items.each do |item|
      value = "s"
    end
    val<type label="?">ue
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn hover_agrees_with_the_unknown_read() {
    check(
        r#"
class Report
  def plain(items)
    value = 1
    items.each do |item|
      value = "s"
    end
    val<hover label="Unknown[ambiguous_reaching_assignment]">ue
  end
end
"#,
    )
    .await;
}
