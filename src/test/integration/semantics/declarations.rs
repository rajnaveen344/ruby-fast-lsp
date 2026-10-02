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
