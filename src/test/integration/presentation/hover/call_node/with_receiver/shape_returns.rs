//! Hover on keyed reads of Hash shapes and RBS records returned across files.

use crate::loader::file_processor::FileProcessor;
use crate::test::harness::{check_multi_file, FakeEditor};

#[tokio::test]
async fn cross_file_hash_shape_return_reaches_keyed_reads_after_early_consumer_open() {
    check_multi_file(&[
        (
            "consumer.rb",
            r#"payload = PayloadFactory.build
payload[:name]<hover label="String">
"#,
        ),
        (
            "payload_factory.rb",
            r#"class PayloadFactory
  def self.build
    { id: 1, name: "Ada" }
  end
end
"#,
        ),
    ])
    .await;
}

#[tokio::test]
async fn rbs_record_return_reaches_cross_file_keyed_reads() {
    let mut editor = FakeEditor::new().await;
    let consumer = "payload = PayloadFactory.build\npayload[:name]\n";
    let signature_uri = crate::test::harness::fixture_uri("/sig/payload_factory.rbs");
    FileProcessor::default()
        .collect_rbs_facts(
            &signature_uri,
            r#"class PayloadFactory
  def self.build: () -> { id: Integer, ?name: String }
end
"#,
            editor.server(),
        )
        .expect("RBS record signature must enter the shared engine");
    editor.open("consumer.rb", consumer).await;
    editor
        .check(
            "consumer.rb",
            "payload = PayloadFactory.build\npayload[:name]<hover label=\"(NilClass | String)\">\n",
        )
        .await;
}

#[tokio::test]
async fn rbs_record_parameter_contract_seeds_ruby_method_flow() {
    let mut editor = FakeEditor::new().await;
    let signature_uri = crate::test::harness::fixture_uri("/sig/payload_service.rbs");
    FileProcessor::default()
        .collect_rbs_facts(
            &signature_uri,
            r#"class PayloadService
  def normalize: ({ kind: :number, value: Integer } payload) -> Integer
end
"#,
            editor.server(),
        )
        .expect("RBS record parameter must enter the shared engine");
    editor
        .open_and_check_fixture(
            "payload_service.rb",
            r#"class PayloadService
  def normalize(payload)
    payload[:value]<hover label="Integer">.abs
  end
end
"#,
        )
        .await;
}

#[tokio::test]
async fn overloaded_rbs_record_parameters_seed_an_exhaustive_correlated_union() {
    let mut editor = FakeEditor::new().await;
    let signature_uri = crate::test::harness::fixture_uri("/sig/payload_service.rbs");
    FileProcessor::default()
        .collect_rbs_facts(
            &signature_uri,
            r#"class PayloadService
  def normalize: ({ kind: :number, value: Integer } payload) -> Integer
               | ({ kind: :text, value: String } payload) -> String
end
"#,
            editor.server(),
        )
        .expect("overloaded RBS records must enter the shared engine");
    editor
        .open_and_check_fixture(
            "payload_service.rb",
            r#"class PayloadService
  def normalize(payload)
    if payload[:kind] == :number
      payload[:value]<hover label="Integer">.abs
    else
      payload[:value]<hover label="String">.upcase
    end
  end
end
"#,
        )
        .await;
}

#[tokio::test]
async fn cross_file_value_constant_shape_reaches_keyed_reads_after_early_consumer_open() {
    check_multi_file(&[
        (
            "consumer.rb",
            r#"PAYLOAD[:name]<hover label="String">
"#,
        ),
        (
            "payload.rb",
            r#"PAYLOAD = { id: 1, name: "Ada" }
"#,
        ),
    ])
    .await;
}

#[tokio::test]
async fn cross_file_shape_return_edit_removes_and_reproves_consumer_evidence() {
    let mut editor = FakeEditor::new().await;
    let consumer = "payload = PayloadFactory.build\npayload[:name]\n";
    editor.open("consumer.rb", consumer).await;
    editor
        .open(
            "payload_factory.rb",
            "class PayloadFactory\n  def self.build\n    { name: \"Ada\" }\n  end\nend\n",
        )
        .await;
    editor
        .check(
            "consumer.rb",
            "payload = PayloadFactory.build\npayload[:name]<hover label=\"String\">\n",
        )
        .await;

    editor
        .set(
            "payload_factory.rb",
            "class PayloadFactory\n  def self.build\n    dynamic_payload\n  end\nend\n",
        )
        .await;
    editor
        .check(
            "consumer.rb",
            "payload = PayloadFactory.build\npayload[:name]<hover label=\"?\">\n",
        )
        .await;

    editor
        .set(
            "payload_factory.rb",
            "class PayloadFactory\n  def self.build\n    { name: 42 }\n  end\nend\n",
        )
        .await;
    editor
        .check(
            "consumer.rb",
            "payload = PayloadFactory.build\npayload[:name]<hover label=\"Integer\">\n",
        )
        .await;
}

#[tokio::test]
async fn cross_file_shape_return_is_independent_of_open_order() {
    let consumer = "payload = PayloadFactory.build\npayload[:name]\n";
    let provider = "class PayloadFactory\n  def self.build\n    { name: \"Ada\" }\n  end\nend\n";
    let expected = "payload = PayloadFactory.build\npayload[:name]<hover label=\"String\">\n";

    for provider_first in [false, true] {
        let mut editor = FakeEditor::new().await;
        if provider_first {
            editor.open("payload_factory.rb", provider).await;
            editor.open("consumer.rb", consumer).await;
        } else {
            editor.open("consumer.rb", consumer).await;
            editor.open("payload_factory.rb", provider).await;
        }
        editor.check("consumer.rb", expected).await;
    }
}
