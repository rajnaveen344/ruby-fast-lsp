//! Find references tests for constants.

use crate::test::harness::check;

/// Find references for a constant.
#[tokio::test]
async fn references_constant() {
    check(
        r#"
VALUE = 42

puts <ref>VALUE$0</ref>
x = <ref>VALUE</ref>
"#,
    )
    .await;
}

/// Find references for qualified constant.
#[tokio::test]
async fn references_qualified_constant() {
    check(
        r#"
module Alpha
  BETA = 100
end

puts <ref>Alpha::BETA$0</ref>
x = <ref>Alpha::BETA</ref>
"#,
    )
    .await;
}

#[tokio::test]
async fn references_const_get_literal_symbol_constant() {
    check(
        r#"
module SampleApp
  module Platform
    module Util
      class <ref>TriggerHelpers</ref>
      end
    end
  end
end

helper = SampleApp::Platform::Util.const_get(:<ref>TriggerHelpers$0</ref>)
klass = <ref>SampleApp::Platform::Util::TriggerHelpers</ref>
"#,
    )
    .await;
}

#[tokio::test]
async fn references_self_const_defined_literal_symbol_constant() {
    check(
        r#"
class PushUnit
  TYPE = "push"

  def self.type
    self.const_defined?(:<ref>TYPE$0</ref>) ? self.const_get(:<ref>TYPE</ref>) : nil
  end
end
"#,
    )
    .await;
}
