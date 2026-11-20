//! Methods that return `self`.
//!
//! Such a call returns its receiver, so the type follows the call site: a
//! subclass instance stays a subclass instance. Where the receiver is not
//! one proven class, or only some paths return `self`, nothing is proven.

use crate::test::harness::check;

/// A call on an instance returns that instance's class.
#[tokio::test]
async fn call_on_instance_returns_its_class() {
    check(
        r#"
class Counter
  def reset
    @count = 0
    self
  end
end

counter = Counter.new.reset
count<type label="Counter">er
"#,
    )
    .await;
}

/// An inherited method returns the subclass it was called on, so a method
/// only the subclass defines is found on the result.
#[tokio::test]
async fn inherited_call_keeps_the_subclass() {
    check(
        r#"
class Counter
  def reset
    self
  end
end

class Tally < Counter
  def extra
    1
  end
end

tally = Tally.new.reset
tal<type label="Tally">ly
extra = Tally.new.reset.extra
ext<type label="Integer">ra
"#,
    )
    .await;
}

/// A method only the subclass defines is still missing on the base class.
#[tokio::test]
async fn base_receiver_does_not_gain_subclass_methods() {
    check(
        r#"
class Counter
  def reset
    self
  end
end

class Tally < Counter
  def extra
    1
  end
end

Counter.new.reset.<warn code="unresolved-method">extra</warn>
counter = Counter.new.reset
counter.<warn code="unresolved-method">extra</warn>
"#,
    )
    .await;
}

/// A module method returns the class that includes the module.
#[tokio::test]
async fn module_method_returns_the_including_class() {
    check(
        r#"
module Chainable
  def touch
    self
  end
end

class Record
  include Chainable
end

record = Record.new.touch
reco<type label="Record">rd
"#,
    )
    .await;
}

/// Every path returning `self` still returns the receiver.
#[tokio::test]
async fn early_return_of_self_returns_the_receiver() {
    check(
        r#"
class Counter
  def bump(amount)
    return self if amount.zero?
    @count = amount
    self
  end
end

counter = Counter.new.bump(1)
count<type label="Counter">er
"#,
    )
    .await;
}

/// A path that returns something else leaves the call unproven.
#[tokio::test]
async fn mixed_returns_stay_unknown() {
    check(
        r#"
class Counter
  def bump(amount)
    return nil if amount.zero?
    self
  end

  def guarded
    self
  rescue StandardError
    nil
  end
end

bumped = Counter.new.bump(1)
bump<type label="?">ed
guarded = Counter.new.guarded
guard<type label="?">ed
"#,
    )
    .await;
}

/// `return self` inside a block may run with another object as `self`.
#[tokio::test]
async fn return_inside_a_block_stays_unknown() {
    check(
        r#"
class Counter
  def each_step(steps)
    steps.each { |step| return self if step }
    self
  end
end

counter = Counter.new.each_step([1])
count<type label="?">er
"#,
    )
    .await;
}

/// A constructor ending in `self` still builds an instance.
#[tokio::test]
async fn constructor_ending_in_self_builds_an_instance() {
    check(
        r#"
class Counter
  def initialize
    @count = 0
    self
  end
end

counter = Counter.new
count<type label="Counter">er
"#,
    )
    .await;
}

/// The result is usable as a receiver inside another method.
#[tokio::test]
async fn result_is_typed_inside_a_method() {
    check(
        r#"
class Counter
  def reset
    self
  end
end

class Report
  def build
    counter = Counter.new.reset
    count<type label="Counter">er
  end
end
"#,
    )
    .await;
}

/// A class method returning `self` returns the class it was called on.
#[tokio::test]
async fn class_method_returns_the_class() {
    check(
        r#"
class Settings
  def self.configure
    self
  end

  def self.reload
    1
  end
end

class LocalSettings < Settings
end

settings = LocalSettings.configure
sett<type label="Class<LocalSettings>">ings
reloaded = Settings.configure.reload
reload<type label="Integer">ed
"#,
    )
    .await;
}

/// `super` runs with the caller as receiver, which the parent lookup does
/// not name, so nothing is proven.
#[tokio::test]
async fn super_call_stays_unknown() {
    check(
        r#"
class Counter
  def reset
    self
  end
end

class Tally < Counter
  def reset
    result = super
    resu<type label="?">lt
    <warn none code="unresolved-method">super.extra</warn>
    1
  end
end
"#,
    )
    .await;
}

/// A private method cannot be called on an explicit receiver, so that call
/// proves nothing.
#[tokio::test]
async fn hidden_method_stays_unknown() {
    check(
        r#"
class Counter
  private

  def reset
    self
  end
end

counter = Counter.new.reset
count<type label="?">er
<warn none code="unresolved-method">Counter.new.reset.nothing</warn>
"#,
    )
    .await;
}

/// A protected method returns its receiver only where it may be called.
#[tokio::test]
async fn protected_method_follows_the_caller() {
    check(
        r#"
class Counter
  def merge(other)
    other.reset
  end

  def inside
    Counter.new.reset.<warn code="unresolved-method">nothing</warn>
  end

  protected

  def reset
    self
  end
end

<warn none code="unresolved-method">Counter.new.reset.nothing</warn>
"#,
    )
    .await;
}

/// Inside the module the receiver is whichever class includes it.
#[tokio::test]
async fn call_inside_the_module_stays_unknown() {
    check(
        r#"
module Chainable
  def touch
    self
  end

  def chain
    touched = touch
    touch<type label="?">ed
    <warn none code="unresolved-method">touch.anything</warn>
  end
end

class Record
  include Chainable
end
"#,
    )
    .await;
}
