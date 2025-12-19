//! Find references tests for methods.

use crate::test::harness::check_references;

/// Find references for instance method.
#[tokio::test]
async fn references_instance_method() {
    check_references(
        r#"
class Greeter
  def greet$0
  end

  def run
    <ref>greet</ref>
  end
end
"#,
    )
    .await;
}

/// Find references for top-level method.
#[tokio::test]
async fn references_top_level_method() {
    check_references(
        r#"
def helper$0
end

<ref>helper</ref>
x = <ref>helper</ref>
"#,
    )
    .await;
}
