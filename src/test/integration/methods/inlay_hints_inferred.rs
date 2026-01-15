use crate::test::harness::*;

#[tokio::test]
async fn test_inferred_return_type_hint() {
    let _ = env_logger::builder().is_test(true).try_init();

    check(
        r#"
class A
  def foo<hint label=" -> Integer">
    1
  end

  def bar<hint label=" -> String">
    "hello"
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn test_unknown_return_type_hint_without_yard() {
    let _ = env_logger::builder().is_test(true).try_init();

    check(
        r#"
class A
  def process(input)<hint label=" -> ?">
    input.transform
  end

  def handle(obj)<hint label=" -> ?">
    obj.some_unknown_method
  end

  def chain(x, y)<hint label=" -> ?">
    x.foo.bar(y.baz)
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn test_mixed_known_and_unknown_types() {
    let _ = env_logger::builder().is_test(true).try_init();

    check(
        r#"
class Calculator
  # Known types are inferred from literals
  def get_number<hint label=" -> Integer">
    42
  end

  # @return [String]
  def greet<hint label="-> String">
    "hello"
  end

  # Unknown types show "?" as visual cue (method calls without known receiver)
  def compute(x)<hint label=" -> ?">
    x.calculate
  end

  # Expressions with operators also show "?" (method resolution incomplete)
  def add<hint label=" -> ?">
    1 + 2
  end
end
"#,
    )
    .await;
}
