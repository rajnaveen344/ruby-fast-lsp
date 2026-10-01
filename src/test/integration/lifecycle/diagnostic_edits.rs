//! Deleting a definition in one file reports its consumers in another open file,
//! and restoring the definition clears that report.

use crate::test::harness::FakeEditor;

const GATEWAY: &str = "class Gateway\n  def capture\n    \"captured\"\n  end\nend\n";
const INVOICE: &str = "class Invoice\n  CURRENCY = \"USD\"\nend\n";
const CALLER: &str =
    "class Billing\n  def charge\n    Gateway.new.capture\n    Invoice::CURRENCY\n  end\nend\n";

#[tokio::test]
async fn deleting_and_restoring_a_method_updates_caller_diagnostics() {
    let mut editor = FakeEditor::new().await;
    editor.open("gateway.rb", GATEWAY).await;
    editor.open("invoice.rb", INVOICE).await;
    editor.open("billing.rb", CALLER).await;
    let clean = "class Billing\n  def charge\n    <warn none code=\"unresolved-method\">Gateway.new.capture</warn>\n    Invoice::CURRENCY\n  end\nend\n";
    editor.check("billing.rb", clean).await;

    editor.set("gateway.rb", "class Gateway\nend\n").await;
    editor
        .check(
            "billing.rb",
            "class Billing\n  def charge\n    Gateway.new.<warn code=\"unresolved-method\">capture</warn>\n    Invoice::CURRENCY\n  end\nend\n",
        )
        .await;

    editor.set("gateway.rb", GATEWAY).await;
    editor.check("billing.rb", clean).await;
}

#[tokio::test]
async fn deleting_and_restoring_a_constant_updates_caller_diagnostics() {
    let mut editor = FakeEditor::new().await;
    editor.open("gateway.rb", GATEWAY).await;
    editor.open("invoice.rb", INVOICE).await;
    editor.open("billing.rb", CALLER).await;
    let clean = "class Billing\n  def charge\n    Gateway.new.capture\n    <err none code=\"unresolved-constant\">Invoice::CURRENCY</err>\n  end\nend\n";
    editor.check("billing.rb", clean).await;

    editor.set("invoice.rb", "class Invoice\nend\n").await;
    editor
        .check(
            "billing.rb",
            "class Billing\n  def charge\n    Gateway.new.capture\n    <err code=\"unresolved-constant\">Invoice::CURRENCY</err>\n  end\nend\n",
        )
        .await;

    editor.set("invoice.rb", INVOICE).await;
    editor.check("billing.rb", clean).await;
}
