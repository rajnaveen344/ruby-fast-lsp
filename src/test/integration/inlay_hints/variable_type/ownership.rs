//! Proof isolation for nonlocal variable assignment hints.

use crate::test::harness::check;

#[tokio::test]
async fn unknown_instance_variable_write_does_not_reuse_an_earlier_concrete_type() {
    check(
        r#"
class First
  def initialize
    @value<hint label=": String"> = "known"
    @value<hint label=": ?"> = dynamic_value
  end
end

class Second
  def initialize
    @value<hint label=": ?"> = another_dynamic_value
  end
end
"#,
    )
    .await;
}
