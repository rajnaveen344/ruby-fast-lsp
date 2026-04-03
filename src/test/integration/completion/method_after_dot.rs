//! Method completion after "." tests.
//!
//! Covers all receiver types: constants, variables, literals, instance variables,
//! self, inherited, mixins, chaining, YARD annotations, and edge cases.

use crate::test::harness::{check, check_multi_file};

// ─── 1. Constant Receivers (Singleton/Class Methods) ───

#[tokio::test]
async fn constant_receiver_class_method() {
    check(
        r#"
class User
  def self.find
    nil
  end
end

User.f$0
<complete items="find">
"#,
    )
    .await;
}

#[tokio::test]
async fn constant_receiver_new() {
    check(
        r#"
class User
end

User.n$0
<complete items="new">
"#,
    )
    .await;
}

#[tokio::test]
async fn constant_receiver_inherited_class_method() {
    check(
        r#"
class Base
  def self.find
    nil
  end
end

class User < Base
end

User.f$0
<complete items="find">
"#,
    )
    .await;
}

#[tokio::test]
async fn module_singleton_method() {
    check(
        r#"
module MathHelper
  def self.calculate
    42
  end
end

MathHelper.c$0
<complete items="calculate">
"#,
    )
    .await;
}

// ─── 2. Variable Receivers (Instance Methods) ───

#[tokio::test]
async fn variable_from_constructor() {
    check(
        r#"
class User
  def name
    "hello"
  end
end

user = User.new
user.n$0
<complete items="name">
"#,
    )
    .await;
}

#[tokio::test]
async fn variable_from_string_literal() {
    check(
        r#"
name = "hello"
name.u$0
<complete items="upcase">
"#,
    )
    .await;
}

#[tokio::test]
async fn variable_from_array_literal() {
    check(
        r#"
arr = [1, 2, 3]
arr.f$0
<complete items="first">
"#,
    )
    .await;
}

#[tokio::test]
async fn variable_from_hash_literal() {
    check(
        r#"
hash = { a: 1 }
hash.k$0
<complete items="keys">
"#,
    )
    .await;
}

#[tokio::test]
async fn variable_from_integer_literal() {
    check(
        r#"
num = 42
num.a$0
<complete items="abs">
"#,
    )
    .await;
}

#[tokio::test]
async fn variable_from_symbol_literal() {
    check(
        r#"
sym = :hello
sym.t$0
<complete items="to_s">
"#,
    )
    .await;
}

// ─── 3. Literal Receivers (Direct) ───

#[tokio::test]
async fn string_literal_receiver() {
    check(
        r#"
"hello".u$0
<complete items="upcase">
"#,
    )
    .await;
}

#[tokio::test]
async fn array_literal_receiver() {
    check(
        r#"
[1, 2, 3].f$0
<complete items="first">
"#,
    )
    .await;
}

// ─── 4. Inherited Methods ───

#[tokio::test]
async fn instance_inherits_parent_methods() {
    check(
        r#"
class Animal
  def breathe
    true
  end
end

class Dog < Animal
  def bark
    "woof"
  end
end

dog = Dog.new
dog.b$0
<complete items="bark,breathe">
"#,
    )
    .await;
}

// ─── 5. Mixin Methods ───

#[tokio::test]
async fn include_mixin_methods() {
    check(
        r#"
module Greetable
  def greet
    "hello"
  end
end

class User
  include Greetable

  def name
    "user"
  end
end

user = User.new
user.g$0
<complete items="greet">
"#,
    )
    .await;
}

// ─── 6. Partial Matching ───

#[tokio::test]
async fn partial_filters_methods() {
    check(
        r#"
class User
  def name
    "hello"
  end

  def nickname
    "hi"
  end

  def age
    25
  end
end

user = User.new
user.n$0
<complete items="name,nickname" excludes="age">
"#,
    )
    .await;
}

#[tokio::test]
async fn longer_partial_narrows_results() {
    check(
        r#"
class User
  def name
    "hello"
  end

  def nickname
    "hi"
  end
end

user = User.new
user.na$0
<complete items="name" excludes="nickname">
"#,
    )
    .await;
}

// ─── 7. No Filter (Empty Partial) ───

#[tokio::test]
async fn no_partial_returns_all_methods() {
    check(
        r#"
class User
  def name
    "hello"
  end

  def age
    25
  end
end

user = User.new
user.$0
<complete items="name,age">
"#,
    )
    .await;
}

// ─── 8. Method Chaining ───

#[tokio::test]
async fn chain_through_new() {
    check(
        r#"
class User
  def name
    "hello"
  end
end

User.new.n$0
<complete items="name">
"#,
    )
    .await;
}

// ─── 8b. Deep Method Chaining ───

#[tokio::test]
async fn chain_two_levels_rbs() {
    // "hello" → String, .upcase → String (RBS), .d → downcase
    check(
        r#"
name = "hello"
name.upcase.d$0
<complete items="downcase">
"#,
    )
    .await;
}

#[tokio::test]
async fn chain_three_levels_rbs() {
    check(
        r#"
name = "hello"
name.upcase.downcase.s$0
<complete items="strip">
"#,
    )
    .await;
}

#[tokio::test]
async fn chain_through_new_then_yard() {
    check(
        r#"
class User
  # @return [String]
  def name
    @name
  end
end

User.new.name.u$0
<complete items="upcase">
"#,
    )
    .await;
}

#[tokio::test]
#[ignore] // Array#first returns generic Elem, can't resolve to Integer without generics support
async fn chain_array_methods() {
    check(
        r#"
arr = [1, 2, 3]
arr.first.a$0
<complete items="abs">
"#,
    )
    .await;
}

// ─── 9. YARD Return Types ───

#[tokio::test]
async fn yard_return_type_on_method() {
    check(
        r#"
class User
  # @return [String]
  def name
    @name
  end
end

user = User.new
user.name.u$0
<complete items="upcase">
"#,
    )
    .await;
}

// ─── 10. attr_accessor Methods ───

#[tokio::test]
async fn attr_accessor_methods() {
    check(
        r#"
class User
  attr_accessor :name
  attr_reader :email
end

user = User.new
user.n$0
<complete items="name">
"#,
    )
    .await;
}

// ─── 11. Cross-File Completion ───

#[tokio::test]
async fn cross_file_method_completion() {
    check_multi_file(&[
        (
            "main.rb",
            r#"
user = User.new
user.n$0
<complete items="name">
"#,
        ),
        (
            "user.rb",
            r#"
class User
  def name
    "hello"
  end
end
"#,
        ),
    ])
    .await;
}

// ─── 12. Instance Variable Receiver ───

#[tokio::test]
async fn instance_variable_string_receiver() {
    check(
        r#"
class User
  def initialize
    @name = "hello"
  end

  def greet
    @name.u$0
  end
end
<complete items="upcase">
"#,
    )
    .await;
}

#[tokio::test]
async fn instance_variable_constructor_receiver() {
    check(
        r#"
class App
  def initialize
    @user = User.new
  end

  def run
    @user.n$0
  end
end

class User
  def name
    "hello"
  end
end
<complete items="name">
"#,
    )
    .await;
}

// ─── 13. Regression: exact screenshot scenario ───

#[tokio::test]
async fn constant_receiver_ne_partial_shows_new() {
    check(
        r#"
class UserA
  def name
  end
end

a = UserA.ne$0
<complete items="new" excludes="name">
"#,
    )
    .await;
}

// ─── 13b. Regression: constant receiver should not leak other classes' methods ───

#[tokio::test]
async fn constant_receiver_does_not_leak_other_class_methods() {
    check(
        r#"
class UserA
  def namea
  end
end

class Order
  def self.calculate_total
  end

  def amount
  end
end

class Product
  def self.find_by_name
  end

  def price
  end
end

a = UserA.$0
<complete excludes="calculate_total,amount,find_by_name,price">
"#,
    )
    .await;
}

// ─── 14. Self Receiver ───

#[tokio::test]
async fn self_receiver_instance_context() {
    check(
        r#"
class User
  def name
    "hello"
  end

  def greet
    self.n$0
  end
end
<complete items="name">
"#,
    )
    .await;
}

// ─── 15. Top-level method completions ───

#[tokio::test]
async fn top_level_method_completion() {
    check(
        r#"
def top_level
end

top$0
<complete items="top_level">
"#,
    )
    .await;
}

// TODO: Bare method call as receiver (e.g., `top_level.to_s`)
// needs inference pipeline to resolve method return types on-the-fly
