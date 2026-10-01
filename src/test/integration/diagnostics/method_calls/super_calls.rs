//! `super` calls resolve against the definition that the enclosing method
//! overrides. A constructor's `super` continues the constructor lookup, so it
//! must find the inherited `initialize`, whether written in the project or
//! supplied by bundled core.

use crate::test::harness::check_project;

#[tokio::test]
async fn constructor_super_finds_a_project_parent_constructor() {
    check_project(&[(
        "shelf.rb",
        r#"
class Shelf
  def initialize(name)
    @name = name
  end
end

class TallShelf < Shelf
  def initialize(name)
    <warn none>super</warn>(name)
  end
end
"#,
    )])
    .await;
}

#[tokio::test]
async fn constructor_super_finds_a_core_parent_constructor() {
    check_project(&[(
        "missing.rb",
        r#"
class Missing < StandardError
  def initialize(name)
    <warn none>super</warn>("missing: #{name}")
  end
end

class Plain
  def initialize
    <warn none>super</warn>()
  end
end
"#,
    )])
    .await;
}

#[tokio::test]
async fn constructor_super_still_checks_the_inherited_arity() {
    check_project(&[(
        "shelf.rb",
        r#"
class Shelf
  def initialize(name)
    @name = name
  end
end

class TallShelf < Shelf
  def initialize
    <warn code="wrong-arity">super</warn>(1, 2)
  end
end
"#,
    )])
    .await;
}

#[tokio::test]
async fn instance_method_super_still_reports_a_missing_parent_method() {
    check_project(&[(
        "shelf.rb",
        r#"
class Shelf
end

class TallShelf < Shelf
  def stack
    <warn code="unresolved-method">super</warn>()
  end
end
"#,
    )])
    .await;
}
