use crate::test::harness::*;

#[tokio::test]
#[ignore = "Requires CFG-based return type inference"]
async fn test_inferred_union_return_with_method_call() {
    let _ = env_logger::builder().is_test(true).try_init();

    check(
        r#"
class A
  def int
    200
  end

  def test<hint label=" -> (Integer | String)">
    if true
      return "heads"
    else
      return int
    end
  end
end
"#,
    )
    .await;
}

#[tokio::test]
#[ignore = "Requires CFG-based return type inference"]
async fn test_inferred_union_return_with_method_call_withtotut_class() {
    let _ = env_logger::builder().is_test(true).try_init();

    check(
        r#"
def test<hint label=" -> (Integer | String)">
  if true
    int
  else
    string
  end
end

def int
  200
end

def string
  "heads"
end
"#,
    )
    .await;
}
