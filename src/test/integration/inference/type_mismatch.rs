//! Tests for type mismatches between YARD and RBS.

use crate::test::harness::check;

/// Test that a mismatch between YARD and RBS (for a standard library method)
/// generates a warning diagnostic.
#[tokio::test]
async fn test_yard_rbs_mismatch_diagnostic() {
    check(
        r#"
class Array
  # @return <warn message="YARD return type 'String' conflicts with RBS type 'Integer'">[String]</warn>
  def count
    0
  end
end
"#,
    )
    .await;
}

/// Test that valid YARD documentation matching RBS does not error
#[tokio::test]
async fn test_valid_yard_rbs_match() {
    check(
        r#"
<err none>class Array
  # @return [Integer]
  def count
    0
  end
end</err>
"#,
    )
    .await;
}

/// Test that Inlay Hints also prioritize RBS over YARD.
#[tokio::test]
async fn test_inlay_hint_rbs_priority() {
    // Array#count returns Integer in RBS, but we claim String in YARD.
    // Inlay hint should show `-> Integer` (RBS), not `-> String` (YARD).
    check(
        r#"
class Array
  # @return [String]
  def count<hint label="Integer">
    0
  end
end
"#,
    )
    .await;
}
