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

const SHOP_INTEGER: &str = "class Shop\n  def count\n    1\n  end\nend\n";
const SHOP_STRING: &str = "class Shop\n  def count\n    \"one\"\n  end\nend\n";
const LEDGER: &str = "class Ledger\n  def total\n    Shop.new.count\n  end\nend\n";

#[tokio::test]
async fn editing_a_return_type_updates_callers_reached_through_a_closed_file() {
    let mut editor = FakeEditor::new().await;
    editor.open("shop.rb", SHOP_INTEGER).await;
    editor.open("ledger.rb", LEDGER).await;
    editor.close("ledger.rb").await;
    editor
        .open(
            "report.rb",
            "class Report\n  def run\n    Ledger.new.total.upcase\n  end\nend\n",
        )
        .await;
    let integer = "class Report\n  def run\n    Ledger.new.total.<warn code=\"unresolved-method\">upcase</warn>\n  end\nend\n";
    let string = "class Report\n  def run\n    <warn none code=\"unresolved-method\">Ledger.new.total.upcase</warn>\n  end\nend\n";
    editor.check("report.rb", integer).await;

    editor.set("shop.rb", SHOP_STRING).await;
    editor.check("report.rb", string).await;

    editor.set("shop.rb", SHOP_INTEGER).await;
    editor.check("report.rb", integer).await;
}

const LEDGER_WITH_LOCAL_HOPS: &str = "class Ledger\n  def count\n    Shop.new.count\n  end\n\n  def total\n    value = count\n    value\n  end\n\n  def grand_total\n    sum = total\n    sum\n  end\nend\n\nclass Audit\n  def figure\n    Ledger.new.count\n  end\nend\n";

#[tokio::test]
async fn editing_a_return_type_updates_callers_reached_through_same_file_hops_in_a_closed_file() {
    let mut editor = FakeEditor::new().await;
    editor.open("shop.rb", SHOP_INTEGER).await;
    editor.open("ledger.rb", LEDGER_WITH_LOCAL_HOPS).await;
    editor.close("ledger.rb").await;
    editor
        .open(
            "report.rb",
            "class Report\n  def run\n    Ledger.new.total.upcase\n    Ledger.new.grand_total.upcase\n    Audit.new.figure.upcase\n  end\nend\n",
        )
        .await;
    let integer = "class Report\n  def run\n    Ledger.new.total.<warn code=\"unresolved-method\">upcase</warn>\n    Ledger.new.grand_total.<warn code=\"unresolved-method\">upcase</warn>\n    Audit.new.figure.<warn code=\"unresolved-method\">upcase</warn>\n  end\nend\n";
    let string = "class Report\n  def run\n    <warn none code=\"unresolved-method\">Ledger.new.total.upcase\n    Ledger.new.grand_total.upcase\n    Audit.new.figure.upcase</warn>\n  end\nend\n";
    editor.check("report.rb", integer).await;

    editor.set("shop.rb", SHOP_STRING).await;
    editor.check("report.rb", string).await;

    editor.set("shop.rb", SHOP_INTEGER).await;
    editor.check("report.rb", integer).await;
}
