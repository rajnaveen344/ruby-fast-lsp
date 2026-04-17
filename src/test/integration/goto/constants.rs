//! Goto definition tests for constants.

use crate::test::harness::check;

/// Goto definition for a simple constant.
#[tokio::test]
async fn goto_constant() {
    check(
        r#"
module MyMod
  <def>VALUE = 42</def>

  def get_value
    VALUE$0
  end
end
"#,
    )
    .await;
}

/// Goto definition for a qualified constant path.
#[tokio::test]
async fn goto_qualified_constant() {
    check(
        r#"
module Alpha
  module Beta
    <def>GAMMA = 100</def>
  end
end

puts Alpha::Beta::GAMMA$0
"#,
    )
    .await;
}

/// Goto definition for constant in hash value.
#[tokio::test]
async fn goto_constant_in_hash() {
    check(
        r#"
<def>MY_CONST = "value"</def>

hash = { key: MY_CONST$0 }
"#,
    )
    .await;
}

/// Goto definition for constant in method default argument.
#[tokio::test]
async fn goto_constant_in_default_arg() {
    check(
        r#"
<def>DEFAULT = 42</def>

def test(value = DEFAULT$0)
end
"#,
    )
    .await;
}

/// Goto definition for top-level constant from nested context.
#[tokio::test]
async fn goto_toplevel_constant_from_nested() {
    check(
        r#"
<def>TOP_CONST = "top"</def>

module Nested
  class Inner
    def use_it
      TOP_CONST$0
    end
  end
end
"#,
    )
    .await;
}
