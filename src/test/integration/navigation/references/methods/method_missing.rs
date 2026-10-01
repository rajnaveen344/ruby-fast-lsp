//! References between `method_missing` definitions and dynamic calls.

use crate::test::harness::check;

#[tokio::test]
async fn references_method_missing_include_dynamic_call() {
    check(
        r#"
class DynamicRecord
  def method_missing$0(name, *args)
    "dynamic"
  end
end

DynamicRecord.new.<ref>virtual_total</ref>
"#,
    )
    .await;
}

#[tokio::test]
async fn references_method_missing_from_dynamic_call() {
    check(
        r#"
class DynamicRecord
  def method_missing(name, *args)
    "dynamic"
  end
end

DynamicRecord.new.<ref>virtual_total$0</ref>
"#,
    )
    .await;
}
