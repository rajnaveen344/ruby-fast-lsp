//! Goto definition for static `send` symbols and `method`/`instance_method` objects.

use crate::test::harness::check;

#[tokio::test]
async fn goto_static_send_symbol_resolves_target_method() {
    check(
        r#"
def patched
  "top-level"
end

class MetaTarget
  <def>def patched
    "patched"
  end</def>
end

MetaTarget.new.send(:patched$0)
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_method_object_symbol_resolves_class_method() {
    check(
        r#"
class FeatureSettings
  <def>def self.get
    true
  end</def>
end

FeatureSettings.method(:g$0et)
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_instance_method_object_symbol_resolves_instance_method() {
    check(
        r#"
class SFTPHelpers
  <def>def copy_data_from_remote
    true
  end</def>
end

SFTPHelpers.instance_method(:copy_data_from_rem$0ote)
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_bare_method_object_symbol_resolves_current_method() {
    check(
        r#"
class SinatraBase
  <def>def health_checks
    true
  end</def>

  def run
    method(:health_ch$0ecks)
  end
end
"#,
    )
    .await;
}
