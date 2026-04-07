//! Tests for callHierarchy (incoming and outgoing calls)
//!
//! Tests the "who calls this method?" and "what does this method call?" features.
//!
//! Tag-based tests use `<incoming>...</incoming>` and `<outgoing>...</outgoing>` to mark
//! expected caller/callee method definitions. Programmatic tests use FakeEditor for
//! cross-file and complex scenarios.

use crate::test::harness::{check, FakeEditor};

// ============================================================================
// Tag-based tests — incoming calls
// ============================================================================

/// Tag-based: method called from another method in same class.
#[tokio::test]
async fn tag_incoming_same_class() {
    check(
        r#"
class Foo
  def greet$0
    "hello"
  end

  <incoming>def run
    greet
  end</incoming>
end
"#,
    )
    .await;
}

/// Tag-based: method called from multiple callers.
#[tokio::test]
async fn tag_incoming_multiple_callers() {
    check(
        r#"
class Foo
  def greet$0; end

  <incoming>def run
    greet
  end</incoming>

  <incoming>def execute
    greet
  end</incoming>
end
"#,
    )
    .await;
}

/// Tag-based: method with no callers — no tags means no assertions needed.
/// Using FakeEditor for this case since check() requires at least one tag.

/// Tag-based: outgoing calls from a method.
#[tokio::test]
async fn tag_outgoing_basic() {
    check(
        r#"
class Pipeline
  <outgoing>def step1; end</outgoing>
  <outgoing>def step2; end</outgoing>

  def run$0
    step1
    step2
  end
end
"#,
    )
    .await;
}

/// Tag-based: bidirectional — step2 calls step1, step3 calls step2.
#[tokio::test]
async fn tag_incoming_and_outgoing() {
    check(
        r#"
class Pipeline
  def step1; end

  def step2$0
    step1
  end

  <incoming>def step3
    step2
  end</incoming>
end
"#,
    )
    .await;
}

// ============================================================================
// Tag-based tests — cursor on call site
// ============================================================================

/// Cursor on a method call should find incoming callers of the called method.
#[tokio::test]
async fn tag_incoming_from_call_site() {
    check(
        r#"
class Foo
  def greet; end

  <incoming>def run
    greet
  end</incoming>

  <incoming>def other
    greet$0
  end</incoming>
end
"#,
    )
    .await;
}

/// Cursor on a method call should find outgoing calls of the called method.
#[tokio::test]
async fn tag_outgoing_from_call_site() {
    check(
        r#"
class Foo
  <outgoing>def step1; end</outgoing>

  def step2
    step1
  end

  def run
    step2$0
  end
end
"#,
    )
    .await;
}

// ============================================================================
// Programmatic tests — incoming calls
// ============================================================================

/// Method called from another method in the same class.
#[tokio::test]
async fn incoming_calls_same_class() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "main.rb",
            r#"
class Foo
  def greet
    "hello"
  end

  def run
    greet
  end
end
"#,
        )
        .await;

    // Cursor on `greet` definition (line 2, char 6)
    let items = editor.prepare_call_hierarchy_at("main.rb", 2, 6).await;
    assert_eq!(items.len(), 1, "Expected 1 item, got {:?}", items);
    assert_eq!(items[0].name, "greet");

    let incoming = editor.incoming_calls_for(items[0].clone()).await;
    assert_eq!(
        incoming.len(),
        1,
        "Expected 1 incoming call, got {:?}",
        incoming
    );
    assert_eq!(incoming[0].from.name, "run");
}

/// Method called from multiple methods.
#[tokio::test]
async fn incoming_calls_multiple_callers() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "main.rb",
            r#"
class Foo
  def greet
    "hello"
  end

  def run
    greet
  end

  def execute
    greet
  end
end
"#,
        )
        .await;

    let items = editor.prepare_call_hierarchy_at("main.rb", 2, 6).await;
    let incoming = editor.incoming_calls_for(items[0].clone()).await;
    assert_eq!(
        incoming.len(),
        2,
        "Expected 2 incoming calls, got {:?}",
        incoming
    );

    let caller_names: Vec<&str> = incoming.iter().map(|c| c.from.name.as_str()).collect();
    assert!(caller_names.contains(&"run"), "Expected 'run' in callers");
    assert!(
        caller_names.contains(&"execute"),
        "Expected 'execute' in callers"
    );
}

/// Method with no callers — returns empty.
#[tokio::test]
async fn incoming_calls_no_callers() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "main.rb",
            r#"
class Foo
  def lonely
    "no one calls me"
  end
end
"#,
        )
        .await;

    let items = editor.prepare_call_hierarchy_at("main.rb", 2, 6).await;
    assert_eq!(items.len(), 1);
    let incoming = editor.incoming_calls_for(items[0].clone()).await;
    assert!(
        incoming.is_empty(),
        "Expected no incoming calls, got {:?}",
        incoming
    );
}

// ============================================================================
// Outgoing calls — "what does this method call?"
// ============================================================================

/// Method that calls other methods.
#[tokio::test]
async fn outgoing_calls_basic() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "main.rb",
            r#"
class Checkout
  def validate; end
  def charge; end
  def notify; end

  def finalize
    validate
    charge
    notify
  end
end
"#,
        )
        .await;

    // Cursor on `finalize` (line 6, char 6)
    let items = editor.prepare_call_hierarchy_at("main.rb", 6, 6).await;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name, "finalize");

    let outgoing = editor.outgoing_calls_for(items[0].clone()).await;
    assert_eq!(
        outgoing.len(),
        3,
        "Expected 3 outgoing calls, got {:?}",
        outgoing
    );

    let callee_names: Vec<&str> = outgoing.iter().map(|c| c.to.name.as_str()).collect();
    assert!(
        callee_names.contains(&"validate"),
        "Expected 'validate' in callees"
    );
    assert!(
        callee_names.contains(&"charge"),
        "Expected 'charge' in callees"
    );
    assert!(
        callee_names.contains(&"notify"),
        "Expected 'notify' in callees"
    );
}

/// Method with no callees — empty body.
#[tokio::test]
async fn outgoing_calls_empty_body() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "main.rb",
            r#"
class Foo
  def empty; end
end
"#,
        )
        .await;

    let items = editor.prepare_call_hierarchy_at("main.rb", 2, 6).await;
    assert_eq!(items.len(), 1);
    let outgoing = editor.outgoing_calls_for(items[0].clone()).await;
    assert!(
        outgoing.is_empty(),
        "Expected no outgoing calls, got {:?}",
        outgoing
    );
}

// ============================================================================
// Cross-class calls
// ============================================================================

/// Method called from a different class via instance.
#[tokio::test]
async fn incoming_calls_cross_class() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "main.rb",
            r#"
class Greeter
  def hello
    "hi"
  end
end

class App
  def run
    g = Greeter.new
    g.hello
  end
end
"#,
        )
        .await;

    // Cursor on `hello` definition (line 2, char 6)
    let items = editor.prepare_call_hierarchy_at("main.rb", 2, 6).await;
    assert_eq!(items.len(), 1, "Expected 1 item, got {:?}", items);

    let incoming = editor.incoming_calls_for(items[0].clone()).await;
    assert_eq!(
        incoming.len(),
        1,
        "Expected 1 incoming call, got {:?}",
        incoming
    );
    assert_eq!(incoming[0].from.name, "run");
}

// ============================================================================
// Cross-file calls
// ============================================================================

/// Method called from a different file.
#[tokio::test]
async fn incoming_calls_cross_file() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "logger.rb",
            r#"
class Logger
  def log
    "logging"
  end
end
"#,
        )
        .await;
    editor
        .open(
            "app.rb",
            r#"
class App
  def run
    logger = Logger.new
    logger.log
  end
end
"#,
        )
        .await;

    // Cursor on `log` in Logger (line 2, char 6)
    let items = editor.prepare_call_hierarchy_at("logger.rb", 2, 6).await;
    assert_eq!(items.len(), 1, "Expected 1 item, got {:?}", items);

    let incoming = editor.incoming_calls_for(items[0].clone()).await;
    assert_eq!(
        incoming.len(),
        1,
        "Expected 1 incoming call, got {:?}",
        incoming
    );
    assert_eq!(incoming[0].from.name, "run");
    assert!(
        incoming[0].from.uri.path().ends_with("app.rb"),
        "Expected caller in app.rb, got {:?}",
        incoming[0].from.uri
    );
}

// ============================================================================
// Singleton methods
// ============================================================================

/// Singleton method calling and being called.
#[tokio::test]
async fn singleton_method_calls() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "main.rb",
            r#"
class Factory
  def self.create
    "created"
  end

  def self.build
    create
  end
end
"#,
        )
        .await;

    // Cursor on `create` (line 2, char 11)
    let items = editor.prepare_call_hierarchy_at("main.rb", 2, 11).await;
    assert_eq!(items.len(), 1, "Expected 1 item, got {:?}", items);

    let incoming = editor.incoming_calls_for(items[0].clone()).await;
    assert_eq!(
        incoming.len(),
        1,
        "Expected 1 incoming call, got {:?}",
        incoming
    );
    assert_eq!(incoming[0].from.name, "build");
}

// ============================================================================
// Call hierarchy from call site (cursor on method call, not definition)
// ============================================================================

/// Cursor on a method call should resolve the called method.
#[tokio::test]
async fn prepare_at_call_site() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "main.rb",
            r#"
class Foo
  def greet
    "hello"
  end

  def run
    greet
  end
end
"#,
        )
        .await;

    // Cursor on `greet` call inside `run` (line 7, char 4)
    let items = editor.prepare_call_hierarchy_at("main.rb", 7, 4).await;
    assert_eq!(items.len(), 1, "Expected 1 item, got {:?}", items);
    assert_eq!(items[0].name, "greet");
}

// ============================================================================
// Bidirectional: incoming + outgoing on the same method
// ============================================================================

/// A method that both calls and is called by others.
#[tokio::test]
async fn bidirectional_calls() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "main.rb",
            r#"
class Pipeline
  def step1; end

  def step2
    step1
  end

  def step3
    step2
  end
end
"#,
        )
        .await;

    // step2 is called by step3 and calls step1
    let items = editor.prepare_call_hierarchy_at("main.rb", 4, 6).await;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].name, "step2");

    let incoming = editor.incoming_calls_for(items[0].clone()).await;
    assert_eq!(incoming.len(), 1);
    assert_eq!(incoming[0].from.name, "step3");

    let outgoing = editor.outgoing_calls_for(items[0].clone()).await;
    assert_eq!(outgoing.len(), 1);
    assert_eq!(outgoing[0].to.name, "step1");
}

// ============================================================================
// Multiple call sites from the same caller
// ============================================================================

/// Same method called multiple times from one caller should produce one entry with multiple ranges.
#[tokio::test]
async fn multiple_call_sites_same_caller() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "main.rb",
            r#"
class Foo
  def log; end

  def run
    log
    log
    log
  end
end
"#,
        )
        .await;

    let items = editor.prepare_call_hierarchy_at("main.rb", 2, 6).await;
    assert_eq!(items.len(), 1);

    let incoming = editor.incoming_calls_for(items[0].clone()).await;
    assert_eq!(
        incoming.len(),
        1,
        "Expected 1 incoming caller (run), got {:?}",
        incoming
    );
    assert_eq!(incoming[0].from.name, "run");
    assert_eq!(
        incoming[0].from_ranges.len(),
        3,
        "Expected 3 call site ranges, got {:?}",
        incoming[0].from_ranges
    );
}
