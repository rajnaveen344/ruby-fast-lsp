use crate::indexer::file_processor::FileProcessor;
use crate::test::harness::{check, FakeEditor};
use ruby_analysis::engine::AnalysisQuery;

#[tokio::test]
async fn symbol_shape_key_completion_uses_proven_literal_fields() {
    check(
        r#"
payload = { id: 1, name: "Ada" }
payload[:na$0]
<complete items="name" excludes="id">
"#,
    )
    .await;
}

#[tokio::test]
async fn string_shape_key_completion_preserves_key_kind() {
    check(
        r#"
payload = { "id" => 1, "name" => "Ada", symbol_name: "ignored" }
payload["na$0"]
<complete items="name" excludes="id,symbol_name">
"#,
    )
    .await;
}

#[tokio::test]
async fn shape_union_key_completion_requires_every_variant() {
    check(
        r#"
result = if condition
  { kind: :number, value: 1, number_only: true }
else
  { kind: :text, value: "ready", text_only: true }
end
result[:va$0]
<complete items="value" excludes="number_only,text_only">
"#,
    )
    .await;
}

#[tokio::test]
async fn invalidated_shape_produces_no_literal_key_completion() {
    check(
        r#"
payload = { id: 1, name: "Ada" }
dynamic_sink(payload)
payload[:na$0]
<complete excludes="name,id">
"#,
    )
    .await;
}

#[tokio::test]
async fn empty_string_literal_completes_every_proven_string_key() {
    check(
        r#"
payload = { "id" => 1, "name" => "Ada" }
payload["$0"]
<complete items="id,name">
"#,
    )
    .await;
}

#[tokio::test]
async fn keyed_read_drives_chained_method_completion_from_the_same_proof() {
    check(
        r#"
payload = { name: "Ada" }
payload[:name].up$0
<complete items="upcase">
"#,
    )
    .await;
}

#[tokio::test]
async fn multiline_keyed_read_chain_uses_the_same_engine_proof() {
    check(
        r#"
payload = { name: "Ada" }
payload[:name]
  .up$0
<complete items="upcase">
"#,
    )
    .await;
}

#[tokio::test]
async fn invalidated_keyed_read_does_not_drive_chained_method_completion() {
    check(
        r#"
payload = { name: "Ada" }
dynamic_sink(payload)
payload[:name].up$0
<complete excludes="upcase">
"#,
    )
    .await;
}

#[tokio::test]
async fn cross_file_call_receiver_uses_its_engine_shape_for_key_completion() {
    let mut editor = FakeEditor::new().await;
    editor
        .open("consumer.rb", "PayloadFactory.build[:n]\n")
        .await;
    editor
        .open(
            "payload_factory.rb",
            r#"class PayloadFactory
  def self.build
    { id: 1, name: "Ada" }
  end
end
"#,
        )
        .await;

    let uri = tower_lsp::lsp_types::Url::parse("file:///consumer.rb")
        .expect("the synthetic consumer URI must be valid");
    let document = editor
        .server()
        .get_doc(&uri)
        .expect("the open consumer must retain its RubyDocument");
    let engine = editor.server().analysis_engine_for_uri(&uri);
    let payload_factory = ruby_analysis::core::FullyQualifiedName::namespace_with_kind(
        vec![ruby_analysis::core::RubyConstant::new("PayloadFactory")
            .expect("the synthetic class name must be valid")],
        ruby_analysis::core::NamespaceKind::Singleton,
    );
    let build = ruby_analysis::core::RubyMethod::new("build")
        .expect("the synthetic method name must be valid");
    let method_return = AnalysisQuery::new(&engine.read())
        .method_return_type_for_receiver(&payload_factory, &build)
        .map(|ruby_type| ruby_type.to_string());
    assert_eq!(
        method_return,
        Some("{ id: Integer, name: String }".to_string()),
        "the cross-file method equation must retain its structural return"
    );
    assert_eq!(
        AnalysisQuery::new(&engine.read())
            .expression_type_ending_at(document.analysis_file_id(), 20)
            .map(|ruby_type| ruby_type.to_string()),
        Some("{ id: Integer, name: String }".to_string()),
        "the cross-file call expression must own the shape proof before completion"
    );
    let labels = editor
        .complete_at("consumer.rb", 0, 23)
        .await
        .into_iter()
        .map(|item| item.label)
        .collect::<Vec<_>>();
    assert!(labels.contains(&"name".to_string()), "{labels:?}");
    assert!(!labels.contains(&"id".to_string()), "{labels:?}");

    editor
        .set(
            "payload_factory.rb",
            r#"class PayloadFactory
  def self.build
    { id: 1, nickname: "Ada" }
  end
end
"#,
        )
        .await;
    let labels_after_edit = editor
        .complete_at("consumer.rb", 0, 23)
        .await
        .into_iter()
        .map(|item| item.label)
        .collect::<Vec<_>>();
    assert!(
        labels_after_edit.contains(&"nickname".to_string()),
        "{labels_after_edit:?}"
    );
    assert!(
        !labels_after_edit.contains(&"name".to_string()),
        "stale structural keys must be removed after provider replacement: {labels_after_edit:?}"
    );
}

#[tokio::test]
async fn shape_key_completion_maps_the_exact_utf16_replacement_range() {
    let source = "marker = \"😀\"; payload = { name: \"Ada\" }; payload[:na]\n";
    let mut editor = FakeEditor::new().await;
    editor.open("utf16.rb", source).await;
    let replacement_start_byte = source
        .rfind("na]")
        .expect("the synthetic partial key must exist");
    let replacement_end_byte = replacement_start_byte + 2;
    let cursor_character = u32::try_from(source[..replacement_end_byte].encode_utf16().count())
        .expect("the synthetic UTF-16 cursor must fit an LSP character");
    let item = editor
        .complete_at("utf16.rb", 0, cursor_character)
        .await
        .into_iter()
        .find(|item| item.label == "name")
        .expect("the proven structural key must be offered");

    assert_eq!(
        item.kind,
        Some(tower_lsp::lsp_types::CompletionItemKind::FIELD)
    );
    let tower_lsp::lsp_types::CompletionTextEdit::Edit(edit) = item
        .text_edit
        .expect("shape-key completion must replace the existing partial literal")
    else {
        panic!(
            "INVARIANT VIOLATED: shape-key completion emitted an insert/replace edit. This is a bug because the adapter owns one exact literal-content range. Fix: map the domain replacement range to CompletionTextEdit::Edit."
        );
    };
    let expected_start = u32::try_from(source[..replacement_start_byte].encode_utf16().count())
        .expect("the synthetic UTF-16 replacement start must fit an LSP character");
    let expected_end = u32::try_from(source[..replacement_end_byte].encode_utf16().count())
        .expect("the synthetic UTF-16 replacement end must fit an LSP character");
    assert_eq!(
        edit.range,
        tower_lsp::lsp_types::Range::new(
            tower_lsp::lsp_types::Position::new(0, expected_start),
            tower_lsp::lsp_types::Position::new(0, expected_end),
        )
    );
    assert_eq!(edit.new_text, "name");
}

#[tokio::test]
async fn declared_optional_rbs_record_key_is_available_to_completion() {
    let mut editor = FakeEditor::new().await;
    let signature_uri = tower_lsp::lsp_types::Url::parse("file:///sig/payload_factory.rbs")
        .expect("the synthetic signature URI must be valid");
    FileProcessor::default()
        .collect_rbs_facts(
            &signature_uri,
            "class PayloadFactory\n  def self.build: () -> { id: Integer, ?name: String }\nend\n",
            editor.server(),
        )
        .expect("the RBS record contract must enter the shared engine");
    editor
        .open("consumer.rb", "PayloadFactory.build[:na]\n")
        .await;

    let labels = editor
        .complete_at("consumer.rb", 0, 24)
        .await
        .into_iter()
        .map(|item| item.label)
        .collect::<Vec<_>>();
    assert!(labels.contains(&"name".to_string()), "{labels:?}");
    assert!(!labels.contains(&"id".to_string()), "{labels:?}");
}
