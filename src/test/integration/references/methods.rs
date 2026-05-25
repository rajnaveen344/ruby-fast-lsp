//! Find references tests for methods.

use crate::test::harness::{check, FakeEditor};

// ============================================================================
// Instance Methods
// ============================================================================

/// Find references for instance method.
#[tokio::test]
async fn references_instance_method() {
    check(
        r#"
class Greeter
  def greet$0
  end

  def run
    <ref>greet</ref>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn references_parent_method_include_super_call() {
    check(
        r#"
class ParentProcessor
  def process$0
    "parent"
  end
end

class ChildProcessor < ParentProcessor
  def process
    <ref>super</ref>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn references_method_missing_include_dynamic_call() {
    check(
        r#"
class DynamicRecord
  def method_missing$0(name, *args)
    "dynamic"
  end
end

DynamicRecord.new.<ref>virtual_total</ref>
"#,
    )
    .await;
}

#[tokio::test]
async fn references_method_missing_from_dynamic_call() {
    check(
        r#"
class DynamicRecord
  def method_missing(name, *args)
    "dynamic"
  end
end

DynamicRecord.new.<ref>virtual_total$0</ref>
"#,
    )
    .await;
}

#[tokio::test]
async fn references_private_method_exclude_invalid_explicit_receivers() {
    check(
        r#"
class Vault
  private

  def secret
    "token"
  end

  def call_secret
    <ref>secret$0</ref>
    Vault.new.secret
    Vault.new.public_send(:secret)
    send(:<ref>secret</ref>)
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn references_private_method_exclude_cross_file_invalid_explicit_receivers() {
    let mut editor = FakeEditor::new().await;
    let target = r#"
class Vault
  private

  def secret
    "token"
  end
end
"#;
    let caller = r#"
class Caller
  def run
    Vault.new.secret
    Vault.new.send(:secret)
  end
end
"#;

    editor.open("vault.rb", target).await;
    editor.open("caller.rb", caller).await;

    let refs = editor.references_at("vault.rb", 4, 6).await;
    assert!(
        !refs.iter().any(|location| {
            location.uri.path().ends_with("caller.rb") && location.range.start.line == 3
        }),
        "private method references must exclude invalid explicit receiver calls, got {refs:?}"
    );
    assert!(
        refs.iter().any(|location| {
            location.uri.path().ends_with("caller.rb") && location.range.start.line == 4
        }),
        "private method references should keep send(:secret), got {refs:?}"
    );
}

#[tokio::test]
async fn references_visibility_argument_form_exclude_invalid_explicit_receivers() {
    check(
        r#"
class Vault
  def secret
    "token"
  end
  private :secret

  def call_secret
    <ref>secret$0</ref>
    Vault.new.secret
    send(:<ref>secret</ref>)
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn references_protected_method_include_same_family_explicit_receiver() {
    check(
        r#"
class Vault
  def semi_secret$0
    "token"
  end
  protected :semi_secret

  def compare
    other = Vault.new
    other.<ref>semi_secret</ref>
  end
end

Vault.new.semi_secret
"#,
    )
    .await;
}

#[tokio::test]
async fn references_private_visibility_override_excludes_included_method_explicit_receiver() {
    check(
        r#"
module SharedSecret
  def hidden$0
    "hidden"
  end
end

class Vault
  include SharedSecret
  private :hidden

  def reveal
    <ref>hidden</ref>
  end
end

Vault.new.hidden
"#,
    )
    .await;
}

#[tokio::test]
async fn references_protected_visibility_override_includes_same_family_included_method_receiver() {
    check(
        r#"
module SharedSecret
  def hidden$0
    "hidden"
  end
end

class Vault
  include SharedSecret
  protected :hidden

  def compare
    other = Vault.new
    other.<ref>hidden</ref>
  end
end

Vault.new.hidden
"#,
    )
    .await;
}

#[tokio::test]
async fn references_public_visibility_override_from_original_mixin_definition_includes_call_site() {
    check(
        r#"
module SharedSecret
  private

  def hidden$0
    "hidden"
  end
end

class Vault
  include SharedSecret
  public :hidden
end

Vault.new.<ref>hidden</ref>
"#,
    )
    .await;
}

/// Find references for instance method called on an instance via `.new`.
#[tokio::test]
async fn references_instance_method_on_new() {
    check(
        r#"
class Foo
  def bar$0
    42
  end
end

Foo.new.<ref>bar</ref>
"#,
    )
    .await;
}

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

#[tokio::test]
async fn references_included_hook_class_method() {
    check(
        r#"
module FeatureFlags
  def self.included(base)
    base.extend(ClassMethods)
  end

  module ClassMethods
    def enabled?$0
      true
    end
  end
end

class Worker
  include FeatureFlags
end

Worker.<ref>enabled?</ref>
"#,
    )
    .await;
}

#[tokio::test]
async fn references_concern_class_method() {
    check(
        r#"
module Searchable
  extend ActiveSupport::Concern

  class_methods do
    def find_by_term$0
      "ok"
    end
  end
end

class Product
  include Searchable
end

Product.<ref>find_by_term</ref>
"#,
    )
    .await;
}

#[tokio::test]
async fn references_included_hook_instance_method() {
    check(
        r#"
module DailyTrends
  def self.included(base)
    base.send :include, SharedMethods
  end

  module SharedMethods
    def get_html$0
      "html"
    end
  end
end

class Worker
  include DailyTrends

  def render
    <ref>get_html</ref>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn references_included_hook_class_eval_instance_method() {
    check(
        r#"
module AdminHelper
  def self.included(base)
    base.class_eval do
      include RequestHelpers
    end
  end

  module RequestHelpers
    def api_get$0
      "ok"
    end
  end
end

class SpecContext
  include AdminHelper

  def render
    <ref>api_get</ref>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn references_bare_module_function_method() {
    check(
        r#"
module Utils
  module_function

  def helper$0
    "helping"
  end
end

Utils.<ref>helper</ref>
"#,
    )
    .await;
}

#[tokio::test]
async fn references_extend_self_module_method() {
    check(
        r#"
module Utils
  extend self

  def helper$0
    "helping"
  end
end

Utils.<ref>helper</ref>
"#,
    )
    .await;
}

#[tokio::test]
async fn references_singleton_class_include_class_method() {
    check(
        r#"
module M_A
  def foo$0
    "ok"
  end
end

class ClassA
  class << self
    include M_A
  end
end

ClassA.<ref>foo</ref>
"#,
    )
    .await;
}

#[tokio::test]
async fn references_instance_method_ignores_same_named_top_level_dsl_call() {
    check(
        r#"
class AdminAPIClient
  def post$0
    "ok"
  end
end

post "/admin/widgets", to: "widgets#create"

AdminAPIClient.new.<ref>post</ref>
"#,
    )
    .await;
}

#[tokio::test]
async fn references_delegate_method_call() {
    check(
        r#"
class User
  def name
    "n"
  end
end

class Order
  delegate :name, to: :user

  def user
    User.new
  end
end

class Report
  def run
    Order.new.<ref>name</ref>
  end
end

class Probe
  def run
    Order.new.<ref>name$0</ref>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn references_delegate_method_definition() {
    check(
        r#"
class User
  def name
    "n"
  end
end

class Order
  delegate :name$0, to: :user

  def user
    User.new
  end
end

class Report
  def run
    Order.new.<ref>name</ref>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn references_forwardable_def_delegators_definition() {
    check(
        r#"
class ServiceFlags
  class << self
    extend Forwardable
    def_delegators :instance, :allow?$0
  end

  def self.instance
    new
  end

  def allow?
    true
  end
end

ServiceFlags.<ref>allow?</ref>
"#,
    )
    .await;
}

#[tokio::test]
async fn references_class_attribute_definition() {
    check(
        r#"
class Worker
  class_attribute :queue_config

  def self.setup
    <ref>queue_config$0</ref>
    self.queue_config = {}
  end

  def run
    <ref>queue_config</ref>
  end
end

Worker.<ref>queue_config</ref>
Worker.new.<ref>queue_config</ref>
"#,
    )
    .await;
}

/// Find references for instance method from multiple call sites.
#[tokio::test]
async fn references_instance_method_multiple_calls() {
    check(
        r#"
class Calculator
  def compute$0
    42
  end

  def run
    <ref>compute</ref>
  end

  def test
    <ref>compute</ref>
  end
end
"#,
    )
    .await;
}

// ============================================================================
// Class Methods
// ============================================================================

/// Find references for class method.
#[tokio::test]
async fn references_class_method() {
    check(
        r#"
class Utils
  def self.process$0
    "processing"
  end
end

Utils.<ref>process</ref>
"#,
    )
    .await;
}

/// Find references for singleton class method.
#[tokio::test]
async fn references_singleton_class_method() {
    check(
        r#"
class Foo
  class << self
    def singleton_method$0
      "singleton"
    end
  end
end

Foo.<ref>singleton_method</ref>
"#,
    )
    .await;
}

// ============================================================================
// Mixins
// ============================================================================

/// Find references for method from included module (called within including class).
#[tokio::test]
async fn references_included_module_method() {
    check(
        r#"
module Loggable
  def log$0
    puts "logging"
  end
end

class App
  include Loggable

  def run
    <ref>log</ref>
  end
end
"#,
    )
    .await;
}

/// Find references for method from module included in another module (transitive).
#[tokio::test]
async fn references_cross_module_method() {
    check(
        r#"
module ModuleA
  def method_a$0
    "from A"
  end
end

module ModuleB
  include ModuleA
end

class TestClass
  include ModuleB

  def test
    <ref>method_a</ref>
  end
end
"#,
    )
    .await;
}

// ============================================================================
// Inheritance
// ============================================================================

/// Find references for method from parent class (called in child).
#[tokio::test]
async fn references_inherited_method() {
    check(
        r#"
class Parent
  def parent_method$0
    "from parent"
  end
end

class Child < Parent
  def test
    <ref>parent_method</ref>
  end
end
"#,
    )
    .await;
}

/// Find references for mixin method through inheritance.
#[tokio::test]
async fn references_inherited_mixin_method() {
    check(
        r#"
module ApiHelpers
  def api_call$0
    "api"
  end
end

class BaseController
  include ApiHelpers
end

class AppController < BaseController
  def show
    <ref>api_call</ref>
  end
end
"#,
    )
    .await;
}

// ============================================================================
// Top-level
// ============================================================================

/// Find references for top-level method.
#[tokio::test]
async fn references_top_level_method() {
    check(
        r#"
def helper$0
end

<ref>helper</ref>
x = <ref>helper</ref>
"#,
    )
    .await;
}

// ============================================================================
// Constructor
// ============================================================================

// ============================================================================
// Chained Methods
// ============================================================================

/// Find references for method called via chained method (e.g., team.leader.name).
#[tokio::test]
async fn references_chained_method_call() {
    check(
        r#"
class User
  def name$0
    "hello"
  end
end

class Animal
  def name
    "animal"
  end
end

class Team
  def leader
    User.new
  end
end

team = Team.new
team.leader.<ref>name</ref>
"#,
    )
    .await;
}

/// Find references for intermediate method in a chain (e.g., team.leader in team.leader.name).
#[tokio::test]
async fn references_intermediate_chained_method() {
    check(
        r#"
class User
  def name
    "hello"
  end
end

class Team
  def leader$0
    User.new
  end
end

team = Team.new
team.<ref>leader</ref>.name
"#,
    )
    .await;
}

/// Find references for method on variable receiver — must NOT include calls on unrelated types.
#[tokio::test]
async fn references_method_on_variable_receiver() {
    check(
        r#"
class User
  def name$0
    "hello"
  end
end

class Animal
  def name
    "animal"
  end
end

user = User.new
user.<ref>name</ref>
animal = Animal.new
animal.name
"#,
    )
    .await;
}

#[tokio::test]
async fn references_method_on_local_receiver_inside_method() {
    check(
        r#"
class User
  def name$0
    "hello"
  end
end

class Presenter
  def render
    user = User.new
    user.<ref>name</ref>
  end
end
"#,
    )
    .await;
}

/// Local receiver references stay on the receiver type, even when an included module has the same method.
#[tokio::test]
async fn references_local_receiver_prefers_receiver_class_over_included_module_collision() {
    check(
        r#"
module Trackable
  def flow
    "module"
  end
end

class Invoice
  include Trackable

  def flow
    "class"
  end

  def charge
    receiver = Invoice.new
    receiver.<ref>flow$0</ref>
  end
end
"#,
    )
    .await;
}

/// Find references for method on instance-variable receiver.
#[tokio::test]
async fn references_method_on_instance_variable_receiver() {
    check(
        r#"
class User
  def name$0
    "hello"
  end
end

class Animal
  def name
    "animal"
  end
end

class Presenter
  def render
    @user = User.new
    @user.<ref>name</ref>
    @animal = Animal.new
    @animal.name
  end
end
"#,
    )
    .await;
}

/// Cross-file chain references update after the intermediate return method is indexed later.
#[tokio::test]
async fn references_chained_method_after_late_intermediate_file_open() {
    let mut editor = FakeEditor::new().await;
    let target = r#"
class Gateway
  def capture
    "ok"
  end
end
"#;
    let caller = r#"
class Invoice
  def charge
    item = Item.new
    item.gateway.capture
  end
end
"#;
    let intermediate = r#"
class Item
  # @return [Gateway]
  def gateway
    Gateway.new
  end
end
"#;

    editor.open("gateway.rb", target).await;
    editor.open("invoice.rb", caller).await;
    editor.close("invoice.rb").await;
    editor.open("item.rb", intermediate).await;
    editor.open("invoice.rb", caller).await;

    let refs = editor.references_at("gateway.rb", 2, 6).await;
    assert!(
        refs.iter().any(|location| {
            location.uri.path().ends_with("invoice.rb") && location.range.start.line == 4
        }),
        "expected Gateway#capture references to include invoice chain call, got {refs:?}"
    );
}

#[tokio::test]
async fn references_namespaced_chained_method_inside_method() {
    check(
        r#"
module Payments
  class Gateway
    def capture$0
      "ok"
    end
  end
end

module Billing
  class Account
    # @return [Payments::Gateway]
    def gateway
      __sim_return_gateway = Payments::Gateway.new
    end
  end

  class Invoice
    def charge
      account = Billing::Account.new
      account.gateway.<ref>capture</ref>
    end
  end
end
"#,
    )
    .await;
}

// ============================================================================
// Constructor
// ============================================================================

/// Find references for constructor (.new calls should reference initialize).
#[tokio::test]
async fn references_constructor_via_new() {
    check(
        r#"
class Foo
  def initialize$0
  end
end

Foo.<ref>new</ref>
"#,
    )
    .await;
}
