//! Declarations that follow Ruby's rules for where a `def`, constant, or
//! visibility call lands. Open files take these from the declaration pass, so
//! each case pins a rule that pass shares with the fact collector.

use crate::test::harness::check;

/// Only a class has a constructor. A module's `initialize` stays an instance
/// method that runs for the classes that include it.
#[tokio::test]
async fn module_initialize_stays_an_instance_method() {
    check(
        r#"
module Shapes
  module Sized
    <def>def initialize(size)
      @size = size
    end</def>

    def reset
      initialize$0(0)
    end
  end
end
"#,
    )
    .await;
}

/// A module has no `new`, so its `initialize` is not a constructor for it.
#[tokio::test]
async fn module_initialize_declares_no_constructor() {
    check(
        r#"
module Shapes
  module Sized
    def initialize(size)
      @size = size
    end
  end
end

Shapes::Sized.new$0<def none>(1)
"#,
    )
    .await;
}

/// Ruby makes `initialize` private even in a public section, so an explicit
/// receiver cannot call it.
#[tokio::test]
async fn included_initialize_is_private() {
    check(
        r#"
module Shapes
  module Sized
    public

    def initialize(size)
      @size = size
    end
  end

  class Box
    include Sized
  end
end

Shapes::Box.new.initialize$0<def none>(1)
"#,
    )
    .await;
}

/// `module_function :name` copies the method to the module as a public
/// singleton method, even when the original is private.
#[tokio::test]
async fn module_function_copy_of_private_method_is_public() {
    check(
        r#"
module Tools
  private

  <def>def clamp(value)
    value
  end</def>
  module_function :clamp
end

Tools.clamp$0(1)
"#,
    )
    .await;
}

/// `module_function :name` makes the instance method private.
#[tokio::test]
async fn module_function_makes_named_instance_method_private() {
    check(
        r#"
module Tools
  def clamp(value)
    value
  end
  module_function :clamp
end

class Gauge
  include Tools
end

Gauge.new.clamp$0<def none>(1)
"#,
    )
    .await;
}

/// After a bare `module_function`, each `def` is a private instance method
/// and a public singleton method.
#[tokio::test]
async fn module_function_mode_makes_instance_methods_private() {
    check(
        r#"
module Tools
  module_function

  def clamp(value)
    value
  end
end

class Gauge
  include Tools
end

Gauge.new.clamp$0<def none>(1)
"#,
    )
    .await;
}

/// Each constant target of a multiple assignment declares a constant, and the
/// parent of `Parent::NAME` resolves lexically from the enclosing namespace.
#[tokio::test]
async fn multi_write_constant_path_target_resolves_parent_lexically() {
    check(
        r#"
module Shapes
  module Inner; end
  <def>Inner::DEPTH</def>, WIDTH = 3, 4
end

Shapes::Inner::DEPTH$0
"#,
    )
    .await;
}

/// A leading `::` writes a top-level constant even inside a namespace.
#[tokio::test]
async fn multi_write_top_level_constant_target() {
    check(
        r#"
module Shapes
  <def>::ORIGIN</def>, WIDTH = 0, 4
end

ORIGIN$0
"#,
    )
    .await;
}

/// A plain constant target in a multiple assignment declares a constant in
/// the current namespace.
#[tokio::test]
async fn multi_write_constant_target_declares_namespaced_constant() {
    check(
        r#"
module Shapes
  WIDTH, <def>HEIGHT</def> = 3, 4
end

Shapes::HEIGHT$0
"#,
    )
    .await;
}
