//! Acceptance tests for signature-driven higher-order call inference.
//!
//! These fixtures intentionally exercise the shared LSP/engine path. Keep
//! collection behavior out of the assertions themselves: the expected type
//! must come from the selected callable signature and inferred block.

use crate::indexer::file_processor::FileProcessor;
use crate::test::harness::{check, FakeEditor};

#[tokio::test]
async fn explicit_map_block_substitutes_its_result_into_the_array() {
    check(
        r#"
values = [1, 2]
strings<hint label="Array<String>"> = values.map { |value| value.to_s }
"#,
    )
    .await;
}

#[tokio::test]
async fn branching_map_block_preserves_the_exhaustive_element_union() {
    check(
        r#"
values = [1, 2]
mapped<hint label="Array<(Integer | String)>"> = values.map do |value|
  if condition
    value
  else
    value.to_s
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn static_symbol_to_proc_resolves_for_every_element_member() {
    check(
        r#"
values = [1, "2"]
strings<hint label="Array<String>"> = values.map(&:to_s)
"#,
    )
    .await;
}

#[tokio::test]
async fn shape_projection_through_map_preserves_the_field_type() {
    check(
        r#"
rows = [{ id: 1, name: "Ada" }, { id: 2, name: "Grace" }]
names<hint label="Array<String>"> = rows.map { |row| row[:name] }
"#,
    )
    .await;
}

#[tokio::test]
async fn filter_map_removes_only_proven_nil_and_false_members() {
    check(
        r#"
values = [1, 2]
strings<hint label="Array<String>"> = values.filter_map do |value|
  if condition
    value.to_s
  else
    nil
  end
end
"#,
    )
    .await;
}

#[tokio::test]
async fn select_retains_the_proven_input_element_type() {
    check(
        r#"
values = [1, "ready"]
selected<hint label="Array<(Integer | String)>"> = values.select { |value| value }
"#,
    )
    .await;
}

#[tokio::test]
async fn collection_aliases_and_filters_use_the_same_signature_solver() {
    check(
        r#"
values = [1, "ready"]
collected<hint label="Array<String>"> = values.collect { |value| value.to_s }
filtered<hint label="Array<(Integer | String)>"> = values.filter { |value| value }
rejected<hint label="Array<(Integer | String)>"> = values.reject { |value| value }
"#,
    )
    .await;
}

#[tokio::test]
async fn three_stage_pipeline_preserves_the_element_proof() {
    check(
        r#"
rows = [{ name: "Ada" }, { name: "Grace" }]
names<hint label="Array<String>"> = rows
  .map { |row| row[:name] }
  .select { |name| name }
  .map { |name| name.upcase }
"#,
    )
    .await;
}

#[tokio::test]
async fn each_with_object_returns_the_proven_accumulator_shape() {
    check(
        r#"
values = [1, 2]
summary<hint label="{ count: Integer }"> = values.each_with_object({ count: 0 }) do |value, memo|
  memo[:count] = value
end
"#,
    )
    .await;
}

#[tokio::test]
async fn unsupported_accumulator_mutation_invalidates_the_whole_result() {
    check(
        r#"
key = dynamic_key
summary<hint label=": ?"> = [1, 2].each_with_object({ count: 0 }) do |value, memo|
  memo[key] = value
end
"#,
    )
    .await;
}

#[tokio::test]
async fn next_values_and_fallthrough_form_the_exhaustive_block_result() {
    check(
        r#"
strings<hint label="Array<String>"> = [1, 2].map do |value|
  next value.to_s if condition
  value.to_s
end

optional<hint label="Array<(NilClass | String)>"> = [1, 2].map do |value|
  value.to_s if condition
end
"#,
    )
    .await;
}

#[tokio::test]
async fn raising_paths_do_not_add_a_block_result_member() {
    check(
        r#"
strings<hint label="Array<String>"> = [1, 2].map do |value|
  raise "stop" if condition
  value.to_s
end
"#,
    )
    .await;
}

#[tokio::test]
async fn generic_rbs_block_signature_substitutes_the_block_result() {
    let mut editor = FakeEditor::new().await;
    let signature_uri = tower_lsp::lsp_types::Url::parse("file:///sig/transformer.rbs")
        .expect("the synthetic RBS signature URI must be valid");
    FileProcessor::default()
        .collect_rbs_facts(
            &signature_uri,
            r#"class Transformer
  def apply: [Input, Output] (Input value) { (Input) -> Output } -> Output
end
"#,
            editor.server(),
        )
        .expect("the synthetic generic block signature must enter the shared engine");
    editor
        .open_and_check_fixture(
            "consumer.rb",
            r#"result<hint label="String"> = Transformer.new.apply(1) { |value| value.to_s }
"#,
        )
        .await;
}

#[tokio::test]
async fn callable_signature_replacement_and_parse_failure_remove_stale_results() {
    let mut editor = FakeEditor::new().await;
    let signature_uri = tower_lsp::lsp_types::Url::parse("file:///sig/converter.rbs")
        .expect("the synthetic RBS signature URI must be valid");
    let processor = FileProcessor::default();
    processor
        .collect_rbs_facts(
            &signature_uri,
            r#"class Converter
  def apply: [Input, Output] (Input value) { (Input) -> Output } -> Output
end
"#,
            editor.server(),
        )
        .expect("the initial callable signature must enter the shared engine");
    editor
        .open_and_check_fixture(
            "converter_consumer.rb",
            r#"result<hint label="String"> = Converter.new.apply(1) { |value| value.to_s }
"#,
        )
        .await;

    processor
        .collect_rbs_facts(
            &signature_uri,
            r#"class Converter
  def apply: [Input, Output] (Input value) { (Input) -> Output } -> Array[Output]
end
"#,
            editor.server(),
        )
        .expect("the replacement callable signature must replace the previous fact");
    editor
        .set_and_check_fixture(
            "converter_consumer.rb",
            r#"result<hint label="Array<String>"> = Converter.new.apply(1) { |value| value.to_s } # refreshed
"#,
        )
        .await;

    assert!(
        processor
            .collect_rbs_facts(
                &signature_uri,
                "class Converter\n  def apply: [A, B, C, D, E, F, G, H, I] (A value) { (A) -> B } -> B\nend",
                editor.server()
            )
            .is_err(),
        "a parse failure must be reported after atomically clearing stale signature facts"
    );
    editor
        .set_and_check_fixture(
            "converter_consumer.rb",
            r#"result<hint label=": ?"> = Converter.new.apply(1) { |value| value.to_s } # invalidated
"#,
        )
        .await;
}

#[tokio::test]
async fn opening_the_consumer_before_its_signature_converges_after_reindex() {
    let mut editor = FakeEditor::new().await;
    editor
        .open_and_check_fixture(
            "late_signature_consumer.rb",
            r#"result<hint label=": ?"> = LateTransformer.new.apply(1) { |value| value.to_s }
"#,
        )
        .await;
    let signature_uri = tower_lsp::lsp_types::Url::parse("file:///sig/late_transformer.rbs")
        .expect("the synthetic RBS signature URI must be valid");
    FileProcessor::default()
        .collect_rbs_facts(
            &signature_uri,
            r#"class LateTransformer
  def apply: [Input, Output] (Input value) { (Input) -> Output } -> Output
end
"#,
            editor.server(),
        )
        .expect("the late callable signature must enter the shared engine");
    editor
        .set_and_check_fixture(
            "late_signature_consumer.rb",
            r#"result<hint label="String"> = LateTransformer.new.apply(1) { |value| value.to_s } # reindexed
"#,
        )
        .await;
}

#[tokio::test]
async fn repeated_seeded_file_orders_produce_one_canonical_result() {
    let signature = r#"class OrderedTransformer
  def apply: [Input, Output] (Input value) { (Input) -> Output } -> Array[Output]
end
"#;
    let mut seed = 0x6a09_e667_f3bc_c909_u64;
    for iteration in 0..8 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let signature_first = seed & 1 == 0;
        let mut editor = FakeEditor::new().await;
        let signature_uri = tower_lsp::lsp_types::Url::parse(&format!(
            "file:///sig/ordered_transformer_{iteration}.rbs"
        ))
        .expect("the seeded synthetic signature URI must be valid");
        let processor = FileProcessor::default();
        if signature_first {
            processor
                .collect_rbs_facts(&signature_uri, signature, editor.server())
                .expect("signature-first indexing must retain the callable contract");
            editor
                .open_and_check_fixture(
                    "ordered_consumer.rb",
                    r#"result<hint label="Array<String>"> = OrderedTransformer.new.apply(1) { |value| value.to_s }
"#,
                )
                .await;
        } else {
            editor
                .open_and_check_fixture(
                    "ordered_consumer.rb",
                    r#"result<hint label=": ?"> = OrderedTransformer.new.apply(1) { |value| value.to_s }
"#,
                )
                .await;
            processor
                .collect_rbs_facts(&signature_uri, signature, editor.server())
                .expect("consumer-first indexing must retain the late callable contract");
            editor
                .set_and_check_fixture(
                    "ordered_consumer.rb",
                    r#"result<hint label="Array<String>"> = OrderedTransformer.new.apply(1) { |value| value.to_s }
"#,
                )
                .await;
        }
    }
}

#[tokio::test]
async fn known_lambda_can_supply_a_collection_transform_block() {
    check(
        r#"
convert = ->(_value) { "converted" }
strings<hint label="Array<String>"> = [1, 2].map(&convert)
"#,
    )
    .await;
}

#[tokio::test]
async fn forwarded_block_keeps_the_collection_result_relationship() {
    check(
        r#"
def transform(values, &block)
  values.map(&block)
end

strings<hint label="Array<String>"> = transform([1, 2]) { |value| value.to_s }
"#,
    )
    .await;
}

#[tokio::test]
async fn direct_ruby_yield_uses_the_shared_callable_result_constraint() {
    check(
        r#"
def transform(value)
  yield(value)
end

result<hint label="Integer"> = transform(1) { |value| value }
"#,
    )
    .await;
}

#[tokio::test]
async fn editing_and_reopening_ruby_yield_evidence_replaces_the_call_result() {
    let mut editor = FakeEditor::new().await;
    editor
        .open_and_check_fixture(
            "yield_lifecycle.rb",
            r#"def transform(value)
  yield(value)
end

result<hint label="Integer"> = transform(1) { |value| value }
"#,
        )
        .await;
    editor
        .set_and_check_fixture(
            "yield_lifecycle.rb",
            r#"def transform(value)
  yield("changed")
end

result<hint label="String"> = transform(1) { |value| value }
"#,
        )
        .await;
    editor.close("yield_lifecycle.rb").await;
    editor
        .open_and_check_fixture(
            "yield_lifecycle.rb",
            r#"def transform(value)
  yield(value)
end

result<hint label="Integer"> = transform(1) { |value| value }
"#,
        )
        .await;
}

#[tokio::test]
async fn unresolved_element_member_invalidates_the_whole_transform() {
    check(
        r#"
values = [1, dynamic_value]
strings<hint label=": ?"> = values.map(&:to_s)
"#,
    )
    .await;
}

#[tokio::test]
async fn unsupported_non_local_block_exit_does_not_publish_an_array_guess() {
    check(
        r#"
result<hint label=": ?"> = [1, 2].map { |value| break value.to_s }
"#,
    )
    .await;
}

#[tokio::test]
async fn hover_completion_diagnostics_and_chained_dispatch_share_the_call_proof() {
    check(
        r#"
strings = [1, 2].map { |value| value.to_s }
strings<hover label="Array<String>">
strings.first<hover label="String">.upcase
strings.first.up$0
<complete items="upcase">
strings.first.<warn code="unresolved-method">missing_member</warn>
"#,
    )
    .await;
}

#[tokio::test]
async fn navigation_inside_a_typed_block_uses_the_ordinary_local_identity() {
    check(
        r#"
[1, 2].map { |<def>value</def>| value$0.to_s }
"#,
    )
    .await;
}
