//! Goto definition tests for methods.

use crate::test::harness::{check, FakeEditor};

// ============================================================================
// Instance Methods
// ============================================================================

/// Goto definition for instance method call.
#[tokio::test]
async fn goto_instance_method() {
    check(
        r#"
class Greeter
  <def>def greet
    puts "Hello"
  end</def>

  def run
    greet$0
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_super_resolves_parent_method() {
    check(
        r#"
class ParentProcessor
  <def>def process
    "parent"
  end</def>
end

class ChildProcessor < ParentProcessor
  def process
    super$0
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_missing_method_resolves_method_missing() {
    check(
        r#"
class DynamicRecord
  <def>def method_missing(name, *args)
    "dynamic"
  end</def>
end

DynamicRecord.new.virtual_total$0
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_explicit_receiver_private_method_does_not_resolve() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "visibility_private_receiver.rb",
            r#"class Vault
  private

  def secret
    "token"
  end
end

Vault.new.secret
Vault.new.send(:secret)
"#,
        )
        .await;

    let explicit_receiver_defs = editor
        .goto_def_at("visibility_private_receiver.rb", 8, 11)
        .await;
    assert!(
        explicit_receiver_defs.is_empty(),
        "explicit receiver must not resolve a private method, got {explicit_receiver_defs:?}"
    );

    let send_defs = editor
        .goto_def_at("visibility_private_receiver.rb", 9, 17)
        .await;
    assert_eq!(
        send_defs.len(),
        1,
        "send should resolve a private method because Ruby send bypasses visibility"
    );
}

#[tokio::test]
async fn goto_visibility_argument_form_filters_explicit_receivers() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "visibility_argument_form.rb",
            r#"class Vault
  def secret
    "token"
  end
  private :secret

  def semi_secret
    "token"
  end
  protected :semi_secret
end

Vault.new.secret
Vault.new.send(:secret)
Vault.new.semi_secret
"#,
        )
        .await;

    let private_explicit_defs = editor
        .goto_def_at("visibility_argument_form.rb", 12, 10)
        .await;
    assert!(
        private_explicit_defs.is_empty(),
        "explicit receiver must not resolve private :name methods, got {private_explicit_defs:?}"
    );

    let private_send_defs = editor
        .goto_def_at("visibility_argument_form.rb", 13, 17)
        .await;
    assert_eq!(
        private_send_defs.len(),
        1,
        "send should resolve private :name methods because Ruby send bypasses visibility"
    );

    let protected_explicit_defs = editor
        .goto_def_at("visibility_argument_form.rb", 14, 10)
        .await;
    assert!(
        protected_explicit_defs.is_empty(),
        "external explicit receiver must not resolve protected :name methods, got {protected_explicit_defs:?}"
    );
}

#[tokio::test]
async fn goto_protected_method_allows_same_family_explicit_receiver() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "protected_same_family.rb",
            r#"class Vault
  def semi_secret
    "token"
  end
  protected :semi_secret

  def compare
    other = Vault.new
    other.semi_secret
  end
end

Vault.new.semi_secret
"#,
        )
        .await;

    let same_family_defs = editor.goto_def_at("protected_same_family.rb", 8, 10).await;
    assert_eq!(
        same_family_defs.len(),
        1,
        "same-family explicit receiver should resolve protected method, got {same_family_defs:?}"
    );

    let external_defs = editor.goto_def_at("protected_same_family.rb", 12, 10).await;
    assert!(
        external_defs.is_empty(),
        "external explicit receiver must not resolve protected method, got {external_defs:?}"
    );
}

#[tokio::test]
async fn goto_private_visibility_override_filters_included_method_explicit_receiver() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "mixin_visibility_override.rb",
            r#"module SharedSecret
  def hidden
    "hidden"
  end
end

class Vault
  include SharedSecret
  private :hidden

  def reveal
    hidden
  end
end

Vault.new.hidden
"#,
        )
        .await;

    let bare_defs = editor
        .goto_def_at("mixin_visibility_override.rb", 11, 5)
        .await;
    assert_eq!(
        bare_defs.len(),
        1,
        "bare call should still resolve included private method, got {bare_defs:?}"
    );

    let explicit_defs = editor
        .goto_def_at("mixin_visibility_override.rb", 15, 10)
        .await;
    assert!(
        explicit_defs.is_empty(),
        "private override on included method must block external explicit receiver, got {explicit_defs:?}"
    );
}

#[tokio::test]
async fn goto_public_visibility_override_allows_included_private_method_explicit_receiver() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "mixin_public_visibility_override.rb",
            r#"module SharedSecret
  private

  def hidden
    "hidden"
  end
end

class Vault
  include SharedSecret
  public :hidden
end

Vault.new.hidden
"#,
        )
        .await;

    let explicit_defs = editor
        .goto_def_at("mixin_public_visibility_override.rb", 13, 10)
        .await;
    assert_eq!(
        explicit_defs.len(),
        1,
        "public override on included private method should allow explicit receiver, got {explicit_defs:?}"
    );
}

#[tokio::test]
async fn goto_protected_visibility_override_allows_same_family_included_method_receiver() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "mixin_protected_visibility_override.rb",
            r#"module SharedSecret
  def hidden
    "hidden"
  end
end

class Vault
  include SharedSecret
  protected :hidden

  def compare
    other = Vault.new
    other.hidden
  end
end

Vault.new.hidden
"#,
        )
        .await;

    let same_family_defs = editor
        .goto_def_at("mixin_protected_visibility_override.rb", 12, 10)
        .await;
    assert_eq!(
        same_family_defs.len(),
        1,
        "protected override should allow same-family explicit receiver, got {same_family_defs:?}"
    );

    let external_defs = editor
        .goto_def_at("mixin_protected_visibility_override.rb", 16, 10)
        .await;
    assert!(
        external_defs.is_empty(),
        "protected override should block external explicit receiver, got {external_defs:?}"
    );
}

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

/// Bare method calls resolve inside their lexical namespace before unrelated same-name methods.
#[tokio::test]
async fn goto_bare_method_prefers_current_module_namespace() {
    check(
        r#"
module Trackable
  <def>def audit
    "module"
  end</def>

  def record
    audit$0
  end
end

class Invoice
  include Trackable

  def audit
    "class"
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_bare_method_prefers_current_module_namespace_after_same_name_class_reopen() {
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
async fn goto_bare_generated_method_prefers_current_module_over_includer() {
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
    assert!(
        defs.iter().any(|location| {
            location
                .uri
                .path()
                .ends_with("atlas_scale_mixins/mixin0000.rb")
                && location.range.start.line == 3
        }),
        "expected generated bare flow call to resolve inside Mixin0000, got {defs:?}"
    );
}

/// Goto definition for method call on instance.
#[tokio::test]
async fn goto_method_on_instance() {
    check(
        r#"
class Foo
  <def>def bar
    42
  end</def>
end

Foo.new.bar$0
"#,
    )
    .await;
}

// ============================================================================
// Class Methods
// ============================================================================

/// Goto definition for class method call.
#[tokio::test]
async fn goto_class_method() {
    check(
        r#"
class Utils
  <def>def self.process
    "processing"
  end</def>
end

Utils.process$0
"#,
    )
    .await;
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
async fn goto_alias_method_call() {
    check(
        r#"
class User
  def name
    "n"
  end

  <def>alias full_name name</def>
end

User.new.full_name$0
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_alias_method_call_form() {
    check(
        r#"
class User
  def name
    "n"
  end

  <def>alias_method :display_name, :name</def>
end

User.new.display_name$0
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_delegate_method_call() {
    check(
        r#"
class User
  def name
    "n"
  end
end

class Order
  <def>delegate :name, to: :user</def>

  def user
    User.new
  end
end

Order.new.name$0
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_forwardable_def_delegators_class_method_call() {
    check(
        r#"
class ServiceFlags
  class << self
    extend Forwardable
    <def>def_delegators :instance, :allow?</def>
  end

  def self.instance
    new
  end

  def allow?
    true
  end
end

ServiceFlags.allow?$0
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_forwardable_def_delegator_class_method_call() {
    check(
        r#"
class S3Storage
  class << self
    extend Forwardable
    <def>def_delegator :instance, :get_storage</def>
  end

  def self.instance
    new
  end

  def get_storage
    "storage"
  end
end

S3Storage.get_storage$0
"#,
    )
    .await;
}

// ============================================================================
// Mixins
// ============================================================================

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

// ============================================================================
// Inheritance
// ============================================================================

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

// ============================================================================
// Top-level
// ============================================================================

/// Goto definition for top-level method.
#[tokio::test]
async fn goto_top_level_method() {
    check(
        r#"
<def>def helper
  "help"
end</def>

helper$0
"#,
    )
    .await;
}

// ============================================================================
// Singleton Class (class << self)
// ============================================================================

/// Goto definition for method inside singleton class.
#[tokio::test]
async fn goto_singleton_class_method() {
    check(
        r#"
class Foo
  class << self
    <def>def singleton_method
      "singleton"
    end</def>
  end
end

Foo.singleton_method$0
"#,
    )
    .await;
}

// ============================================================================
// Constructor (initialize -> new)
// ============================================================================

/// Goto definition for .new which maps to initialize.
#[tokio::test]
async fn goto_constructor_via_new() {
    check(
        r#"
class Foo
  <def>def initialize
  end</def>
end

Foo.new$0
"#,
    )
    .await;
}
