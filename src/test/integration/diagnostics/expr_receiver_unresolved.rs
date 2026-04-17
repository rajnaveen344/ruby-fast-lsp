//! Tests for unresolved-method tracking on expression receivers + chains.
//!
//! Today: `user.unknown` (expression receiver) is silently skipped — type inference
//! resolves the receiver class but no diagnostic is emitted when the method is missing.
//!
//! Fix: emit `unresolved-method` ONLY when the receiver class is fully known and the
//! method does not exist on that class. Downstream chain links after a broken call
//! stay silent — once the first link is flagged, further "unknown receiver" warnings
//! would be redundant noise.
//!
//! Note: `User.new` itself currently warns ("Unresolved method `new` on `User`")
//! because `Class#new` isn't in the user index. Tests scope assertions tightly
//! around the calls under test to avoid colliding with that pre-existing noise.

use crate::test::harness::check;

#[tokio::test]
async fn expr_receiver_known_type_unknown_method_warns() {
    check(
        r#"
class User
  def name
    "x"
  end
end

u = User.new
u.<warn code="unresolved-method">foo</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn expr_receiver_known_type_known_method_no_warn() {
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
async fn chain_first_link_flagged_downstream_silent() {
    // u.foo unresolved on User → flag foo only. .bar's receiver type is
    // unknown after the broken link, so do NOT add additional noise.
    check(
        r#"
class User
  def name
    "x"
  end
end

u = User.new
u.<warn code="unresolved-method">foo</warn>.<warn none code="unresolved-method">bar</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn chain_returning_unknown_stays_silent_downstream() {
    // name's return type is Unknown → upcase has unknown receiver → no warn.
    check(
        r#"
class User
  def name
    "x"
  end
end

u = User.new
<warn none code="unresolved-method">u.name.upcase</warn>
"#,
    )
    .await;
}
