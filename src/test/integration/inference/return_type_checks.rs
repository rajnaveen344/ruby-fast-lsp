//! Tests for return type inference and diagnostics.

use crate::indexer::file_processor::FileProcessor;
use crate::test::harness::{check, FakeEditor};

#[tokio::test]
async fn test_explicit_return_mismatch() {
    check(
        r#"
class A
  # @return [String]
  def foo
    <warn message="Expected return type String, but found Integer">return 1</warn>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn test_implicit_return_mismatch() {
    check(
        r#"
class A
  # @return [String]
  def foo
    <warn message="Expected return type String, but found Integer">1</warn>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn test_valid_return() {
    check(
        r#"
<err none>
class A
  # @return [Integer]
  def foo
    1
  end
end
</err>
"#,
    )
    .await;
}

#[tokio::test]
async fn unknown_declared_return_type_does_not_prove_a_mismatch() {
    check(
        r#"
class A
  # @return [?]
  def foo
    <warn none code="declared-return-type-mismatch">nil</warn>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn unknown_nested_return_type_does_not_prove_a_mismatch() {
    check(
        r#"
class A
  # @return [Array<Integer>]
  def foo
    <warn none code="declared-return-type-mismatch">return 1, dynamic_value</warn>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn inferred_return_type_is_not_treated_as_a_declaration() {
    check(
        r#"
class A
  def foo
    value = "value"
    <warn none code="declared-return-type-mismatch">return value</warn>
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn complete_rbs_record_contract_reports_a_structural_return_mismatch() {
    let mut editor = FakeEditor::new().await;
    let signature_uri = tower_lsp::lsp_types::Url::parse("file:///sig/payload_factory.rbs")
        .expect("test signature URI must be valid");
    FileProcessor::default()
        .collect_rbs_facts(
            &signature_uri,
            "class PayloadFactory\n  def build: () -> { id: Integer }\nend\n",
            editor.server(),
        )
        .expect("RBS record return contract must enter the shared engine");
    editor
        .open_and_check_fixture(
            "payload_factory.rb",
            r#"class PayloadFactory
  def build
    <warn code="declared-return-type-mismatch" message="Expected return type { id: Integer }, but found { id: String }">{ id: "wrong" }</warn>
  end
end
"#,
        )
        .await;
}

#[tokio::test]
async fn incomplete_rbs_record_return_evidence_does_not_report_a_mismatch() {
    let mut editor = FakeEditor::new().await;
    let signature_uri = tower_lsp::lsp_types::Url::parse("file:///sig/payload_factory.rbs")
        .expect("test signature URI must be valid");
    FileProcessor::default()
        .collect_rbs_facts(
            &signature_uri,
            "class PayloadFactory\n  def build: () -> { id: Integer }\nend\n",
            editor.server(),
        )
        .expect("RBS record return contract must enter the shared engine");
    editor
        .open_and_check_fixture(
            "payload_factory.rb",
            r#"class PayloadFactory
  def build
    <warn none code="declared-return-type-mismatch">{ id: dynamic_value }</warn>
  end
end
"#,
        )
        .await;
}
