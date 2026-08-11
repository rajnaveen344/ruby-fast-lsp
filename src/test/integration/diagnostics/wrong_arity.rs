//! Tests for `wrong-arity` diagnostic.
//!
//! V1 scope:
//! - Positional args only — splat (`*args`) at callsite or `*args` in def → skip check.
//! - Kwargs not validated yet (separate `unknown-kwarg` diagnostic later).
//! - Receivers covered: no-receiver (current namespace) and constant receivers (`Foo.bar`).
//! - Expression receivers (`u.foo(...)`) deferred to V2.
//!
//! Skip if method can't be strictly resolved on owner+ancestors (avoid double-warning
//! with `unresolved-method`).

use crate::test::harness::check;

#[tokio::test]
async fn too_few_positional_warns() {
    check(
        r#"
def greet(name)
  name
end

<warn code="wrong-arity">greet</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn too_many_positional_warns() {
    check(
        r#"
def greet(name)
  name
end

<warn code="wrong-arity">greet</warn>("a", "b")
"#,
    )
    .await;
}

#[tokio::test]
async fn exact_match_no_warn() {
    check(
        r#"
def greet(name)
  name
end

<warn none code="wrong-arity">greet("a")</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn optional_param_within_range_no_warn() {
    check(
        r#"
def greet(name, age = 0)
  name
end

<warn none code="wrong-arity">greet("a")</warn>
<warn none code="wrong-arity">greet("a", 1)</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn optional_param_too_many_warns() {
    check(
        r#"
def greet(name, age = 0)
  name
end

<warn code="wrong-arity">greet</warn>("a", 1, 2)
"#,
    )
    .await;
}

#[tokio::test]
async fn post_required_parameter_is_part_of_the_positional_arity() {
    check(
        r#"
def render(prefix = "value", body)
  [prefix, body]
end

<warn code="wrong-arity">render</warn>
<warn none code="wrong-arity">render("body")</warn>
<warn none code="wrong-arity">render("prefix", "body")</warn>
<warn code="wrong-arity">render</warn>("prefix", "body", "extra")
"#,
    )
    .await;
}

#[tokio::test]
async fn proven_core_collection_and_string_receivers_use_their_rbs_arity() {
    check(
        r#"
values = [1, 2]
<warn none code="wrong-arity">values.select { |value| value > 1 }</warn>
<warn none code="wrong-arity">values.find { |value| value > 1 }</warn>

name = ""
<warn none code="wrong-arity">name.concat("a")</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn namespaced_value_constants_dispatch_on_their_proven_value_type() {
    check(
        r#"
def find(transaction_id)
  transaction_id
end

module Catalog
  VALUES = [1, 2]
  LABELS = { "one" => 1 }
end

<warn none code="wrong-arity">Catalog::VALUES.find { |value| value == 2 }</warn>
<warn none code="wrong-arity">Catalog::LABELS.find { |name, _value| name == "one" }</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn rest_param_unbounded_no_warn() {
    check(
        r#"
def greet(*args)
  args
end

<warn none code="wrong-arity">greet</warn>
<warn none code="wrong-arity">greet("a", "b", "c", "d")</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn splat_at_callsite_skips_check() {
    // Splat at callsite means we don't know argument count — be silent.
    check(
        r#"
def greet(name)
  name
end

args = ["a", "b", "c"]
<warn none code="wrong-arity">greet(*args)</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn forwarding_args_at_callsite_skips_check() {
    check(
        r#"
def target(name, age)
  name
end

def wrapper(...)
  <warn none code="wrong-arity">target(...)</warn>
end
"#,
    )
    .await;
}

#[tokio::test]
async fn forwarding_parameter_accepts_any_call_shape() {
    check(
        r#"
def target(...)
end

<warn none code="wrong-arity">target</warn>
<warn none code="wrong-arity">target(1, 2, option: true)</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn incomplete_generated_parameter_shape_does_not_assume_zero_arity() {
    check(
        r#"
module Formatter
  def format(value)
    value
  end
  module_function :format
end

<warn none code="wrong-arity">Formatter.format("value")</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn alias_parameter_shape_is_not_assumed_to_be_zero() {
    check(
        r#"
class Formatter
  def render(value)
    value
  end
  alias_method :format, :render
end

formatter = Formatter.new
<warn none code="wrong-arity">formatter.format("value")</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn reflected_method_reference_is_not_a_zero_argument_call() {
    check(
        r#"
class Formatter
  def render(value)
    value
  end

  def renderer
    <warn none code="wrong-arity">method(:render)</warn>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn explicit_super_arguments_are_checked_as_the_actual_call_shape() {
    check(
        r#"
class Parent
  def render(value)
    value
  end
end

class Formatter < Parent
  def render(value)
    <warn none code="wrong-arity">super(value)</warn>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn forwarding_super_does_not_assume_zero_arguments() {
    check(
        r#"
class Parent
  def render(value)
    value
  end
end

class Formatter < Parent
  def render(value)
    <warn none code="wrong-arity">super</warn>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn forwarding_super_does_not_invent_a_nonempty_options_hash() {
    check(
        r#"
class Parent
  def reset
  end
end

class Child < Parent
  def reset
    <warn none code="wrong-arity">super</warn>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn rocket_hash_is_one_positional_argument() {
    check(
        r#"
def compare(expected)
  expected
end

<warn none code="wrong-arity">compare("answer" => 42)</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn trailing_options_hash_is_arity_ambiguous_for_keyword_declarations() {
    check(
        r#"
def configure(path, **options)
  [path, options]
end

options = {}
<warn none code="wrong-arity">configure("config.yml", options)</warn>
<warn code="wrong-arity">configure</warn>("config.yml", "extra", options)
"#,
    )
    .await;
}

#[tokio::test]
async fn ordinary_block_receiver_is_unknown_without_an_execution_contract() {
    check(
        r#"
def event(access_token, event_id)
end

def configure(&block)
  Object.new.instance_exec(&block)
end

configure do
  <warn none code="wrong-arity">event(:start)</warn>
end
"#,
    )
    .await;
}

#[tokio::test]
async fn anonymous_rest_parameter_accepts_any_positional_shape() {
    check(
        r#"
def target(*)
end

<warn none code="wrong-arity">target</warn>
<warn none code="wrong-arity">target(1, 2)</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn generated_attribute_writer_accepts_its_assigned_value() {
    check(
        r#"
class User
  attr_accessor :name

  def rename
    <warn none code="wrong-arity">self.name = "Ada"</warn>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn inherited_attribute_reader_precedes_same_named_mixin_method() {
    check(
        r#"
module ApiHelpers
  def user(id)
    id
  end
end

class BaseFeed
  include ApiHelpers
  attr_accessor :additional_feed_params, :feed_unit_config_params, :feed_unit_latency,
                :post_ids, :anchor_post, :access_context, :promoted_posts, :deduped_post_ids,
                :deduped_ads, :user, :same_domain_view, :enable_rerank, :use_freshness_ranking,
                :post_meta, :rerank_model_version, :rerank_entity_type, :use_size_boosting
end

class PersonalizedFeed < BaseFeed
  def payload
    <warn none code="wrong-arity">user</warn>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn generated_class_attribute_writer_accepts_its_assigned_value() {
    check(
        r#"
class Config
  class_attribute :mode

  def configure
    <warn none code="wrong-arity">self.mode = :strict</warn>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn unresolved_method_no_arity_warn() {
    // Method doesn't exist → unresolved-method handles it; no double-warn.
    check(
        r#"
<warn none code="wrong-arity">does_not_exist("a", "b")</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn unresolved_superclass_makes_missing_inherited_method_inconclusive() {
    check(
        r#"
class User < ActiveRecord::Base
  def save_record
    <warn none code="unresolved-method">save!</warn>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn unresolved_superclass_cannot_fall_through_to_unrelated_top_level_signature() {
    check(
        r#"
def validate(filename)
  filename
end

class SupplierForm < ExternalFormModel
  <warn none code="wrong-arity">validate :supplier, :provider</warn>
end
"#,
    )
    .await;
}

#[tokio::test]
async fn nested_class_cannot_prove_an_unrelated_top_level_method_was_loaded() {
    check(
        r#"
def validate(filename)
  filename
end

class SupplierForm
  <warn none code="wrong-arity">validate :supplier, :provider</warn>
end
"#,
    )
    .await;
}

#[tokio::test]
async fn captured_local_does_not_reuse_a_stale_flow_read_after_unknown_reassignment() {
    check(
        r#"
class Formatter
  def render(value)
    value
  end
end

def process(enabled, items)
  formatter = Formatter.new
  formatter.render("known") if enabled
  formatter = dynamic_formatter
  items.each do
    <warn none code="wrong-arity">formatter.render</warn>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn constant_receiver_too_few_warns() {
    check(
        r#"
class Foo
  def self.bar(x, y)
    x + y
  end
end

Foo.<warn code="wrong-arity">bar</warn>(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn expr_receiver_too_many_warns() {
    check(
        r#"
class User
  def name
    "x"
  end
end

u = User.new
u.<warn code="wrong-arity">name</warn>(1, 2)
"#,
    )
    .await;
}

#[tokio::test]
async fn expr_receiver_too_few_warns() {
    check(
        r#"
class User
  def greet(name, age)
    name
  end
end

u = User.new
u.<warn code="wrong-arity">greet</warn>("a")
"#,
    )
    .await;
}

#[tokio::test]
async fn expr_receiver_exact_no_warn() {
    check(
        r#"
class User
  def greet(name)
    name
  end
end

u = User.new
<warn none code="wrong-arity">u.greet("a")</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn expr_receiver_unknown_class_no_warn() {
    // String#upcase is RBS-backed, not in user index → skip arity check.
    check(
        r#"
s = "hello"
<warn none code="wrong-arity">s.upcase(1, 2)</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn constant_receiver_exact_no_warn() {
    check(
        r#"
class Foo
  def self.bar(x, y)
    x + y
  end
end

<warn none code="wrong-arity">Foo.bar(1, 2)</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn splat_with_fixed_args_under_max_silent() {
    check(
        r#"
def greet(name)
  name
end

args = ["a", "b"]
<warn none code="wrong-arity">greet(*args)</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn splat_with_fixed_args_at_max_silent() {
    check(
        r#"
def greet(name)
  name
end

args = []
<warn none code="wrong-arity">greet("a", *args)</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn splat_with_fixed_args_exceeding_max_warns() {
    check(
        r#"
def greet(name)
  name
end

args = []
<warn code="wrong-arity">greet</warn>("a", "b", *args)
"#,
    )
    .await;
}

#[tokio::test]
async fn splat_with_many_fixed_args_exceeding_max_warns() {
    check(
        r#"
def greet(name, age = 0)
  name
end

args = []
<warn code="wrong-arity">greet</warn>("a", 1, 2, *args)
"#,
    )
    .await;
}

#[tokio::test]
async fn splat_in_method_with_rest_silent() {
    check(
        r#"
def greet(*args)
  args
end

xs = []
<warn none code="wrong-arity">greet("a", "b", "c", *xs)</warn>
"#,
    )
    .await;
}
