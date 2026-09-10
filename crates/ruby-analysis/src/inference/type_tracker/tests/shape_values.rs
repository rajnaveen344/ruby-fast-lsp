use super::*;

#[test]
fn local_hash_shape_recursively_uses_proven_local_values() {
    let source = r#"def build
  label = "ready"
  { payload: { label: label, count: 1 } }
end"#;

    assert_eq!(
        tracked_method_type(source).to_string(),
        "{ payload: { count: Integer, label: String } }"
    );
}

#[test]
fn if_join_preserves_correlated_shape_variants() {
    let source = r#"def build(condition)
  if condition
    { kind: :number, value: 1 }
  else
    { kind: :text, value: "ready" }
  end
end"#;

    assert_eq!(
        tracked_method_type(source).to_string(),
        "({ kind: :number, value: Integer } | { kind: :text, value: String })"
    );
}

#[test]
fn missing_shape_branch_contributes_implicit_nil() {
    let source = r#"def build(condition)
  if condition
    { state: :ready }
  end
end"#;

    assert_eq!(
        tracked_method_type(source).to_string(),
        "(NilClass | { state: :ready })"
    );
}

#[test]
fn missing_shape_branch_preserves_the_prior_value() {
    let source = r#"def build(condition)
  result = { state: :waiting, value: "cached" }
  if condition
    result = { state: :ready, value: 1 }
  end
  result
end"#;

    assert_eq!(
        tracked_method_type(source).to_string(),
        "({ state: :ready, value: Integer } | { state: :waiting, value: String })"
    );
}

#[test]
fn diverging_shape_branch_does_not_reach_join() {
    let source = r#"def build(condition)
  if condition
    { state: :ready }
  else
    raise "failed"
  end
end"#;

    assert_eq!(tracked_method_type(source).to_string(), "{ state: :ready }");
}

#[test]
fn unless_join_preserves_correlated_shape_variants() {
    let source = r#"def build(condition)
  unless condition
    { kind: :offline, value: "cached" }
  else
    { kind: :online, value: 1 }
  end
end"#;

    assert_eq!(
        tracked_method_type(source).to_string(),
        "({ kind: :offline, value: String } | { kind: :online, value: Integer })"
    );
}

#[test]
fn case_join_preserves_shape_variants_and_unmatched_nil() {
    let source = r#"def build(mode)
  case mode
  when :number
    { kind: :number, value: 1 }
  when :text
    { kind: :text, value: "ready" }
  end
end"#;

    assert_eq!(
        tracked_method_type(source).to_string(),
        "(NilClass | { kind: :number, value: Integer } | { kind: :text, value: String })"
    );
}

#[test]
fn local_shape_splat_uses_ruby_overwrite_order() {
    let source = r#"def build
  base = { state: :waiting, count: 1 }
  { before: true, **base, state: :ready }
end"#;

    assert_eq!(
        tracked_method_type(source).to_string(),
        "{ before: TrueClass, count: Integer, state: :ready }"
    );
}

#[test]
fn local_literal_key_read_uses_the_valid_shape_state() {
    let source = r#"def read
  payload = { count: 1 }
  payload[:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::integer());
}

#[test]
fn absent_and_dynamic_shape_reads_include_nil() {
    let absent = r#"def read
  payload = { count: 1 }
  payload[:missing]
end"#;
    assert_eq!(tracked_method_type(absent), RubyType::nil_class());

    let dynamic = r#"def read(key)
  payload = { count: 1, label: "ready" }
  payload[key]
end"#;
    assert_eq!(
        tracked_method_type(dynamic),
        RubyType::union([
            RubyType::integer(),
            RubyType::nil_class(),
            RubyType::string(),
        ])
    );
}

#[test]
fn fetch_and_dig_use_complete_shape_evidence() {
    let fetch_source = r#"def read
  payload = { count: 1 }
  payload.fetch(:missing, "fallback")
end"#;
    assert_eq!(tracked_method_type(fetch_source), RubyType::string());

    let dig_source = r#"def read
  payload = { user: { profile: { name: "Ada" } } }
  payload.dig(:user, :profile, :name)
end"#;
    assert_eq!(tracked_method_type(dig_source), RubyType::string());
}

#[test]
fn keys_values_and_each_project_the_current_shape() {
    let keys_source = r#"def read
  payload = { count: 1, label: "ready" }
  payload.keys
end"#;
    assert_eq!(
        tracked_method_type(keys_source),
        RubyType::Array(vec![RubyType::symbol()])
    );

    let values_source = r#"def read
  payload = { count: 1, label: "ready" }
  payload.values
end"#;
    assert_eq!(
        tracked_method_type(values_source),
        RubyType::Array(vec![RubyType::integer(), RubyType::string()])
    );

    let each_source = r#"def read
  payload = { count: 1 }
  payload.each
end"#;
    assert_eq!(tracked_method_type(each_source).to_string(), "Enumerator");

    let each_with_block_source = r#"def read
  payload = { count: 1 }
  payload.each { |_key, _value| nil }
end"#;
    assert_eq!(
        tracked_method_type(each_with_block_source).to_string(),
        "{ count: Integer }"
    );
}
