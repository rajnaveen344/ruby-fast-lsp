//! Goto definition tests for method calls without receivers in various contexts.
//!
//! These tests verify that implicit `self` method calls resolve correctly when
//! invoked from different syntactic contexts (blocks, lambdas, rescue, etc.).
//!
//! The key behavior being tested: the enclosing method's context should propagate
//! through nested scopes (blocks, lambdas) so that bare method calls resolve to
//! the correct namespace.

use crate::test::harness::{check, FakeEditor};

async fn check_unproven_block_receiver(fixture: &str) {
    let untagged = fixture.replace("<def>", "").replace("</def>", "");
    let marker = untagged
        .find("$0")
        .expect("unproven block receiver fixture must contain one cursor");
    let before = &untagged[..marker];
    let line = before.matches('\n').count() as u32;
    let character = (marker - before.rfind('\n').map(|index| index + 1).unwrap_or(0)) as u32;
    let source = untagged.replace("$0", "");
    let mut editor = FakeEditor::new().await;
    editor.open("inline_test.rb", &source).await;
    let definitions = editor.goto_def_at("inline_test.rb", line, character).await;
    assert!(
        definitions.is_empty(),
        "an ordinary block without an execution-context contract must not guess lexical self, got {definitions:?}"
    );
}

// ============================================================================
// Block Contexts
// ============================================================================

/// An ordinary block can be executed with a rebound `self`; without a callee
/// contract, navigation must not guess the lexical class's method.
#[tokio::test]
async fn goto_method_inside_block() {
    check_unproven_block_receiver(
        r#"
class Processor
  <def>def helper
    "helping"
  end</def>

  def process(items)
    items.each do |item|
      helper$0
    end
  end
end
"#,
    )
    .await;
}

/// Brace blocks obey the same proof barrier as do/end blocks.
#[tokio::test]
async fn goto_method_inside_brace_block() {
    check_unproven_block_receiver(
        r#"
class Mapper
  <def>def transform(x)
    x * 2
  end</def>

  def map_all(items)
    items.map { |x| transform$0(x) }
  end
end
"#,
    )
    .await;
}

/// Nested ordinary blocks remain unknown without an execution contract.
#[tokio::test]
async fn goto_method_inside_nested_blocks() {
    check_unproven_block_receiver(
        r#"
class DataProcessor
  <def>def validate(item)
    item.valid?
  end</def>

  def process(data)
    data.each do |group|
      group.each do |item|
        validate$0(item)
      end
    end
  end
end
"#,
    )
    .await;
}

// ============================================================================
// Lambda and Proc Contexts
// ============================================================================

/// Method call inside a lambda should resolve to the enclosing class's method.
#[tokio::test]
async fn goto_method_inside_lambda() {
    check(
        r#"
class Handler
  <def>def log(msg)
    puts msg
  end</def>

  def create_handler
    ->(event) { log$0(event.to_s) }
  end
end
"#,
    )
    .await;
}

/// Method call inside a stabby lambda should resolve correctly.
#[tokio::test]
async fn goto_method_inside_stabby_lambda() {
    check(
        r#"
class Calculator
  <def>def compute(x)
    x * 2
  end</def>

  def setup
    @processor = -> (n) { compute$0(n) }
  end
end
"#,
    )
    .await;
}

/// A Proc can be supplied to an API that rebinds `self`, so its bare receiver
/// remains unknown until the invocation contract is proven.
#[tokio::test]
async fn goto_method_inside_proc_new() {
    check_unproven_block_receiver(
        r#"
class Worker
  <def>def perform_task
    "working"
  end</def>

  def create_task
    Proc.new { perform_task$0 }
  end
end
"#,
    )
    .await;
}

// ============================================================================
// Exception Handling Contexts
// ============================================================================

/// Method call inside rescue block should resolve correctly.
#[tokio::test]
async fn goto_method_inside_rescue() {
    check(
        r#"
class SafeExecutor
  <def>def handle_error(e)
    puts e.message
  end</def>

  def execute
    begin
      risky_operation
    rescue StandardError => e
      handle_error$0(e)
    end
  end
end
"#,
    )
    .await;
}

/// Method call inside ensure block should resolve correctly.
#[tokio::test]
async fn goto_method_inside_ensure() {
    check(
        r#"
class ResourceManager
  <def>def cleanup
    "cleaning up"
  end</def>

  def with_resource
    begin
      yield
    ensure
      cleanup$0
    end
  end
end
"#,
    )
    .await;
}

/// Method call inside else block of begin/rescue should resolve correctly.
#[tokio::test]
async fn goto_method_inside_rescue_else() {
    check(
        r#"
class ResultHandler
  <def>def on_success
    "success"
  end</def>

  def try_operation
    begin
      operation
    rescue
      "failed"
    else
      on_success$0
    end
  end
end
"#,
    )
    .await;
}

// ============================================================================
// Conditional Contexts
// ============================================================================

/// Method call inside if condition should resolve correctly.
#[tokio::test]
async fn goto_method_inside_if_condition() {
    check(
        r#"
class Validator
  <def>def valid?
    true
  end</def>

  def check
    if valid?$0
      "ok"
    end
  end
end
"#,
    )
    .await;
}

/// Method call inside if body should resolve correctly.
#[tokio::test]
async fn goto_method_inside_if_body() {
    check(
        r#"
class Notifier
  <def>def send_notification
    "sent"
  end</def>

  def notify_if_needed(condition)
    if condition
      send_notification$0
    end
  end
end
"#,
    )
    .await;
}

/// Method call inside case/when should resolve correctly.
#[tokio::test]
async fn goto_method_inside_case_when() {
    check(
        r#"
class Router
  <def>def handle_get
    "GET handler"
  end</def>

  def route(method)
    case method
    when :get
      handle_get$0
    end
  end
end
"#,
    )
    .await;
}

// ============================================================================
// Loop Contexts
// ============================================================================

/// Method call inside while loop should resolve correctly.
#[tokio::test]
async fn goto_method_inside_while_loop() {
    check(
        r#"
class Poller
  <def>def poll_once
    "polled"
  end</def>

  def poll_until_done
    while running?
      poll_once$0
    end
  end
end
"#,
    )
    .await;
}

/// Method call inside until loop should resolve correctly.
#[tokio::test]
async fn goto_method_inside_until_loop() {
    check(
        r#"
class Retrier
  <def>def attempt
    "attempting"
  end</def>

  def retry_until_success
    until success?
      attempt$0
    end
  end
end
"#,
    )
    .await;
}

/// Method call inside for loop should resolve correctly.
#[tokio::test]
async fn goto_method_inside_for_loop() {
    check(
        r#"
class BatchProcessor
  <def>def process_item(item)
    item.to_s
  end</def>

  def process_batch(items)
    for item in items
      process_item$0(item)
    end
  end
end
"#,
    )
    .await;
}

// ============================================================================
// Module Method Contexts
// ============================================================================

/// Bare method call inside module method (not included anywhere).
#[tokio::test]
async fn goto_method_inside_module_method() {
    check(
        r#"
module Utilities
  <def>def helper
    "helping"
  end</def>

  def run
    helper$0
  end
end
"#,
    )
    .await;
}

/// Method call inside module with module_function.
#[tokio::test]
async fn goto_module_function_call() {
    check(
        r#"
module FileUtils
  <def>def read_file(path)
    File.read(path)
  end</def>
  module_function :read_file

  def process_file(path)
    content = read_file$0(path)
    content.upcase
  end
  module_function :process_file
end
"#,
    )
    .await;
}

// ============================================================================
// Singleton Class Method Contexts
// ============================================================================

/// Method call inside singleton class method body.
#[tokio::test]
async fn goto_method_inside_singleton_class_method() {
    check(
        r#"
class Configuration
  class << self
    <def>def load_defaults
      {}
    end</def>

    def configure
      defaults = load_defaults$0
      defaults.merge(custom_config)
    end
  end
end
"#,
    )
    .await;
}

/// Private singleton method called from another singleton method.
#[tokio::test]
async fn goto_private_singleton_from_singleton() {
    check(
        r#"
class API
  class << self
    def fetch
      build_request$0
    end

    private

    <def>def build_request
      {}
    end</def>
  end
end
"#,
    )
    .await;
}

// ============================================================================
// Class Method (def self.x) Contexts
// ============================================================================

/// Bare method call inside class method (def self.x style).
#[tokio::test]
async fn goto_method_inside_class_method_def_self() {
    check(
        r#"
class Service
  <def>def self.helper
    "helping"
  end</def>

  def self.run
    helper$0
  end
end
"#,
    )
    .await;
}

/// A block created in a class method still needs an execution-context contract
/// before its implicit receiver can be published.
#[tokio::test]
async fn goto_method_inside_class_method_with_block() {
    check_unproven_block_receiver(
        r#"
class BatchService
  <def>def self.process_item(item)
    item.to_s
  end</def>

  def self.process_all(items)
    items.map do |item|
      process_item$0(item)
    end
  end
end
"#,
    )
    .await;
}
