//! Hover tests for class constants.

use crate::test::harness::check;

/// Hover on class definition shows "class ClassName"
#[tokio::test]
async fn class_definition() {
    check(
        r#"
class MyClass<hover label="class MyClass">
end
"#,
    )
    .await;
}

/// Hover on class reference shows "class ClassName"
#[tokio::test]
async fn class_reference() {
    check(
        r#"
class Foo; end
x = Foo<hover label="class Foo">.new
"#,
    )
    .await;
}

#[tokio::test]
async fn const_get_literal_symbol_class_reference() {
    check(
        r#"
module SampleApp
  module Platform
    module Util
      class TriggerHelpers
      end
    end
  end
end

helper = SampleApp::Platform::Util.const_get(:TriggerHelpers<hover label="class SampleApp::Platform::Util::TriggerHelpers">)
"#,
    )
    .await;
}

#[tokio::test]
async fn self_const_defined_literal_symbol_constant_value() {
    check(
        r#"
class PushUnit
  TYPE = "push"

  def self.type
    self.const_defined?(:TYPE<hover label="String">) ? self.const_get(:TYPE) : nil
  end
end
"#,
    )
    .await;
}

/// Hover on method definition shows return type (from YARD)
#[tokio::test]
async fn method_definition_return_type() {
    check(
        r#"
class Foo
  # @return [String]
  def bar<hover label="String">
    "hello"
  end
end
"#,
    )
    .await;
}
