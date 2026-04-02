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
#[ignore] // BUG: RBS `new` not returned for user-defined classes
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
#[ignore] // BUG: Direct literal receiver extraction fails (works via variable assignment)
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
#[ignore] // BUG: Direct literal receiver extraction fails (works via variable assignment)
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
#[ignore] // BUG: Method chaining through .new not resolved
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

// ─── 9. YARD Return Types ───

#[tokio::test]
#[ignore] // BUG: Chained method call return type not inferred for completion
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

// ─── 12. Self Receiver ───

#[tokio::test]
#[ignore] // BUG: self receiver not handled in completion context
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
