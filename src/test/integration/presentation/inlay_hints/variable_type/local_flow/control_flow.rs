//! Inlay hints for local variables assigned from Ruby control-flow expressions.

use crate::test::harness::check;

#[tokio::test]
async fn hash_pattern_capture_assignment_has_matched_value_type() {
    check(
        r#"
class User
end

case {user: User.new}
in {user: user}
  copy<hint label="User"> = user
end
"#,
    )
    .await;
}

#[tokio::test]
async fn hash_pattern_capture_feeds_method_return_type() {
    check(
        r#"
class User
end

def pick_user<hint label=" -> User">
  case {user: User.new}
  in {user: user}
    user
  end
end

pick_user
"#,
    )
    .await;
}

#[tokio::test]
async fn assignment_from_if_else_expression() {
    check(
        r#"
def m(cond)
  result<hint label="(Integer | String)"> = if cond then 1 else "ok" end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn assignment_from_if_without_else_includes_nil() {
    check(
        r#"
def m(cond)
  result<hint label="(Integer | NilClass)"> = if cond then 1 end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn assignment_from_unless_else_expression() {
    check(
        r#"
def m(cond)
  result<hint label="(Integer | String)"> = unless cond then 1 else "ok" end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn assignment_from_case_expression() {
    check(
        r#"
def m(value)
  result<hint label="(Integer | String)"> = case value
  when :one then 1
  else "ok"
  end
end
"#,
    )
    .await;
}
