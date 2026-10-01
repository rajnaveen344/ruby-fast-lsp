//! References through included hooks, concerns, module functions, `extend self`, and inheritance.

use crate::test::harness::check;

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
