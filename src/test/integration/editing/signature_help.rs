use crate::test::harness::FakeEditor;
use tower_lsp::lsp_types::{InitializeParams, ParameterLabel};
use tower_lsp::LanguageServer;

#[tokio::test]
async fn advertises_signature_help_with_ruby_trigger_characters() {
    let editor = FakeEditor::new().await;
    let initialized = editor
        .server()
        .initialize(InitializeParams::default())
        .await
        .expect("signature help capability initialization must succeed");
    let options = initialized
        .capabilities
        .signature_help_provider
        .expect("signature help must be advertised");

    assert_eq!(
        options.trigger_characters,
        Some(vec!["(".to_string(), ",".to_string()])
    );
}

#[tokio::test]
async fn user_method_signature_help_reports_parameter_shapes_and_active_parameter() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "greeter.rb",
            r#"class Greeter
  def greet(name, punctuation = "!", *extras, loud:, suffix: nil, **options, &block)
  end
end

Greeter.new.greet("Ruby", "?")
"#,
        )
        .await;

    let help = editor
        .signature_help_at("greeter.rb", 5, 30)
        .await
        .expect("signature help should resolve the user-defined method");

    assert_eq!(help.active_signature, Some(0));
    assert_eq!(help.active_parameter, Some(1));
    assert_eq!(help.signatures.len(), 1);
    assert_eq!(
        help.signatures[0].label,
        "greet(name, punctuation = ..., *extras, loud:, suffix: ..., **options, &block)"
    );

    let parameters = help.signatures[0]
        .parameters
        .as_ref()
        .expect("signature parameters must be present");
    let labels = parameters
        .iter()
        .map(|parameter| match &parameter.label {
            ParameterLabel::Simple(label) => label.as_str(),
            ParameterLabel::LabelOffsets(_) => {
                panic!("signature help must use explicit parameter labels")
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        labels,
        vec![
            "name",
            "punctuation = ...",
            "*extras",
            "loud:",
            "suffix: ...",
            "**options",
            "&block",
        ]
    );
}

#[tokio::test]
async fn nested_call_signature_help_selects_the_innermost_call() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "nested.rb",
            r#"def outer(value, fallback)
end

def inner(first, second)
end

outer(inner(1, 2), 3)
"#,
        )
        .await;

    let help = editor
        .signature_help_at("nested.rb", 6, 16)
        .await
        .expect("nested signature help should resolve");
    assert_eq!(help.signatures[0].label, "inner(first, second)");
    assert_eq!(help.active_parameter, Some(1));
}

#[tokio::test]
async fn keyword_argument_selects_the_matching_declared_keyword() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "keywords.rb",
            r#"def configure(path, retries = 1, mode:, verbose: false)
end

configure("app", mode: "fast")
"#,
        )
        .await;

    let help = editor
        .signature_help_at("keywords.rb", 3, 29)
        .await
        .expect("keyword signature help should resolve");
    assert_eq!(help.active_parameter, Some(2));
}

#[tokio::test]
async fn signature_help_updates_after_method_reindex() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "lifecycle.rb",
            "def publish(message)\nend\n\npublish(\"hello\")\n",
        )
        .await;
    let before = editor
        .signature_help_at("lifecycle.rb", 3, 15)
        .await
        .expect("initial signature help should resolve");
    assert_eq!(before.signatures[0].label, "publish(message)");

    editor
        .set(
            "lifecycle.rb",
            "def publish(message, channel:)\nend\n\npublish(\"hello\", channel: \"ops\")\n",
        )
        .await;
    let after = editor
        .signature_help_at("lifecycle.rb", 3, 31)
        .await
        .expect("reindexed signature help should resolve");
    assert_eq!(after.signatures[0].label, "publish(message, channel:)");
    assert_eq!(after.active_parameter, Some(1));
}

#[tokio::test]
async fn inherited_method_signature_uses_engine_mro_resolution() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "inheritance.rb",
            r#"class Base
  def deliver(message, priority:)
  end
end

class Child < Base
end

Child.new.deliver("hello", priority: 1)
"#,
        )
        .await;

    let help = editor
        .signature_help_at("inheritance.rb", 8, 39)
        .await
        .expect("inherited signature help should resolve through the MRO");
    assert_eq!(help.signatures[0].label, "deliver(message, priority:)");
}

#[tokio::test]
async fn explicit_receiver_private_method_has_no_signature_help() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "private.rb",
            r#"class Secret
  private

  def reveal(token)
  end
end

Secret.new.reveal("token")
"#,
        )
        .await;

    assert!(
        editor
            .signature_help_at("private.rb", 7, 26)
            .await
            .is_none(),
        "explicit private calls must not expose a callable signature"
    );
}

#[tokio::test]
async fn rbs_builtin_signature_help_reports_parameter_types() {
    let mut editor = FakeEditor::new().await;
    editor.open("rbs.rb", "\"hello\".sub(\"h\", \"j\")\n").await;

    let help = editor
        .signature_help_at("rbs.rb", 0, 20)
        .await
        .expect("RBS-backed signature help should resolve String#sub");
    assert!(
        help.signatures
            .iter()
            .any(|signature| signature.label.contains("pattern")
                && signature.label.contains("replacement")
                && signature.label.contains("String")),
        "expected typed String#sub signature, got {:?}",
        help.signatures
    );
    assert_eq!(help.active_parameter, Some(1));
}

#[tokio::test]
async fn yard_signature_help_includes_types_and_documentation() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "yard.rb",
            r#"# Greets a user by name.
# @param name [String] the user's display name
# @return [String] the rendered greeting
def greet(name)
  "Hello #{name}"
end

greet("Ruby")
"#,
        )
        .await;

    let help = editor
        .signature_help_at("yard.rb", 7, 12)
        .await
        .expect("YARD-backed signature help should resolve");
    let signature = &help.signatures[0];
    assert_eq!(signature.label, "greet(name: String) -> String");
    let documentation = signature
        .documentation
        .as_ref()
        .expect("method documentation should be present");
    assert!(format!("{documentation:?}").contains("Greets a user by name"));
    let parameter = signature.parameters.as_ref().unwrap()[0]
        .documentation
        .as_ref()
        .expect("parameter documentation should be present");
    assert!(format!("{parameter:?}").contains("display name"));
}

#[tokio::test]
async fn ambiguous_reopened_method_returns_each_signature() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "first.rb",
            "class Service\n  def call(message)\n  end\nend\n",
        )
        .await;
    editor
        .open(
            "second.rb",
            "class Service\n  def call(message, retries)\n  end\nend\n",
        )
        .await;
    editor
        .open("usage.rb", "Service.new.call(\"hello\", 2)\n")
        .await;

    let help = editor
        .signature_help_at("usage.rb", 0, 28)
        .await
        .expect("ambiguous definitions should expose overload choices");
    let labels = help
        .signatures
        .iter()
        .map(|signature| signature.label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(labels, vec!["call(message)", "call(message, retries)"]);
}

#[tokio::test]
async fn yard_signature_metadata_resolves_across_files() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "definition.rb",
            "# Converts a value.\n# @param value [Integer] input value\n# @return [String] rendered value\ndef render(value)\nend\n",
        )
        .await;
    editor.open("usage.rb", "render(42)\n").await;

    let help = editor
        .signature_help_at("usage.rb", 0, 9)
        .await
        .expect("cross-file YARD signature help should resolve");
    assert_eq!(help.signatures[0].label, "render(value: Integer) -> String");
    assert!(format!("{:?}", help.signatures[0].documentation).contains("Converts a value"));
}

#[tokio::test]
async fn extra_positional_arguments_keep_the_rest_parameter_active() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "rest.rb",
            "def collect(first, *remaining, mode:, **options, &block)\nend\n\ncollect(1, 2, 3)\n",
        )
        .await;

    let help = editor
        .signature_help_at("rest.rb", 3, 15)
        .await
        .expect("rest parameter signature help should resolve");
    assert_eq!(help.active_parameter, Some(1));
}

#[tokio::test]
async fn unknown_keyword_selects_the_keyword_rest_parameter() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "keyword_rest.rb",
            "def configure(path, mode:, **options)\nend\n\nconfigure(\"app\", timeout: 5)\n",
        )
        .await;

    let help = editor
        .signature_help_at("keyword_rest.rb", 3, 27)
        .await
        .expect("keyword-rest signature help should resolve");
    assert_eq!(help.active_parameter, Some(2));
}
