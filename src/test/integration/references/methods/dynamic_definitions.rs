//! References for `class_eval`, `define_method`, static `send`, and method-object definitions.

use crate::test::harness::check;

#[tokio::test]
async fn references_method_defined_inside_class_eval_block() {
    check(
        r#"
def patched
  "top-level"
end

class MetaTarget
end

MetaTarget.class_eval do
  def patched$0
    "patched"
  end
end

MetaTarget.new.<ref>patched</ref>
"#,
    )
    .await;
}

#[tokio::test]
async fn references_method_defined_with_define_method_symbol() {
    check(
        r#"
def patched
  "top-level"
end

class MetaTarget
  define_method(:patched$0) do
    "patched"
  end
end

MetaTarget.new.<ref>patched</ref>
"#,
    )
    .await;
}

#[tokio::test]
async fn references_method_defined_with_const_get_send_define_method() {
    check(
        r#"
module Net
  class SMTP
  end
end

Net.const_get(:SMTP).send(:define_method, :tls?$0) do
  true
end

Net::SMTP.new.<ref>tls?</ref>
"#,
    )
    .await;
}

#[tokio::test]
async fn references_static_send_symbol_resolves_target_method() {
    check(
        r#"
def patched
  "top-level"
end

class MetaTarget
  def patched$0
    "patched"
  end
end

MetaTarget.new.send(:<ref>patched</ref>)
"#,
    )
    .await;
}

#[tokio::test]
async fn references_method_object_symbol_resolves_class_method() {
    check(
        r#"
class FeatureSettings
  def self.get$0
    true
  end
end

FeatureSettings.method(:<ref>get</ref>)
FeatureSettings.<ref>get</ref>
"#,
    )
    .await;
}

#[tokio::test]
async fn references_instance_method_object_symbol_resolves_instance_method() {
    check(
        r#"
class SFTPHelpers
  def copy_data_from_remote$0
    true
  end
end

SFTPHelpers.instance_method(:<ref>copy_data_from_remote</ref>)
SFTPHelpers.new.<ref>copy_data_from_remote</ref>
"#,
    )
    .await;
}
