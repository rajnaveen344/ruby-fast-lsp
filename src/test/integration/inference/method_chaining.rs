//! Method chaining goto definition tests.
//!
//! Tests type-aware method lookup for chained method calls like `a.b.c`.

use crate::test::harness::{check, FakeEditor};

// ============================================================================
// Type-Aware Method Chaining
// ============================================================================

/// Goto definition for chained method call with typed variable receiver.
/// When the receiver is assigned from a constructor (e.g., `obj = Wrapper.new`),
/// the type is inferred and used for method resolution.
///
/// **This test proves type-aware lookup works with variables**: Both `Inner` and `Other`
/// have a `process` method, but only `Inner#process` should be found because
/// `Wrapper#unwrap` returns `Inner` (via YARD @return).
#[tokio::test]
async fn goto_method_chain_variable_receiver() {
    check(
        r#"
class Wrapper
  # @return [Inner]
  def unwrap
    Inner.new
  end
end

class Inner
  <def>def process
    "inner result"
  end</def>
end

class Other
  def process
    "other result"
  end
end

obj = Wrapper.new
result = obj.unwrap.process$0
"#,
    )
    .await;
}

/// An instance variable belongs to its lexical execution owner. Reopening a
/// class after another class assigned the same variable name must not let the
/// intervening assignment redirect method navigation.
#[tokio::test]
async fn goto_instance_variable_receiver_uses_source_ordered_owner_fact() {
    check(
        r#"
class IntegerValue
  <def>def pick
    1
  end</def>
end

class StringValue
  def pick
    "wrong owner"
  end
end

class Consumer
  def initialize
    @value = IntegerValue.new
  end
end

class Other
  def initialize
    @value = StringValue.new
  end
end

class Consumer
  def run
    @value.pick$0
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn goto_instance_variable_receiver_fails_closed_after_unknown_reassignment() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "main.rb",
            r#"class Target
  def upcase
    "target"
  end
end

class Types
  def convert
    @value = Target.new
    @value = dynamic_value
    @value.upcase
  end
end
"#,
        )
        .await;

    let definitions = editor.goto_def_at("main.rb", 10, 13).await;
    assert!(
        definitions.is_empty(),
        "an Unknown reassignment must invalidate the earlier receiver proof, got {definitions:?}"
    );
}

#[tokio::test]
async fn goto_class_variable_receiver_fails_closed_after_unknown_reassignment() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "main.rb",
            r#"class Target
  def known
    true
  end
end

class Consumer
  @@value = Target.new
  @@value = dynamic_value

  def run
    @@value.known
  end
end
"#,
        )
        .await;

    let definitions = editor.goto_def_at("main.rb", 11, 14).await;
    assert!(
        definitions.is_empty(),
        "an Unknown class-variable write must invalidate the earlier receiver proof, got {definitions:?}"
    );
}

#[tokio::test]
async fn goto_global_variable_receiver_fails_closed_after_unknown_reassignment() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "main.rb",
            r#"class Target
  def known
    true
  end
end

$global = Target.new
$global = dynamic_value
$global.known
"#,
        )
        .await;

    let definitions = editor.goto_def_at("main.rb", 8, 10).await;
    assert!(
        definitions.is_empty(),
        "an Unknown global-variable write must invalidate the earlier receiver proof, got {definitions:?}"
    );
}

/// Goto definition for chained method call from constructor.
/// `Foo.new` returns `Foo`, so `.bar` should resolve to `Foo#bar`,
/// and `.baz` should resolve based on bar's return type.
#[tokio::test]
async fn goto_method_chain_from_constructor() {
    check(
        r#"
class Foo
  # @return [Bar]
  <def>def bar
    Bar.new
  end</def>
end

class Bar
  def baz
    "result"
  end
end

obj = Foo.new
result = obj.bar$0
"#,
    )
    .await;
}

/// Goto definition for deeply nested method chain.
/// Tests three levels: `a.foo.bar.baz`
#[tokio::test]
async fn goto_method_chain_deep_nesting() {
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
  <def>def final_method
    42
  end</def>
end

a = First.new
result = a.to_second.to_third.final_method$0
"#,
    )
    .await;
}

/// Goto definition for method chain with variable assignment.
/// Tests that intermediate variable type is tracked.
#[tokio::test]
async fn goto_method_chain_with_intermediate_variable() {
    check(
        r#"
class Producer
  # @return [Consumer]
  def produce
    Consumer.new
  end
end

class Consumer
  <def>def consume
    "done"
  end</def>
end

producer = Producer.new
consumer = producer.produce
result = consumer.consume$0
"#,
    )
    .await;
}

/// Goto definition for method on instance assigned from chained call.
/// Tests that we correctly infer type from `Builder.new.build` and filter out
/// incompatible methods (both top-level and other classes).
#[tokio::test]
async fn goto_method_chain_assigned_result() {
    check(
        r#"
class Builder
  # @return [Product]
  def build
    Product.new
  end
end

# This top-level method should NOT be found
def use
  "top-level use"
end

class Product
  <def>def use
    "using"
  end</def>
end

product = Builder.new.build
product.use$0
"#,
    )
    .await;
}

/// Negative test: This SHOULD FAIL if type filtering is working.
/// We mark `Other#process` as the expected definition, but the receiver type
/// should be `Inner`, so `Inner#process` should be found instead.
#[tokio::test]
#[should_panic(expected = "Expected definition at")]
async fn goto_method_chain_rejects_wrong_type() {
    check(
        r#"
class Wrapper
  # @return [Inner]
  def unwrap
    Inner.new
  end
end

class Inner
  def process
    "inner result"
  end
end

class Other
  <def>def process
    "other result"
  end</def>
end

obj = Wrapper.new
result = obj.unwrap.process$0
"#,
    )
    .await;
}

/// Goto definition for INTERMEDIATE method in chain (not the last one).
/// In `a.b.c`, when cursor is on `b`, we should:
/// 1. Resolve `a`'s type
/// 2. Find `b` on that type
#[tokio::test]
async fn goto_intermediate_method_in_chain() {
    check(
        r#"
class First
  # @return [Second]
  <def>def to_second
    Second.new
  end</def>
end

class Second
  # @return [Third]
  def to_third
    Third.new
  end
end

class Third
  def final_method
    42
  end
end

a = First.new
a.to_second$0.to_third.final_method
"#,
    )
    .await;
}

/// Goto definition where the receiver is a method call (not a variable).
/// In `First.new.to_second`, cursor on `to_second`:
/// 1. `First.new` returns `First` (instance)
/// 2. Find `to_second` on `First`
#[tokio::test]
async fn goto_method_with_method_call_receiver() {
    check(
        r#"
class First
  # @return [Second]
  <def>def to_second
    Second.new
  end</def>
end

class Second
end

First.new.to_second$0
"#,
    )
    .await;
}

/// A chained call on a union receiver is concrete only when every reachable
/// receiver member proves the called method's return type. String#length is
/// known, but Integer#length is not a valid call, so the chain must stay
/// Unknown instead of silently discarding the Integer branch.
#[tokio::test]
async fn union_chain_does_not_publish_a_partial_return_type() {
    check(
        r#"
class Choice
  def value(flag)
    if flag
      "text"
    else
      1
    end
  end
end

result<hint label=": ?"> = Choice.new.value(true).length
"#,
    )
    .await;
}
