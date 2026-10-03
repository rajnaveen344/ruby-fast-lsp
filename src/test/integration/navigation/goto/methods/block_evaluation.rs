//! Goto definition through `class_eval`, `class_exec`, `module_exec`, `instance_eval`, and `instance_exec` blocks.

use crate::test::harness::{check, FakeEditor};

#[tokio::test]
async fn goto_method_defined_inside_class_eval_block() {
    check(
        r#"
def patched
  "top-level"
end

class MetaTarget
end

MetaTarget.class_eval do
  <def>def patched
    "patched"
  end</def>
end

MetaTarget.new.patched$0
"#,
    )
    .await;
}

#[tokio::test]
async fn class_eval_changes_method_owner_without_changing_lexical_namespace() {
    check(
        r#"
class MetaTarget
end

module LexicalOwner
  ::MetaTarget.class_eval do
    <def>def patched
      "patched"
    end</def>
  end
end

MetaTarget.new.patched$0
"#,
    )
    .await;
}

#[tokio::test]
async fn class_eval_declared_instance_method_uses_instance_receiver() {
    check(
        r#"
class MetaTarget
  <def>def target_helper
    "target"
  end</def>

  def self.target_helper
    "target singleton"
  end
end

module LexicalOwner
  def self.target_helper
    "lexical"
  end

  ::MetaTarget.class_eval do
    def patched
      target_helper$0
    end
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn class_eval_block_expression_uses_target_singleton_receiver() {
    check(
        r#"
class MetaTarget
  <def>def self.target_helper
    "target"
  end</def>
end

module LexicalOwner
  def self.target_helper
    "lexical"
  end

  ::MetaTarget.class_eval do
    target_helper$0
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_method_defined_inside_module_eval_block() {
    check(
        r#"
def patched
  "top-level"
end

module MetaMixin
  module_eval do
    <def>def patched
      "patched"
    end</def>
  end
end

class MetaTarget
  include MetaMixin
end

MetaTarget.new.patched$0
"#,
    )
    .await;
}

#[tokio::test]
async fn class_exec_changes_method_owner_and_implicit_receiver() {
    check(
        r#"
class MetaTarget
  def target_helper
    "target"
  end
end

module LexicalOwner
  def self.target_helper
    "lexical"
  end

  ::MetaTarget.class_exec do
    <def>def patched
      target_helper
    end</def>
  end
end

MetaTarget.new.patched$0
"#,
    )
    .await;
}

#[tokio::test]
async fn class_exec_declared_instance_method_uses_instance_receiver() {
    check(
        r#"
class MetaTarget
  <def>def target_helper
    "target"
  end</def>

  def self.target_helper
    "target singleton"
  end
end

module LexicalOwner
  def self.target_helper
    "lexical"
  end

  ::MetaTarget.class_exec do
    def patched
      target_helper$0
    end
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn instance_eval_on_constant_changes_method_owner_to_singleton() {
    check(
        r#"
class MetaTarget
  def self.target_helper
    "target"
  end
end

module LexicalOwner
  def self.target_helper
    "lexical"
  end

  ::MetaTarget.instance_eval do
    <def>def patched
      target_helper
    end</def>
  end
end

MetaTarget.patched$0
"#,
    )
    .await;
}

#[tokio::test]
async fn instance_exec_on_constant_changes_implicit_receiver_to_singleton() {
    check(
        r#"
class MetaTarget
  <def>def self.target_helper
    "target"
  end</def>
end

module LexicalOwner
  def self.target_helper
    "lexical"
  end

  ::MetaTarget.instance_exec do
    target_helper$0
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn module_exec_changes_method_owner_and_implicit_receiver() {
    check(
        r#"
module MetaMixin
  <def>def target_helper
    "target"
  end</def>

  def self.target_helper
    "target singleton"
  end
end

module LexicalOwner
  def self.target_helper
    "lexical"
  end

  ::MetaMixin.module_exec do
    def patched
      target_helper$0
    end
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn instance_exec_defines_singleton_method_on_target_constant() {
    check(
        r#"
class MetaTarget
end

MetaTarget.instance_exec do
  <def>def patched
    "patched"
  end</def>
end

MetaTarget.patched$0
"#,
    )
    .await;
}

#[tokio::test]
async fn nested_unqualified_instance_eval_changes_definition_owner_to_singleton() {
    check(
        r#"
class MetaTarget
end

module LexicalOwner
  ::MetaTarget.class_eval do
    instance_eval do
      <def>def patched
        "patched"
      end</def>
    end
  end
end

MetaTarget.patched$0
"#,
    )
    .await;
}

#[tokio::test]
async fn nested_unqualified_class_eval_changes_definition_owner_to_instance() {
    check(
        r#"
class MetaTarget
end

module LexicalOwner
  ::MetaTarget.instance_eval do
    class_eval do
      <def>def patched
        "patched"
      end</def>
    end
  end
end

MetaTarget.new.patched$0
"#,
    )
    .await;
}

#[tokio::test]
async fn string_eval_forms_do_not_use_block_execution_semantics() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "string_eval.rb",
            r#"class MetaTarget
end

MetaTarget.class_eval("def generated_from_string; end")
MetaTarget.module_eval("def generated_from_module_string; end")
MetaTarget.instance_eval("def generated_singleton_from_string; end")
MetaTarget.new.generated_from_string
MetaTarget.new.generated_from_module_string
MetaTarget.generated_singleton_from_string
"#,
        )
        .await;

    assert!(
        editor.goto_def_at("string_eval.rb", 6, 20).await.is_empty(),
        "class_eval string must not inherit block lexical semantics or synthesize parser facts"
    );
    assert!(
        editor.goto_def_at("string_eval.rb", 7, 20).await.is_empty(),
        "module_eval string must not inherit block lexical semantics or synthesize parser facts"
    );
    assert!(
        editor.goto_def_at("string_eval.rb", 8, 12).await.is_empty(),
        "instance_eval string must not inherit block lexical semantics or synthesize parser facts"
    );
}

#[tokio::test]
async fn class_eval_on_const_get_receiver_changes_method_owner() {
    check(
        r#"
module Outer
  class Inner
  end
end

Outer.const_get(:Inner).class_eval do
  <def>def patched
    "patched"
  end</def>
end

Outer::Inner.new.patched$0
"#,
    )
    .await;
}
