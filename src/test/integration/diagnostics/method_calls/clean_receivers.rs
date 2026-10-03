//! Valid calls whose receivers come from Struct constants must not be reported.

use crate::test::harness::check;

#[tokio::test]
async fn keyword_init_struct_constant_responds_to_new() {
    check(
        r#"
module Shapes
  Point = Struct.new(:x, :y, keyword_init: true)

  def self.origin
    <warn none code="unresolved-method">Point.new(x: 0, y: 0)</warn>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn positional_struct_constant_responds_to_new() {
    check(
        r#"
module Shapes
  Pair = Struct.new(:a)

  def self.build
    <warn none code="unresolved-method">Pair.new(1)</warn>
  end
end
"#,
    )
    .await;
}
