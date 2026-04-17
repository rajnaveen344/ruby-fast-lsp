//! Integration tests for namespace kind (instance vs singleton) method resolution.
//!
//! These tests verify that the LSP correctly distinguishes between:
//! - Instance methods (belong to the instance namespace)
//! - Singleton/class methods (belong to the singleton namespace)
//!
//! Ruby models this as: all methods are instance methods, but they belong to
//! different namespaces (the class itself vs its singleton class).

use crate::test::harness::check;

// ============================================================================
// Basic Instance vs Singleton Methods
// ============================================================================

/// Goto definition for instance method - should NOT find class method with same name.
/// The owner's namespace kind (Instance vs Singleton) distinguishes instance from class methods.
#[tokio::test]
async fn goto_instance_method_ignores_class_method() {
    check(
        r#"
class Calculator
  def self.compute
    "class method"
  end

  <def>def compute
    "instance method"
  end</def>
end

calc = Calculator.new
calc.compute$0
"#,
    )
    .await;
}

/// Goto definition for class method - should NOT find instance method with same name.
/// The owner's namespace kind (Instance vs Singleton) distinguishes instance from class methods.
#[tokio::test]
async fn goto_class_method_ignores_instance_method() {
    check(
        r#"
class Calculator
  <def>def self.compute
    "class method"
  end</def>

  def compute
    "instance method"
  end
end

Calculator.compute$0
"#,
    )
    .await;
}

// ============================================================================
// Singleton Class (`class << self`) Style
// ============================================================================

/// Goto definition for method defined in singleton class block.
#[tokio::test]
async fn goto_singleton_class_method() {
    check(
        r#"
class Config
  class << self
    <def>def load
      "loading config"
    end</def>
  end
end

Config.load$0
"#,
    )
    .await;
}

/// Goto definition for private method in singleton class block.
#[tokio::test]
async fn goto_private_singleton_method() {
    check(
        r#"
class Service
  class << self
    <def>def public_api
      internal_work
    end</def>

    private

    def internal_work
      "working"
    end
  end
end

Service.public_api$0
"#,
    )
    .await;
}

/// Singleton class methods should be accessible as class methods.
#[tokio::test]
async fn singleton_class_equivalent_to_self_dot() {
    check(
        r#"
class Validator
  class << self
    <def>def validate
      true
    end</def>
  end
end

# Both styles should work
Validator.validate$0
"#,
    )
    .await;
}

// ============================================================================
// Inheritance of Class Methods
// ============================================================================

/// Class methods should be inherited by subclasses.
#[tokio::test]
async fn inherited_class_method() {
    check(
        r#"
class BaseService
  <def>def self.configure
    "configuring"
  end</def>
end

class UserService < BaseService
end

UserService.configure$0
"#,
    )
    .await;
}

/// Subclass can override parent's class method.
/// Method override resolution returns the child's method (shadows parent).
#[tokio::test]
async fn overridden_class_method() {
    check(
        r#"
class Parent
  def self.greet
    "Hello from Parent"
  end
end

class Child < Parent
  <def>def self.greet
    "Hello from Child"
  end</def>
end

Child.greet$0
"#,
    )
    .await;
}

/// Deep inheritance chain for class methods.
#[tokio::test]
async fn deep_class_method_inheritance() {
    check(
        r#"
class GrandParent
  <def>def self.family_name
    "Smith"
  end</def>
end

class Parent < GrandParent
end

class Child < Parent
end

Child.family_name$0
"#,
    )
    .await;
}

// ============================================================================
// Module Extend for Class Methods
// ============================================================================

/// Extended module methods become class methods.
#[tokio::test]
async fn extended_module_becomes_class_method() {
    check(
        r#"
module ClassMethods
  <def>def find_all
    []
  end</def>
end

class User
  extend ClassMethods
end

User.find_all$0
"#,
    )
    .await;
}

/// Included module methods become instance methods (not class methods).
#[tokio::test]
async fn included_module_stays_instance_method() {
    check(
        r#"
module InstanceMethods
  <def>def save
    true
  end</def>
end

class Record
  include InstanceMethods
end

record = Record.new
record.save$0
"#,
    )
    .await;
}

// ============================================================================
// Method Calls in Class Body Context
// ============================================================================

/// Method call in class body resolves to class method.
#[tokio::test]
async fn class_body_method_call() {
    check(
        r#"
class Configuration
  <def>def self.register(name)
    @registered ||= []
    @registered << name
  end</def>

  register$0 :default
end
"#,
    )
    .await;
}

/// DSL-style class method calls in class body.
#[tokio::test]
async fn dsl_class_method_in_body() {
    check(
        r#"
class Model
  <def>def self.attribute(name)
    # define getter/setter
  end</def>

  attribute$0 :name
  attribute :email
end
"#,
    )
    .await;
}

// ============================================================================
// Mixed Instance and Class Methods with Same Name
// ============================================================================

/// Type inference distinguishes instance vs class method return types.
/// The owner's namespace kind ensures the correct method is used for type inference.
#[tokio::test]
async fn type_inference_respects_namespace() {
    check(
        r#"
class Factory
  # @return [Factory]
  def self.create
    new
  end

  # @return [String]
  <def>def create
    "created"
  end</def>
end

instance = Factory.new
result<hint label=": String"> = instance.create$0
"#,
    )
    .await;
}

/// Hover shows correct method based on receiver type.
#[tokio::test]
async fn hover_correct_method_for_receiver() {
    check(
        r#"
class Builder
  # Class method to create builder
  # @return [Builder]
  def self.start
    new
  end
end

Builder.start<hover label="Builder" substring="@return [Builder]">
"#,
    )
    .await;
}

// ============================================================================
// Self Receiver in Different Contexts
// ============================================================================

/// `self.method` in instance method calls instance method.
#[tokio::test]
async fn self_in_instance_method_calls_instance() {
    check(
        r#"
class Counter
  <def>def increment
    @count = (@count || 0) + 1
  end</def>

  def double_increment
    self.increment$0
    self.increment
  end
end
"#,
    )
    .await;
}

/// `self.method` in class method context calls class method.
#[tokio::test]
async fn self_in_class_method_calls_class() {
    check(
        r#"
class Logger
  <def>def self.log(msg)
    puts msg
  end</def>

  def self.debug(msg)
    self.log$0("[DEBUG] #{msg}")
  end
end
"#,
    )
    .await;
}

// ============================================================================
// Module Function (Both Instance and Class Method)
// ============================================================================

/// module_function creates both instance and class method.
/// When `module_function :helper` is called, it creates a singleton method on the module.
#[tokio::test]
async fn module_function_as_class_method() {
    check(
        r#"
module Utils
  <def>def helper
    "helping"
  end</def>
  module_function :helper
end

# Can call as module method
Utils.helper$0
"#,
    )
    .await;
}

// ============================================================================
// Nested Classes and Namespaces
// ============================================================================

/// Class method in nested class.
#[tokio::test]
async fn nested_class_method() {
    check(
        r#"
module Services
  class UserService
    <def>def self.find(id)
      # find user
    end</def>
  end
end

Services::UserService.find$0(1)
"#,
    )
    .await;
}

/// Instance method in deeply nested namespace.
#[tokio::test]
async fn deeply_nested_instance_method() {
    check(
        r#"
module Api
  module V1
    class UsersController
      <def>def index
        []
      end</def>
    end
  end
end

controller = Api::V1::UsersController.new
controller.index$0
"#,
    )
    .await;
}
