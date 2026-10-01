//! Goto definition for methods created with `define_method`, `define_singleton_method`, and `send`.

use crate::test::harness::check;

#[tokio::test]
async fn define_method_inside_class_exec_uses_runtime_definition_owner() {
    check(
        r#"
class MetaTarget
end

module LexicalOwner
  MetaTarget.class_exec do
    define_method(:<def>patched</def>) do
      "patched"
    end
  end
end

MetaTarget.new.patched$0
"#,
    )
    .await;
}

#[tokio::test]
async fn define_method_block_uses_defined_instance_as_implicit_receiver() {
    check(
        r#"
class MetaTarget
  <def>def target_helper
    "instance"
  end</def>

  def self.target_helper
    "singleton"
  end

  define_method(:patched) do
    target_helper$0
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn explicit_receiver_define_method_block_uses_target_instance() {
    check(
        r#"
class MetaTarget
  <def>def target_helper
    "instance"
  end</def>
end

def target_helper
  "top-level"
end

MetaTarget.send(:define_method, :patched) do
  target_helper$0
end
"#,
    )
    .await;
}

#[tokio::test]
async fn define_method_inside_instance_eval_block_uses_target_instance() {
    check(
        r#"
class MetaTarget
  <def>def target_helper
    "instance"
  end</def>

  def self.target_helper
    "singleton"
  end
end

MetaTarget.instance_eval do
  define_method(:patched) do
    target_helper$0
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn define_method_inside_instance_eval_defines_target_instance_method() {
    check(
        r#"
class MetaTarget
end

MetaTarget.instance_eval do
  define_method(:<def>patched</def>) do
    "patched"
  end
end

MetaTarget.new.patched$0
"#,
    )
    .await;
}

#[tokio::test]
async fn explicit_receiver_define_singleton_method_block_uses_target_singleton() {
    check(
        r#"
class MetaTarget
  <def>def self.target_helper
    "singleton"
  end</def>
end

def target_helper
  "top-level"
end

MetaTarget.define_singleton_method(:patched) do
  target_helper$0
end
"#,
    )
    .await;
}

#[tokio::test]
async fn define_method_in_singleton_class_uses_singleton_method_receiver() {
    check(
        r#"
class MetaTarget
  class << self
    <def>def target_helper
      "singleton"
    end</def>

    define_method(:patched) do
      target_helper$0
    end
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn define_method_in_singleton_class_defines_singleton_method() {
    check(
        r#"
class MetaTarget
  class << self
    define_method(:<def>patched</def>) do
      "patched"
    end
  end
end

MetaTarget.patched$0
"#,
    )
    .await;
}

#[tokio::test]
async fn send_define_singleton_method_defines_and_executes_on_target_singleton() {
    check(
        r#"
class MetaTarget
  <def>def self.target_helper
    "singleton"
  end</def>
end

def target_helper
  "top-level"
end

MetaTarget.send(:define_singleton_method, :patched) do
  target_helper$0
end
"#,
    )
    .await;
}

#[tokio::test]
async fn define_method_block_preserves_lexical_method_definition_owner() {
    check(
        r#"
class MetaTarget
end

class LexicalOwner
  MetaTarget.send(:define_method, :patched) do
    <def>def nested
      "lexical"
    end</def>
  end
end

LexicalOwner.new.nested$0
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_method_defined_with_define_method_symbol() {
    check(
        r#"
def patched
  "top-level"
end

class MetaTarget
  define_method(:<def>patched</def>) do
    "patched"
  end
end

MetaTarget.new.patched$0
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_singleton_method_defined_with_symbol_in_class_body() {
    check(
        r#"
class MetaTarget
  define_singleton_method(:<def>patched</def>) do
    "patched"
  end
end

MetaTarget.patched$0
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_singleton_method_defined_on_constant_receiver() {
    check(
        r#"
class MetaTarget
end

MetaTarget.define_singleton_method(:<def>patched</def>) do
  "patched"
end

MetaTarget.patched$0
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_method_defined_with_constant_send_define_method() {
    check(
        r#"
def patched
  "top-level"
end

class MetaTarget
end

MetaTarget.send(:define_method, :<def>patched</def>) do
  "patched"
end

MetaTarget.new.patched$0
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_method_defined_with_const_get_send_define_method() {
    check(
        r#"
module Net
  class SMTP
  end
end

Net.const_get(:SMTP).send(:define_method, :<def>tls?</def>) do
  true
end

Net::SMTP.new.tls?$0
"#,
    )
    .await;
}
