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

use crate::test::harness::{check, check_multi_file};

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
async fn method_missing_suppresses_unresolved_method_warn() {
    check(
        r#"
class DynamicRecord
  def method_missing(name, *args)
    "dynamic"
  end
end

record = DynamicRecord.new
<warn none code="unresolved-method">record.virtual_total</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn duplicate_inherited_method_defs_do_not_warn() {
    check_multi_file(&[
        (
            "child.rb",
            r#"
class Child < Base
  def save
    <warn none code="unresolved-method">run</warn>
  end
end
"#,
        ),
        (
            "base_a.rb",
            r#"
class Base
  def run
  end
end
"#,
        ),
        (
            "base_b.rb",
            r#"
class Base
  def run
  end
end
"#,
        ),
    ])
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

#[tokio::test]
async fn bare_kernel_method_in_class_does_not_warn_when_object_includes_kernel() {
    check_multi_file(&[
        (
            "scenario_extractor.rb",
            r#"
class ScenarioExtractor
  def to_output
    <warn none code="unresolved-method">puts "ok"</warn>
  end
end
"#,
        ),
        (
            "kernel.rb",
            r#"
module Kernel
  def puts(obj = nil, *args)
  end
end
"#,
        ),
        (
            "object.rb",
            r#"
class Object
  include Kernel
end
"#,
        ),
    ])
    .await;
}

#[tokio::test]
async fn bare_kernel_method_in_nested_class_uses_implicit_object_superclass() {
    check_multi_file(&[
        (
            "nested.rb",
            r#"
module Reports
  class ScenarioExtractor
    def to_output
      <warn none code="unresolved-method">puts "ok"</warn>
    end
  end
end
"#,
        ),
        (
            "kernel.rb",
            r#"
module Kernel
  def puts(obj = nil, *args)
  end
end
"#,
        ),
        (
            "object.rb",
            r#"
class Object
  include Kernel
end
"#,
        ),
    ])
    .await;
}

#[tokio::test]
async fn array_include_predicate_does_not_warn_when_receiver_type_is_known() {
    check(
        r#"
items = [1, 2, 3]
<warn none code="unresolved-method">items.include?(2)</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn visibility_modifier_does_not_warn_as_unresolved_method() {
    check(
        r#"
class BulkAccountActionForm
  <warn none code="unresolved-method">private</warn>

  def bulk_account_action_validation_helper
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn bare_raise_does_not_warn_as_unresolved_method() {
    check_multi_file(&[
        (
            "bulk_account_action_form.rb",
            r#"
class BulkAccountActionForm
  def bulk_account_action_validation_helper
    return <warn none code="unresolved-method">raise GosPosh::Platform::Errors::InvalidInputError.new("Action")</warn>
  end
end
"#,
        ),
        (
            "kernel.rb",
            r#"
module Kernel
  def raise(...)
  end
end
"#,
        ),
        (
            "object.rb",
            r#"
class Object
  include Kernel
end
"#,
        ),
    ])
    .await;
}
