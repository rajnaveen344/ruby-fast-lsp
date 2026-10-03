//! Document symbol outline: nesting and method visibility details.

use crate::test::harness::FakeEditor;
use tower_lsp::lsp_types::DocumentSymbol;

fn method_detail<'a>(symbols: &'a [DocumentSymbol], name: &str) -> Option<&'a str> {
    symbols.iter().find_map(|symbol| {
        if symbol.name == name {
            return symbol.detail.as_deref();
        }
        method_detail(symbol.children.as_deref().unwrap_or_default(), name)
    })
}

#[tokio::test]
async fn visibility_calls_with_arguments_change_only_the_named_methods() {
    let mut editor = FakeEditor::new().await;
    editor
        .open(
            "visibility_arguments.rb",
            r#"class Vault
  def secret
  end
  private :secret

  private def hidden
  end

  def open
  end
end
"#,
        )
        .await;

    let symbols = editor.document_symbols("visibility_arguments.rb").await;
    assert_eq!(
        method_detail(&symbols, "secret"),
        Some("private • instance method")
    );
    assert_eq!(
        method_detail(&symbols, "hidden"),
        Some("private • instance method")
    );
    assert_eq!(
        method_detail(&symbols, "open"),
        Some("public • instance method"),
        "a visibility call with arguments must not change later definitions"
    );
}
