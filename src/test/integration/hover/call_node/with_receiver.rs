//! Hover tests for method calls WITH explicit receiver.
//!
//! Examples: obj.method, Class.new, arr.length

use crate::indexer::file_processor::FileProcessor;
use crate::test::harness::{check, check_multi_file, FakeEditor};

// =============================================================================
// Simple Method Calls
// =============================================================================

/// Hover on method call shows return type from YARD
#[tokio::test]
async fn method_call_yard_return_type() {
    check(
        r#"
class Foo
  # @return [Integer]
  def count
    42
  end
end
x = Foo.new.count<hover label="Integer">
"#,
    )
    .await;
}

/// Variable assigned from method call shows return type
#[tokio::test]
async fn variable_from_method_call() {
    check(
        r#"
class Builder
  # @return [Product]
  def build
    Product.new
  end
end

class Product
end

product<hover label="Product"> = Builder.new.build
"#,
    )
    .await;
}

#[tokio::test]
async fn cross_file_hash_shape_return_reaches_keyed_reads_after_early_consumer_open() {
    check_multi_file(&[
        (
            "consumer.rb",
            r#"payload = PayloadFactory.build
payload[:name]<hover label="String">
"#,
        ),
        (
            "payload_factory.rb",
            r#"class PayloadFactory
  def self.build
    { id: 1, name: "Ada" }
  end
end
"#,
        ),
    ])
    .await;
}

#[tokio::test]
async fn rbs_record_return_reaches_cross_file_keyed_reads() {
    let mut editor = FakeEditor::new().await;
    let consumer = "payload = PayloadFactory.build\npayload[:name]\n";
    let signature_uri = tower_lsp::lsp_types::Url::parse("file:///sig/payload_factory.rbs")
        .expect("test signature URI must be valid");
    FileProcessor::default()
        .collect_rbs_facts(
            &signature_uri,
            r#"class PayloadFactory
  def self.build: () -> { id: Integer, ?name: String }
end
"#,
            editor.server(),
        )
        .expect("RBS record signature must enter the shared engine");
    editor.open("consumer.rb", consumer).await;
    editor
        .check(
            "consumer.rb",
            "payload = PayloadFactory.build\npayload[:name]<hover label=\"String\">\n",
        )
        .await;
}

#[tokio::test]
async fn rbs_record_parameter_contract_seeds_ruby_method_flow() {
    let mut editor = FakeEditor::new().await;
    let signature_uri = tower_lsp::lsp_types::Url::parse("file:///sig/payload_service.rbs")
        .expect("test signature URI must be valid");
    FileProcessor::default()
        .collect_rbs_facts(
            &signature_uri,
            r#"class PayloadService
  def normalize: ({ kind: :number, value: Integer } payload) -> Integer
end
"#,
            editor.server(),
        )
        .expect("RBS record parameter must enter the shared engine");
    editor
        .open_and_check_fixture(
            "payload_service.rb",
            r#"class PayloadService
  def normalize(payload)
    payload[:value]<hover label="Integer">.abs
  end
end
"#,
        )
        .await;
}

#[tokio::test]
async fn overloaded_rbs_record_parameters_seed_an_exhaustive_correlated_union() {
    let mut editor = FakeEditor::new().await;
    let signature_uri = tower_lsp::lsp_types::Url::parse("file:///sig/payload_service.rbs")
        .expect("test signature URI must be valid");
    FileProcessor::default()
        .collect_rbs_facts(
            &signature_uri,
            r#"class PayloadService
  def normalize: ({ kind: :number, value: Integer } payload) -> Integer
               | ({ kind: :text, value: String } payload) -> String
end
"#,
            editor.server(),
        )
        .expect("overloaded RBS records must enter the shared engine");
    editor
        .open_and_check_fixture(
            "payload_service.rb",
            r#"class PayloadService
  def normalize(payload)
    if payload[:kind] == :number
      payload[:value]<hover label="Integer">.abs
    else
      payload[:value]<hover label="String">.upcase
    end
  end
end
"#,
        )
        .await;
}

#[tokio::test]
async fn cross_file_value_constant_shape_reaches_keyed_reads_after_early_consumer_open() {
    check_multi_file(&[
        (
            "consumer.rb",
            r#"PAYLOAD[:name]<hover label="String">
"#,
        ),
        (
            "payload.rb",
            r#"PAYLOAD = { id: 1, name: "Ada" }
"#,
        ),
    ])
    .await;
}

#[tokio::test]
async fn cross_file_shape_return_edit_removes_and_reproves_consumer_evidence() {
    let mut editor = FakeEditor::new().await;
    let consumer = "payload = PayloadFactory.build\npayload[:name]\n";
    editor.open("consumer.rb", consumer).await;
    editor
        .open(
            "payload_factory.rb",
            "class PayloadFactory\n  def self.build\n    { name: \"Ada\" }\n  end\nend\n",
        )
        .await;
    editor
        .check(
            "consumer.rb",
            "payload = PayloadFactory.build\npayload[:name]<hover label=\"String\">\n",
        )
        .await;

    editor
        .set(
            "payload_factory.rb",
            "class PayloadFactory\n  def self.build\n    dynamic_payload\n  end\nend\n",
        )
        .await;
    editor
        .check(
            "consumer.rb",
            "payload = PayloadFactory.build\npayload[:name]<hover label=\"?\">\n",
        )
        .await;

    editor
        .set(
            "payload_factory.rb",
            "class PayloadFactory\n  def self.build\n    { name: 42 }\n  end\nend\n",
        )
        .await;
    editor
        .check(
            "consumer.rb",
            "payload = PayloadFactory.build\npayload[:name]<hover label=\"Integer\">\n",
        )
        .await;
}

#[tokio::test]
async fn cross_file_shape_return_is_independent_of_open_order() {
    let consumer = "payload = PayloadFactory.build\npayload[:name]\n";
    let provider = "class PayloadFactory\n  def self.build\n    { name: \"Ada\" }\n  end\nend\n";
    let expected = "payload = PayloadFactory.build\npayload[:name]<hover label=\"String\">\n";

    for provider_first in [false, true] {
        let mut editor = FakeEditor::new().await;
        if provider_first {
            editor.open("payload_factory.rb", provider).await;
            editor.open("consumer.rb", consumer).await;
        } else {
            editor.open("consumer.rb", consumer).await;
            editor.open("payload_factory.rb", provider).await;
        }
        editor.check("consumer.rb", expected).await;
    }
}

#[tokio::test]
async fn lambda_call_uses_block_return_type() {
    check(
        r#"
builder = -> { "ready" }
builder.call<hover label="String">
"#,
    )
    .await;
}

#[tokio::test]
async fn proc_call_uses_block_return_type() {
    check(
        r#"
builder = Proc.new { 1 }
builder.call<hover label="Integer">
"#,
    )
    .await;
}

#[tokio::test]
async fn method_missing_call_uses_fallback_return_type() {
    check(
        r#"
class DynamicRecord
  # @return [String]
  def method_missing(name, *args)
    "dynamic"
  end
end

DynamicRecord.new.virtual_total<hover label="String">
"#,
    )
    .await;
}

#[tokio::test]
async fn explicit_receiver_private_method_hover_is_unknown() {
    check(
        r#"
class Vault
  private

  # @return [String]
  def secret
    "token"
  end
end

Vault.new.secret<hover label="?">
"#,
    )
    .await;
}

#[tokio::test]
async fn explicit_receiver_private_argument_form_method_hover_is_unknown() {
    check(
        r#"
class Vault
  # @return [String]
  def secret
    "token"
  end
  private :secret
end

Vault.new.secret<hover label="?">
"#,
    )
    .await;
}

#[tokio::test]
async fn explicit_receiver_private_mixin_visibility_override_hover_is_unknown() {
    check(
        r#"
module SharedSecret
  # @return [String]
  def hidden
    "hidden"
  end
end

class Vault
  include SharedSecret
  private :hidden
end

Vault.new.hidden<hover label="?">
"#,
    )
    .await;
}

#[tokio::test]
async fn explicit_receiver_public_mixin_visibility_override_hover_uses_return_type() {
    check(
        r#"
module SharedSecret
  private

  # @return [String]
  def hidden
    "hidden"
  end
end

class Vault
  include SharedSecret
  public :hidden
end

Vault.new.hidden<hover label="String">
"#,
    )
    .await;
}

#[tokio::test]
async fn same_family_protected_mixin_visibility_override_hover_uses_return_type() {
    check(
        r#"
module SharedSecret
  # @return [String]
  def hidden
    "hidden"
  end
end

class Vault
  include SharedSecret
  protected :hidden

  def compare
    other = Vault.new
    other.hidden<hover label="String">
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn protected_same_family_explicit_receiver_hover_uses_return_type() {
    check(
        r#"
class Vault
  # @return [String]
  def semi_secret
    "token"
  end
  protected :semi_secret

  def compare
    other = Vault.new
    other.semi_secret<hover label="String">
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn alias_method_call_uses_original_return_type() {
    check(
        r#"
class User
  # @return [String]
  def name
    "n"
  end

  alias full_name name
end

User.new.full_name<hover label="String">
"#,
    )
    .await;
}

#[tokio::test]
async fn alias_method_definition_uses_original_return_type() {
    check(
        r#"
class User
  # @return [String]
  def name
    "n"
  end

  alias full_name<hover label="String"> name
end
"#,
    )
    .await;
}

#[tokio::test]
async fn alias_method_call_form_uses_original_return_type() {
    check(
        r#"
class User
  # @return [String]
  def name
    "n"
  end

  alias_method :display_name, :name
end

User.new.display_name<hover label="String">
"#,
    )
    .await;
}

#[tokio::test]
async fn alias_method_call_form_definition_uses_original_return_type() {
    check(
        r#"
class User
  # @return [String]
  def name
    "n"
  end

  alias_method :display_name<hover label="String">, :name
end
"#,
    )
    .await;
}

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
async fn call_expression_proof_outcome_is_replaced_after_edit() {
    let mut editor = FakeEditor::new().await;
    editor
        .open_and_check_fixture(
            "call_outcome_lifecycle.rb",
            r#"class User
  def profile
    dynamic_profile
  end
end

User.new.profile<hover label="Unknown[unresolved_method_return]">
"#,
        )
        .await;

    editor
        .set_and_check_fixture(
            "call_outcome_lifecycle.rb",
            r#"class User
  def profile
    "ready"
  end
end

User.new.profile<hover label="String">
"#,
        )
        .await;

    editor
        .set_and_check_fixture(
            "call_outcome_lifecycle.rb",
            r#"class User
  def profile
    dynamic_profile
  end
end

User.new.profile<hover label="Unknown[unresolved_method_return]">
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

#[tokio::test]
async fn bare_module_function_call_uses_target_return_type() {
    check(
        r#"
module Utils
  module_function

  # @return [String]
  def helper
    "helping"
  end
end

Utils.helper<hover label="String">
"#,
    )
    .await;
}

#[tokio::test]
async fn extend_self_call_uses_target_return_type() {
    check(
        r#"
module Utils
  extend self

  # @return [String]
  def helper
    "helping"
  end
end

Utils.helper<hover label="String">
"#,
    )
    .await;
}

#[tokio::test]
async fn delegate_method_call_uses_target_return_type() {
    check(
        r#"
class User
  # @return [String]
  def name
    "n"
  end
end

class Order
  delegate :name, to: :user

  # @return [User]
  def user
    User.new
  end
end

Order.new.name<hover label="String">
"#,
    )
    .await;
}

#[tokio::test]
async fn forwardable_def_delegators_call_uses_target_return_type() {
    check(
        r#"
class ServiceFlags
  class << self
    extend Forwardable
    def_delegators :instance, :allow?
  end

  # @return [ServiceFlags]
  def self.instance
    new
  end

  # @return [String]
  def allow?
    "enabled"
  end
end

ServiceFlags.allow?<hover label="String">
"#,
    )
    .await;
}

#[tokio::test]
async fn cross_file_delegate_method_call_uses_target_return_type() {
    check_multi_file(&[
        (
            "user.rb",
            r#"
class User
  # @return [String]
  def name
    "n"
  end
end
"#,
        ),
        (
            "order.rb",
            r#"
class Order
  delegate :name, to: :user

  # @return [User]
  def user
    User.new
  end
end
"#,
        ),
        (
            "report.rb",
            r#"
class Report
  def run
    Order.new.name<hover label="String">
  end
end
"#,
        ),
    ])
    .await;
}

#[tokio::test]
async fn namespaced_delegate_method_call_survives_late_target_file_open() {
    check_multi_file(&[
        (
            "caller.rb",
            r#"
module SimDelegate
  class Report
    def run
      SimDelegate::Order.new.name<hover label="String">
    end
  end
end
"#,
        ),
        (
            "order.rb",
            r#"
module SimDelegate
  class Order
    # @return [SimDelegate::User]
    def user
      SimDelegate::User.new
    end

    delegate :name, to: :user
  end
end
"#,
        ),
        (
            "user.rb",
            r#"
module SimDelegate
  class User
    # @return [String]
    def name
      "n"
    end
  end
end
"#,
        ),
    ])
    .await;
}

// =============================================================================
// Chained Method Calls
// =============================================================================

/// Hover on chained calls shows type at each step
#[tokio::test]
async fn chained_method_calls() {
    check(
        r#"
class User
  # @return [Profile]
  def profile
    Profile.new
  end
end

class Profile
  # @return [String]
  def name
    "John"
  end
end

user = User.new
user.profile<hover label="Profile">.name<hover label="String">
"#,
    )
    .await;
}

/// Unknown propagates when chain breaks
#[tokio::test]
async fn chain_unknown_propagation() {
    check(
        r#"
class Foo
  def unknown_method
    bar  # bar is undefined, returns unknown
  end
end

x = Foo.new.unknown_method<hover label="?">
"#,
    )
    .await;
}

// =============================================================================
// RBS Built-in Types
// =============================================================================

/// Array methods use RBS types
#[tokio::test]
async fn array_methods() {
    check(
        r#"
arr = [1, 2, 3]
arr.length<hover label="Integer">
"#,
    )
    .await;
}

/// Hash methods use RBS types
#[tokio::test]
async fn hash_methods() {
    check(
        r#"
hash = { a: 1 }
hash.keys<hover label="Array<Symbol>">
"#,
    )
    .await;
}

#[tokio::test]
async fn structural_hash_reads_use_exact_and_dynamic_key_evidence() {
    check(
        r#"
def inspect_payload(dynamic_key)
  payload = { count: 1, user: { name: "Ada" } }
  payload[:count]<hover label="Integer">
  payload[:missing]<hover label="NilClass">
  payload[dynamic_key]<hover label="(Integer | NilClass | { name: String })">
  payload.fetch<hover label="Integer">(:count)
  payload.fetch<hover label="String">(:missing, "fallback")
  payload.dig<hover label="String">(:user, :name)
end
"#,
    )
    .await;
}

#[tokio::test]
async fn structural_hash_iteration_projects_keys_values_and_block_parameters() {
    check(
        r#"
def inspect_payload
  payload = { age: 42, name: "Ada" }
  payload.keys<hover label="Array<Symbol>">
  payload.values<hover label="Array<Integer | String>">
  iterator = payload.each
  iterator<hover label="Enumerator">
  payload.each<hover label="{ age: Integer, name: String }"> do |key, value|
    key<hover label="Symbol">
    value<hover label="(Integer | String)">
  end
end
"#,
    )
    .await;
}

// =============================================================================
// Deep Chained Calls - hover on each intermediate method
// =============================================================================

/// Hover on each method in a deep chain shows correct return type
#[tokio::test]
async fn deep_chain_intermediate_methods() {
    check(
        r#"
class First
  # @return [Second]
  def to_second
    Second.new
  end
end

class Second
  # @return [Third]
  def to_third
    Third.new
  end
end

class Third
  # @return [Integer]
  def value
    42
  end
end

a = First.new
a.to_second<hover label="Second">.to_third<hover label="Third">.value<hover label="Integer">
"#,
    )
    .await;
}

/// Hover on method where receiver is method call (not variable)
#[tokio::test]
async fn method_call_as_receiver() {
    check(
        r#"
class First
  # @return [Second]
  def to_second
    Second.new
  end
end

class Second
  # @return [String]
  def name
    "hello"
  end
end

First.new.to_second<hover label="Second">.name<hover label="String">
"#,
    )
    .await;
}

/// Hover on method call through instance-variable receiver uses the ivar type.
#[tokio::test]
async fn instance_variable_receiver_method_call() {
    check(
        r#"
class Gateway
  # @return [String]
  def refund
    "ok"
  end
end

class Invoice
  def charge
    @gateway = Gateway.new
    @gateway.refund<hover label="String">
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn singleton_class_include_class_method_call() {
    check(
        r#"
module M_A
  # @return [String]
  def foo
    "ok"
  end
end

class ClassA
  class << self
    include M_A
  end
end

ClassA.foo<hover label="String">
"#,
    )
    .await;
}

#[tokio::test]
async fn included_hook_class_method_call_uses_target_return_type() {
    check(
        r#"
module FeatureFlags
  def self.included(base)
    base.extend(ClassMethods)
  end

  module ClassMethods
    # @return [String]
    def status
      "on"
    end
  end
end

class Worker
  include FeatureFlags
end

Worker.status<hover label="String">
"#,
    )
    .await;
}

#[tokio::test]
async fn concern_class_method_call_uses_target_return_type() {
    check(
        r#"
module Searchable
  extend ActiveSupport::Concern

  class_methods do
    # @return [String]
    def find_by_term
      "ok"
    end
  end
end

class Product
  include Searchable
end

Product.find_by_term<hover label="String">
"#,
    )
    .await;
}
