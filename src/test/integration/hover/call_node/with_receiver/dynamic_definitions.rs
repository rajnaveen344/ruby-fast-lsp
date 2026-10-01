//! Hover on calls to methods created by `class_eval`, `define_method`, and static `send`.

use crate::test::harness::{check, FakeEditor};

#[tokio::test]
async fn class_eval_method_call_uses_block_method_return_type() {
    check(
        r#"
def patched
  1
end

class MetaTarget
end

MetaTarget.class_eval do
  # @return [String]
  def patched
    "patched"
  end
end

MetaTarget.new.patched<hover label="String">
"#,
    )
    .await;
}

#[tokio::test]
async fn define_method_call_uses_block_method_return_type() {
    check(
        r#"
def patched
  1
end

class MetaTarget
  # @return [String]
  define_method(:patched) do
    "patched"
  end
end

MetaTarget.new.patched<hover label="String">
"#,
    )
    .await;
}

#[tokio::test]
async fn define_method_infers_block_return_using_target_instance_receiver() {
    check(
        r#"
class MetaTarget
  # @return [String]
  def target_helper
    "target"
  end

  define_method(:patched) do
    target_helper
  end
end

MetaTarget.new.patched<hover label="String">
"#,
    )
    .await;
}

#[tokio::test]
async fn define_singleton_method_infers_block_return_using_target_singleton_receiver() {
    check(
        r#"
class MetaTarget
  # @return [String]
  def self.target_helper
    "target"
  end
end

MetaTarget.define_singleton_method(:patched) do
  target_helper
end

MetaTarget.patched<hover label="String">
"#,
    )
    .await;
}

#[tokio::test]
async fn define_method_inferred_return_is_replaced_after_edit() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "dynamic.rb",
            r#"class MetaTarget
  define_method(:patched) do
    "string"
  end
end

MetaTarget.new.patched
"#,
        )
        .await;
    editor
        .check(
            "dynamic.rb",
            r#"class MetaTarget
  define_method(:patched) do
    "string"
  end
end

MetaTarget.new.patched<hover label="String">
"#,
        )
        .await;

    editor
        .set(
            "dynamic.rb",
            r#"class MetaTarget
  define_method(:patched) do
    1
  end
end

MetaTarget.new.patched
"#,
        )
        .await;
    editor
        .check(
            "dynamic.rb",
            r#"class MetaTarget
  define_method(:patched) do
    1
  end
end

MetaTarget.new.patched<hover label="Integer">
"#,
        )
        .await;

    editor
        .set(
            "dynamic.rb",
            r#"class MetaTarget
end

MetaTarget.new.patched
"#,
        )
        .await;
    editor
        .check(
            "dynamic.rb",
            r#"class MetaTarget
end

MetaTarget.new.patched<hover label="?">
"#,
        )
        .await;
}

#[tokio::test]
async fn const_get_define_method_call_uses_block_method_return_type() {
    check(
        r#"
module Net
  class SMTP
  end
end

# @return [String]
Net.const_get(:SMTP).send(:define_method, :tls?) do
  "tls"
end

Net::SMTP.new.tls?<hover label="String">
"#,
    )
    .await;
}

#[tokio::test]
async fn static_send_symbol_call_uses_target_return_type() {
    check(
        r#"
def patched
  1
end

class MetaTarget
  # @return [String]
  def patched
    "patched"
  end
end

MetaTarget.new.send(:patched<hover label="String">)
"#,
    )
    .await;
}
