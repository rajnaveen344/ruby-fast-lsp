//! Inlay hint tests for YARD-documented methods.

use crate::test::harness::get_inlay_hints;

/// YARD @return shows method return type hint.
#[tokio::test]
async fn yard_return_type() {
    let hints = get_inlay_hints(
        r#"
class Greeter
  # @return [String] the greeting
  def greet
    "hello"
  end
end
"#,
    )
    .await;

    // Method should have return type hint
    let return_hints: Vec<_> = hints
        .iter()
        .filter(|h| {
            let label = match &h.label {
                tower_lsp::lsp_types::InlayHintLabel::String(s) => s.clone(),
                tower_lsp::lsp_types::InlayHintLabel::LabelParts(parts) => {
                    parts.iter().map(|p| p.value.clone()).collect::<String>()
                }
            };
            label.contains("-> String")
        })
        .collect();

    assert!(
        !return_hints.is_empty(),
        "Expected return type hint, got: {:?}",
        hints
    );
}

/// YARD @param shows parameter type hint.
#[tokio::test]
async fn yard_param_type() {
    let hints = get_inlay_hints(
        r#"
class Greeter
  # @param name [String] the name
  # @return [String]
  def greet(name)
    "Hello, #{name}"
  end
end
"#,
    )
    .await;

    let param_hints: Vec<_> = hints
        .iter()
        .filter(|h| {
            let label = match &h.label {
                tower_lsp::lsp_types::InlayHintLabel::String(s) => s.clone(),
                tower_lsp::lsp_types::InlayHintLabel::LabelParts(parts) => {
                    parts.iter().map(|p| p.value.clone()).collect::<String>()
                }
            };
            label.contains("String") && !label.contains("->")
        })
        .collect();

    assert!(
        !param_hints.is_empty(),
        "Expected param type hint, got: {:?}",
        hints
    );
}

/// Method without YARD has no type hints.
#[tokio::test]
async fn no_yard_no_method_hints() {
    let hints = get_inlay_hints(
        r#"
class Greeter
  def greet
    "hello"
  end
end
"#,
    )
    .await;

    // No method-level type hints without YARD
    let method_hints: Vec<_> = hints
        .iter()
        .filter(|h| {
            let label = match &h.label {
                tower_lsp::lsp_types::InlayHintLabel::String(s) => s.clone(),
                tower_lsp::lsp_types::InlayHintLabel::LabelParts(parts) => {
                    parts.iter().map(|p| p.value.clone()).collect::<String>()
                }
            };
            label.contains("->")
        })
        .collect();

    assert!(
        method_hints.is_empty(),
        "Expected no method hints without YARD, got: {:?}",
        method_hints
    );
}
