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

#[tokio::test]
async fn class_eval_block_preserves_lexical_constant_scope() {
    check(
        r#"
class MetaTarget
end

module LexicalOwner
  <def>VALUE = "lexical"</def>

  ::MetaTarget.class_eval do
    def value
      VALUE$0
    end
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn class_exec_block_preserves_lexical_constant_scope() {
    check(
        r#"
class MetaTarget
end

module LexicalOwner
  <def>VALUE = "lexical"</def>

  ::MetaTarget.class_exec do
    def value
      VALUE$0
    end
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn instance_exec_block_preserves_lexical_constant_scope() {
    check(
        r#"
class MetaTarget
end

module LexicalOwner
  <def>VALUE = "lexical"</def>

  ::MetaTarget.instance_exec do
    def value
      VALUE$0
    end
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn module_exec_block_preserves_lexical_constant_scope() {
    check(
        r#"
module MetaTarget
end

module LexicalOwner
  <def>VALUE = "lexical"</def>

  ::MetaTarget.module_exec do
    def value
      VALUE$0
    end
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn instance_eval_block_preserves_lexical_constant_scope() {
    check(
        r#"
class MetaTarget
end

module LexicalOwner
  <def>VALUE = "lexical"</def>

  ::MetaTarget.instance_eval do
    def value
      VALUE$0
    end
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn define_method_block_preserves_lexical_constant_scope() {
    check(
        r#"
class MetaTarget
end

module LexicalOwner
  <def>VALUE = "lexical"</def>

  ::MetaTarget.send(:define_method, :value) do
    VALUE$0
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn define_singleton_method_block_preserves_lexical_constant_scope() {
    check(
        r#"
class MetaTarget
end

module LexicalOwner
  <def>VALUE = "lexical"</def>

  ::MetaTarget.define_singleton_method(:value) do
    VALUE$0
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_const_get_literal_symbol_constant() {
    check(
        r#"
module SampleApp
  module Platform
    module Util
      <def>class TriggerHelpers
      end</def>
    end
  end
end

helper = SampleApp::Platform::Util.const_get(:TriggerHelpers$0)
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_self_const_defined_literal_symbol_constant() {
    check(
        r#"
class PushUnit
  <def>TYPE = "push"</def>

  def self.type
    self.const_defined?(:TYPE$0) ? self.const_get(:TYPE) : nil
  end
end
"#,
    )
    .await;
}
