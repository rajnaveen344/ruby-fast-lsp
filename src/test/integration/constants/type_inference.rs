//! Constant value type inference tests.

use crate::test::harness::{check, FakeEditor};

#[tokio::test]
async fn constant_literal_type_is_value_type() {
    check(
        r#"
A<type label="Integer" kind="const"> = 1
"#,
    )
    .await;
}

#[tokio::test]
async fn constant_string_type_is_value_type() {
    check(
        r#"
NAME<type label="String" kind="const"> = "Ada"
"#,
    )
    .await;
}

#[tokio::test]
async fn constant_class_object_type_is_class_reference() {
    check(
        r#"
class User
end

MODEL<type label="Class<User>" kind="const"> = User
"#,
    )
    .await;
}

#[tokio::test]
async fn const_get_literal_symbol_type_is_class_reference() {
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

helper<type label="Class<SampleApp::Platform::Util::TriggerHelpers>" kind="var"> = SampleApp::Platform::Util.const_get(:TriggerHelpers)
"#,
    )
    .await;
}

#[tokio::test]
async fn self_const_get_literal_symbol_type_is_constant_value() {
    check(
        r#"
class PushUnit
  TYPE = "push"

  def self.type
    value<type label="String" kind="var"> = self.const_get(:TYPE)
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn constant_path_literal_type_is_value_type() {
    check(
        r#"
module Foo
end

Foo::A<type label="Integer" kind="const"> = 1
"#,
    )
    .await;
}

#[tokio::test]
async fn constant_type_fact_replaced_after_edit() {
    let mut editor = FakeEditor::new().await;
    editor.open("test.rb", "A = 1").await;
    editor
        .check("test.rb", r#"A<type label="Integer" kind="const"> = 1"#)
        .await;

    editor.set("test.rb", r#"A = "Ada""#).await;
    editor
        .check("test.rb", r#"A<type label="String" kind="const"> = "Ada""#)
        .await;
}
