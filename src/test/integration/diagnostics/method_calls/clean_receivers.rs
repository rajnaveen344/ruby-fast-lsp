//! Valid calls whose receivers come from Struct constants must not be reported,
//! and a class whose parent is computed at run time cannot prove a method absent.

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

/// The parent is whatever the call returns, so its methods are not known.
#[tokio::test]
async fn class_with_a_computed_parent_reports_no_missing_method() {
    check(
        r#"
class Point < Struct.new(:x, :y)
  def sum
    <warn none code="unresolved-method">x + y</warn>
  end
end

point = Point.new(1, 2)
<warn none code="unresolved-method">point.x</warn>
<warn none code="unresolved-method">point.y = 3</warn>
<warn none code="unresolved-method">point.to_a</warn>
<warn none code="unresolved-method">point.absent</warn>
<warn none code="unresolved-method">Point.absent</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn class_with_a_called_or_indexed_parent_reports_no_missing_method() {
    check(
        r#"
class Base
  def self.variant(kind)
    self
  end

  def self.[](version)
    self
  end
end

class Special < Base.variant(:fast)
  def run
    <warn none code="unresolved-method">helper</warn>
  end
end

class Versioned < Base[1]
  def run
    <warn none code="unresolved-method">helper</warn>
  end
end

<warn none code="unresolved-method">Special.new.other</warn>
<warn none code="unresolved-method">Versioned.new.other</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn subclass_of_a_class_with_a_computed_parent_reports_no_missing_method() {
    check(
        r#"
class Point < Struct.new(:x, :y)
end

class Pixel < Point
  def sum
    <warn none code="unresolved-method">x + y</warn>
  end
end

<warn none code="unresolved-method">Pixel.new(1, 2).absent</warn>
"#,
    )
    .await;
}

/// A named parent is fully known, so a missing method is still reported.
#[tokio::test]
async fn class_with_a_named_parent_still_reports_a_missing_method() {
    check(
        r#"
class Base
end

class Special < Base
  def run
    <warn code="unresolved-method">helper</warn>
  end
end

Special.new.<warn code="unresolved-method">other</warn>
"#,
    )
    .await;
}
