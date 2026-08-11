//! Tests for misspelled-method suggestions.
//!
//! When `unresolved-method` fires and a method on the receiver class+ancestors
//! is "close enough" by Levenshtein distance, the diagnostic message is
//! enriched with `Did you mean \`X\`?`.

use crate::test::harness::check;

#[tokio::test]
async fn typo_in_expr_receiver_method_suggests() {
    check(
        r#"
class User
  def name
    "x"
  end
end

u = User.new
u.<warn code="unresolved-method" message="Did you mean `name`?">naem</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn typo_in_constant_receiver_method_suggests() {
    check(
        r#"
class Class
end

class Foo
  def self.bar(x)
    x
  end
end

Foo.<warn code="unresolved-method" message="Did you mean `bar`?">barr</warn>(1)
"#,
    )
    .await;
}

#[tokio::test]
async fn exact_match_no_suggestion_needed() {
    // Method exists → no unresolved-method warning at all.
    check(
        r#"
class User
  def name
    "x"
  end
end

u = User.new
<warn none code="unresolved-method">u.name</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn unknown_reassignment_does_not_emit_a_receiver_method_diagnostic() {
    check(
        r#"
class Target
  def known
    true
  end
end

class Consumer
  def run
    @value = Target.new
    @value = dynamic_value
    <warn none code="unresolved-method">@value.known</warn>
  end
end
"#,
    )
    .await;
}
