//! Method chaining goto definition tests.
//!
//! Tests type-aware method lookup for chained method calls like `a.b.c`.

use crate::test::harness::check;

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
  def <def>process</def>
    "inner result"
  end
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

/// Goto definition for chained method call from constructor.
/// `Foo.new` returns `Foo`, so `.bar` should resolve to `Foo#bar`,
/// and `.baz` should resolve based on bar's return type.
#[tokio::test]
async fn goto_method_chain_from_constructor() {
    check(
        r#"
class Foo
  # @return [Bar]
  def <def>bar</def>
    Bar.new
  end
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
  def <def>final_method</def>
    42
  end
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
  def <def>consume</def>
    "done"
  end
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
  def <def>use</def>
    "using"
  end
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
  def <def>process</def>
    "other result"
  end
end

obj = Wrapper.new
result = obj.unwrap.process$0
"#,
    )
    .await;
}
