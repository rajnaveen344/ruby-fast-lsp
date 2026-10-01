//! Reviewed Ruby method-dispatch contracts. Each expectation matches what Ruby
//! executes for the same program (`Method#owner` or the raised `NoMethodError`).

use crate::test::harness::check;

#[tokio::test]
async fn dispatch_instance_method_inherited_from_superclass() {
    check(
        r#"
class Base
  <def>def label
    "base"
  end</def>
end

class Child < Base
end

Child.new.label$0
"#,
    )
    .await;
}

#[tokio::test]
async fn dispatch_prepended_module_wins_over_class_method() {
    check(
        r#"
module Stamp
  <def>def label
    "stamp"
  end</def>
end

class Parcel
  prepend Stamp

  def label
    "parcel"
  end
end

Parcel.new.label$0
"#,
    )
    .await;
}

#[tokio::test]
async fn dispatch_included_module_wins_over_superclass() {
    check(
        r#"
module Stamp
  <def>def label
    "stamp"
  end</def>
end

class Base
  def label
    "base"
  end
end

class Child < Base
  include Stamp
end

Child.new.label$0
"#,
    )
    .await;
}

#[tokio::test]
async fn dispatch_last_included_module_wins() {
    check(
        r#"
module First
  def label
    "first"
  end
end

module Last
  <def>def label
    "last"
  end</def>
end

class Parcel
  include First
  include Last
end

Parcel.new.label$0
"#,
    )
    .await;
}

#[tokio::test]
async fn dispatch_class_method_inherited_from_superclass() {
    check(
        r#"
class Base
  <def>def self.label
    "base"
  end</def>
end

class Child < Base
end

Child.label$0
"#,
    )
    .await;
}

#[tokio::test]
async fn dispatch_rejects_explicit_receiver_call_to_private_method() {
    check(
        r#"
class Parcel
  private

  def label
    "private"
  end
end

Parcel.new.label$0<def none>
"#,
    )
    .await;
}

#[tokio::test]
async fn dispatch_module_call_follows_host_override_of_transitive_default() {
    check(
        r#"
module Defaults
  def label
    "default"
  end
end

module Feature
  include Defaults

  def render
    label$0
  end
end

class Host
  include Feature

  <def>def label
    "host"
  end</def>
end
"#,
    )
    .await;
}

#[tokio::test]
async fn dispatch_module_call_lists_each_host_override_but_not_unrelated_classes() {
    check(
        r#"
module Defaults
  def label
    "default"
  end
end

module Feature
  include Defaults

  def render
    label$0
  end
end

class First
  include Feature

  <def>def label
    "first"
  end</def>
end

class Second
  include Feature

  <def>def label
    "second"
  end</def>
end

class Unrelated
  def label
    "unrelated"
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn dispatch_module_call_follows_module_prepended_to_host() {
    check(
        r#"
module Defaults
  def label
    "default"
  end
end

module Feature
  include Defaults

  def render
    label$0
  end
end

module Wrapper
  <def>def label
    "wrapper"
  end</def>
end

class Host
  include Feature
  prepend Wrapper

  def label
    "host"
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn dispatch_module_call_uses_shared_default_when_hosts_do_not_override() {
    check(
        r#"
module Defaults
  <def>def label
    "default"
  end</def>
end

module Feature
  include Defaults

  def render
    label$0
  end
end

class First
  include Feature
end

class Second
  include Feature
end
"#,
    )
    .await;
}

#[tokio::test]
async fn dispatch_instance_method_reflection_keeps_module_owner() {
    check(
        r#"
module Defaults
  <def>def label
    "default"
  end</def>
end

module Feature
  include Defaults
end

class Host
  include Feature

  def label
    "host"
  end
end

Feature.instance_method(:lab$0el)
"#,
    )
    .await;
}
