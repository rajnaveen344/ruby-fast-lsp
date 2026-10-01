//! Hover on calls to methods provided by module functions, `extend self`, included hooks, and concerns.

use crate::test::harness::check;

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
