//! Goto definition through included modules, `extend self`, inheritance, and includer overrides.

use crate::test::harness::{check, FakeEditor};

/// Bare calls inside included modules dispatch on the actual including object.
#[tokio::test]
async fn goto_bare_method_follows_includer_override() {
    check(
        r#"
module Trackable
  def audit
    "module"
  end

  def record
    audit$0
  end
end

class Invoice
  include Trackable

  <def>def audit
    "class"
  end</def>
end
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_bare_method_ignores_unrelated_class_after_reopen() {
    let mut editor = FakeEditor::new().await;
    let module = r#"
module Trackable
  def audit
    "module"
  end

  def record
    audit
  end
end
"#;
    let class = r#"
class Invoice
  def audit
    "class"
  end
end
"#;

    editor.open("invoice.rb", class).await;
    editor.open("trackable.rb", module).await;
    editor.close("trackable.rb").await;
    editor.open("trackable.rb", module).await;

    let defs = editor.goto_def_at("trackable.rb", 7, 4).await;
    assert!(
        defs.iter().any(|location| {
            location.uri.path().ends_with("trackable.rb") && location.range.start.line == 2
        }),
        "expected bare audit to resolve inside Trackable after reopen, got {defs:?}"
    );
}

#[tokio::test]
async fn goto_bare_generated_method_follows_includer_override_after_reopen() {
    let mut editor = FakeEditor::new().await;
    let module = r#"
module AtlasScaleMixins
  module Mixin0000
    def flow_0000_00
      "module"
    end

    def flow_0000_01
      flow_0000_00
    end
  end
end
"#;
    let class = r#"
module AtlasDomain00
  class Model0000
    include AtlasScaleMixins::Mixin0000

    def flow_0000_00
      "class"
    end
  end
end
"#;

    editor.open("atlas_domain00/model0000.rb", class).await;
    editor.open("atlas_scale_mixins/mixin0000.rb", module).await;
    editor.close("atlas_scale_mixins/mixin0000.rb").await;
    editor.open("atlas_scale_mixins/mixin0000.rb", module).await;

    let defs = editor
        .goto_def_at("atlas_scale_mixins/mixin0000.rb", 8, 6)
        .await;
    assert_eq!(
        defs.len(),
        1,
        "the known includer has one effective override: {defs:?}"
    );
    assert!(
        defs[0].uri.path().ends_with("atlas_domain00/model0000.rb")
            && defs[0].range.start.line == 5,
        "the generated bare flow call must reach the includer's override after reopen: {defs:?}"
    );
}

#[tokio::test]
async fn goto_extend_self_module_method() {
    check(
        r#"
module Utils
  extend self

  <def>def helper
    "helping"
  end</def>
end

Utils.helper$0
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_module_function_method_through_module_receiver() {
    check(
        r#"
module Utils
  module_function

  <def>def helper
    "helping"
  end</def>
end

Utils.helper$0
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_extended_module_method_through_subclass_receiver() {
    check(
        r#"
module ClassHelpers
  <def>def configure
    "configured"
  end</def>
end

class Base
  extend ClassHelpers
end

class Child < Base
end

Child.configure$0
"#,
    )
    .await;
}

/// Goto definition for method from included module.
#[tokio::test]
async fn goto_included_module_method() {
    check(
        r#"
module Loggable
  <def>def log
    puts "logging"
  end</def>
end

class App
  include Loggable

  def run
    log$0
  end
end
"#,
    )
    .await;
}

/// Goto definition for method from module included in another module.
#[tokio::test]
async fn goto_cross_module_method() {
    check(
        r#"
module ModuleA
  <def>def method_a
    "from A"
  end</def>
end

module ModuleB
  include ModuleA
end

class TestClass
  include ModuleB

  def test
    method_a$0
  end
end
"#,
    )
    .await;
}

/// Goto definition for method from parent class.
#[tokio::test]
async fn goto_inherited_method() {
    check(
        r#"
class Parent
  <def>def parent_method
    "from parent"
  end</def>
end

class Child < Parent
  def test
    parent_method$0
  end
end
"#,
    )
    .await;
}

/// Goto definition for mixin method through inheritance.
#[tokio::test]
async fn goto_inherited_mixin_method() {
    check(
        r#"
module ApiHelpers
  <def>def api_call
    "api"
  end</def>
end

class BaseController
  include ApiHelpers
end

class AppController < BaseController
  def show
    api_call$0
  end
end
"#,
    )
    .await;
}
