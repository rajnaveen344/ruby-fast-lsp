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
  
  def baz(a)<hint label=" -> Integer">
    a = 1
    a
  end
end
"#,
    )
    .await;
}
