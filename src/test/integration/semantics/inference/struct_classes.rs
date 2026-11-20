//! Classes built by `Name = Struct.new(:member, ...)`.
//!
//! The assignment declares a class with a reader and a writer per member,
//! and `Name.new` builds an instance. Member values are not typed. A form
//! whose members are not all written as symbols declares nothing.

//!
//! The harness has no core classes, so each fixture declares the `Struct`
//! the definitions inherit.

use crate::test::harness::check;

/// `Name.new` builds an instance, and its members are callable.
#[tokio::test]
async fn new_builds_an_instance_with_member_accessors() {
    check(
        r#"
class Struct
  def self.new(*members)
  end

  def to_a
    []
  end
end

Point = Struct.new(:x, :y)

point = Point.new(1, 2)
poi<type label="Point">nt
<warn none code="unresolved-method">point.x</warn>
<warn none code="unresolved-method">point.y = 3</warn>
<warn none code="unresolved-method">point.to_a</warn>
point.<warn code="unresolved-method">absent</warn>
"#,
    )
    .await;
}

/// A member's value is whatever the caller passed, so it stays unknown.
#[tokio::test]
async fn member_value_stays_unknown() {
    check(
        r#"
class Struct
  def self.new(*members)
  end

  def to_a
    []
  end
end

Point = Struct.new(:x, :y)

point = Point.new(1, 2)
x_value = point.x
x_val<type label="?">ue
"#,
    )
    .await;
}

/// Members may be omitted or passed by keyword; neither call is reported.
#[tokio::test]
async fn any_member_argument_form_is_accepted() {
    check(
        r#"
class Struct
  def self.new(*members)
  end

  def to_a
    []
  end
end

Point = Struct.new(:x, :y)
Pair = Struct.new(:left, :right, keyword_init: true)

<warn none>empty = Point.new</warn>
emp<type label="Point">ty
<warn none>partial = Point.new(1)</warn>
parti<type label="Point">al
<warn none>named = Point.new(x: 1, y: 2)</warn>
nam<type label="Point">ed
<warn none>pair = Pair.new(left: 1, right: 2)</warn>
pa<type label="Pair">ir
"#,
    )
    .await;
}

/// Methods defined in the block belong to the class, on the side `def`
/// names.
#[tokio::test]
async fn block_methods_belong_to_the_class() {
    check(
        r#"
class Struct
  def self.new(*members)
  end

  def to_a
    []
  end
end

Point = Struct.new(:x, :y) do
  def total
    <warn none code="unresolved-method">x.to_s + y.to_s</warn>
  end

  def label
    "point"
  end

  def self.origin
    new(0, 0)
  end

  def self.unit
    Point.new(1, 1)
  end
end

point = Point.new(1, 2)
<warn none code="unresolved-method">point.total</warn>
label = point.label
lab<type label="String">el
<warn none code="unresolved-method">Point.origin</warn>
unit = Point.unit
un<type label="Point">it
"#,
    )
    .await;
}

/// A subclass of the built class builds its own instances.
#[tokio::test]
async fn subclass_builds_its_own_instances() {
    check(
        r#"
class Struct
  def self.new(*members)
  end

  def to_a
    []
  end
end

Point = Struct.new(:x, :y)

class Pixel < Point
  def shade
    1
  end
end

pixel = Pixel.new(1, 2)
pix<type label="Pixel">el
<warn none code="unresolved-method">pixel.x</warn>
pixel.<warn code="unresolved-method">absent</warn>
shade = pixel.shade
sha<type label="Integer">de
"#,
    )
    .await;
}

/// A class written as a direct `Struct` subclass still builds a new class
/// from `new`, not an instance.
#[tokio::test]
async fn direct_struct_subclass_does_not_build_an_instance() {
    check(
        r#"
class Struct
  def self.new(*members)
  end

  def to_a
    []
  end
end

class Base < Struct
end

made = Base.new(:a)
ma<type label="?">de
"#,
    )
    .await;
}

/// Members that are not plain symbols, a leading class name, a conditional
/// assignment and a scoped target declare nothing.
#[tokio::test]
async fn unrecognised_forms_declare_nothing() {
    check(
        r#"
class Struct
  def self.new(*members)
  end

  def to_a
    []
  end
end

FIELDS = [:a, :b]
module Outer
end

Named = Struct.new("Named", :a)
Spread = Struct.new(*FIELDS)
Lazy ||= Struct.new(:a)
Outer::Inner = Struct.new(:a)

named = Named.new(1)
nam<type label="?">ed
spread = Spread.new(1)
spre<type label="?">ad
lazy = Lazy.new(1)
la<type label="?">zy
inner = Outer::Inner.new(1)
inn<type label="?">er
<warn none code="unresolved-method">named.anything</warn>
<warn none code="unresolved-method">spread.anything</warn>
<warn none code="unresolved-method">lazy.anything</warn>
<warn none code="unresolved-method">inner.anything</warn>
"#,
    )
    .await;
}

/// A `Struct` constant that is not the core class proves nothing.
#[tokio::test]
async fn shadowed_struct_constant_proves_nothing() {
    check(
        r#"
module Shapes
  class Struct
    def self.new(*names)
      names
    end
  end

  Point = Struct.new(:x, :y)

  point = Point.new(1, 2)
  poi<type label="?">nt
  <warn none code="unresolved-method">point.anything</warn>
end
"#,
    )
    .await;
}

/// Reopening the class adds to it and keeps `Struct` as its parent.
#[tokio::test]
async fn reopened_class_keeps_its_members() {
    check(
        r#"
class Struct
  def self.new(*members)
  end

  def to_a
    []
  end
end

Point = Struct.new(:x, :y)

class Point
  def total
    1
  end
end

point = Point.new(1, 2)
poi<type label="Point">nt
<warn none code="unresolved-method">point.x</warn>
<warn none code="unresolved-method">point.to_a</warn>
total = point.total
tot<type label="Integer">al
point.<warn code="unresolved-method">absent</warn>
"#,
    )
    .await;
}

/// A definition nested in a module is named inside it.
#[tokio::test]
async fn nested_definition_is_named_in_its_module() {
    check(
        r#"
class Struct
  def self.new(*members)
  end
end

module Shapes
  Point = Struct.new(:x, :y) do
    def self.unit
      Point.new(1, 1)
    end

    def twin
      Point.new(x, y)
    end
  end
end

class Canvas
  def draw
    inner = Shapes::Point.unit
    inn<type label="Shapes::Point">er
  end
end

point = Shapes::Point.new(1, 2)
poi<type label="Shapes::Point">nt
unit = Shapes::Point.unit
un<type label="Shapes::Point">it
twin = point.twin
tw<type label="Shapes::Point">in
point.<warn code="unresolved-method">absent</warn>
"#,
    )
    .await;
}
