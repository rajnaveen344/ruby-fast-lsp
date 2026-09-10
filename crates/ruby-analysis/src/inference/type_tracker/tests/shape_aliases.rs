use super::*;

#[test]
fn keyed_reads_observe_alias_mutation_and_invalidation() {
    let mutated = r#"def read
  payload = { count: 1 }
  copy = payload
  copy[:count] = "many"
  payload[:count]
end"#;
    assert_eq!(tracked_method_type(mutated), RubyType::string());

    let invalidated = r#"def read
  payload = { count: 1 }
  dynamic_sink(payload)
  payload[:count]
end"#;
    assert_eq!(tracked_method_type(invalidated), RubyType::Unknown);
}

#[test]
fn known_alias_write_updates_every_live_shape_alias() {
    let source = r#"def build
  payload = { count: 1, state: :ready }
  copy = payload
  copy[:count] = "many"
  payload
end"#;

    let parse = ruby_prism::parse(source.as_bytes());
    let definition = parse
        .node()
        .as_program_node()
        .unwrap()
        .statements()
        .body()
        .iter()
        .next()
        .unwrap()
        .as_def_node()
        .unwrap();
    let mut tracker = TypeTracker::new();
    assert_eq!(
        tracker.track_method(&definition).to_string(),
        "{ count: String, state: :ready }"
    );
    assert_eq!(tracker.max_live_shape_aliases(), 2);
}

#[test]
fn nested_shape_read_alias_updates_the_parent_field() {
    let source = r#"def read
  payload = { nested: { count: 1 } }
  nested = payload[:nested]
  nested[:count] = "many"
  payload[:nested][:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::string());
}

#[test]
fn hash_literal_containing_a_shape_alias_observes_child_mutation() {
    let source = r#"def read
  child = { count: 1 }
  payload = { nested: child }
  child[:count] = "many"
  payload[:nested][:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::string());
}

#[test]
fn replacing_a_parent_field_detaches_the_old_child_identity() {
    let source = r#"def read
  payload = { nested: { count: 1 } }
  old_child = payload[:nested]
  payload[:nested] = { count: 2 }
  old_child[:count] = "detached"
  payload[:nested][:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::integer());
}

#[test]
fn frozen_outer_shape_observes_its_mutable_child_identity() {
    let source = r#"def read
  payload = { nested: { count: 1 } }
  payload.freeze
  child = payload[:nested]
  child[:count] = "many"
  payload[:nested][:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::string());
}

#[test]
fn repeated_nested_reads_share_one_child_identity() {
    let source = r#"def read
  payload = { nested: { count: 1 } }
  first = payload[:nested]
  second = payload[:nested]
  second[:count] = "many"
  first[:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::string());
}

#[test]
fn fetch_of_a_nested_shape_retains_the_child_identity() {
    let source = r#"def read
  payload = { nested: { count: 1 } }
  child = payload.fetch(:nested)
  child[:count] = "many"
  payload[:nested][:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::string());
}

#[test]
fn dig_of_a_nested_shape_retains_every_containment_edge() {
    let source = r#"def read
  payload = { outer: { nested: { count: 1 } } }
  child = payload.dig(:outer, :nested)
  child[:count] = "many"
  payload[:outer][:nested][:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::string());
}

#[test]
fn escaping_a_nested_child_invalidates_the_parent_shape_proof() {
    let source = r#"def read
  payload = { nested: { count: 1 } }
  child = payload[:nested]
  dynamic_sink(child)
  payload[:nested][:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::Unknown);
}

#[test]
fn one_child_identity_updates_every_containing_parent() {
    let source = r#"def read
  child = { count: 1 }
  first = { nested: child }
  second = { nested: child }
  child[:count] = "many"
  [first[:nested][:count], second[:nested][:count]]
end"#;

    assert_eq!(
        tracked_method_type(source),
        RubyType::Array(vec![RubyType::string()])
    );
}

#[test]
fn nested_child_mutation_preserves_variant_correlations_through_parent_updates() {
    let source = r#"def read(condition)
  child = { count: 1, state: :ready }
  payload = { nested: child }
  if condition
    child[:state] = :left
  else
    child[:state] = :right
  end
  child[:count] = "many"
  payload[:nested][:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::string());
}

#[test]
fn assigning_a_shape_alias_into_a_parent_field_tracks_containment() {
    let source = r#"def read
  payload = { state: :empty }
  child = { count: 1 }
  payload[:nested] = child
  child[:count] = "many"
  payload[:nested][:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::string());
}

#[test]
fn hash_to_h_preserves_the_shape_identity() {
    let source = r#"def read
  payload = { count: 1 }
  copy = payload.to_h
  copy[:count] = "many"
  payload[:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::string());
}

#[test]
fn non_mutating_merge_shares_nested_child_identities() {
    let source = r#"def read
  child = { count: 1 }
  payload = { nested: child }
  combined = payload.merge({ state: :ready })
  extracted = combined[:nested]
  extracted[:count] = "many"
  payload[:nested][:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::string());
}

#[test]
fn merge_bang_with_a_literal_links_nested_child_identities() {
    let source = r#"def read
  child = { count: 1 }
  payload = { state: :empty }
  payload.merge!({ nested: child })
  child[:count] = "many"
  payload[:nested][:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::string());
}

#[test]
fn merge_bang_with_a_shape_alias_links_nested_child_identities() {
    let source = r#"def read
  child = { count: 1 }
  addition = { nested: child }
  payload = { state: :empty }
  payload.merge!(addition)
  child[:count] = "many"
  payload[:nested][:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::string());
}

#[test]
fn hash_splat_shares_nested_child_identities() {
    let source = r#"def read
  child = { count: 1 }
  payload = { nested: child }
  copy = { **payload }
  extracted = copy[:nested]
  extracted[:count] = "many"
  payload[:nested][:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::string());
}

#[test]
fn branch_local_alias_write_joins_complete_shape_states() {
    let source = r#"def build(condition)
  payload = { count: 1, state: :ready }
  copy = payload
  if condition
    copy[:count] = "many"
  end
  payload
end"#;

    assert_eq!(
        tracked_method_type(source).to_string(),
        "({ count: Integer, state: :ready } | { count: String, state: :ready })"
    );
}

#[test]
fn known_delete_clear_and_merge_bang_transform_the_shared_shape() {
    let delete_source = r#"def build
  payload = { count: 1, state: :ready }
  copy = payload
  copy.delete(:count)
  payload
end"#;
    assert_eq!(
        tracked_method_type(delete_source).to_string(),
        "{ state: :ready }"
    );

    let clear_source = r#"def build
  payload = { count: 1 }
  payload.clear
  payload
end"#;
    assert_eq!(tracked_method_type(clear_source).to_string(), "{ }");

    let merge_source = r#"def build
  payload = { count: 1, state: :waiting }
  copy = payload
  copy.merge!({ state: :ready, label: "done" })
  payload
end"#;
    assert_eq!(
        tracked_method_type(merge_source).to_string(),
        "{ count: Integer, label: String, state: :ready }"
    );
}

#[test]
fn non_mutating_merge_creates_an_independent_shape_identity() {
    let source = r#"def build
  payload = { count: 1 }
  combined = payload.merge({ label: "done" })
  combined[:count] = "many"
  payload
end"#;

    assert_eq!(
        tracked_method_type(source).to_string(),
        "{ count: Integer }"
    );
}

#[test]
fn unresolved_argument_escape_invalidates_every_shape_alias() {
    let source = r#"def build
  payload = { count: 1 }
  copy = payload
  dynamic_sink(copy)
  payload
end"#;

    assert_eq!(tracked_method_type(source), RubyType::Unknown);
}

#[test]
fn unsupported_receiver_mutation_invalidates_every_shape_alias() {
    let source = r#"def build
  payload = { count: 1 }
  copy = payload
  copy.transform_values! { |value| value.to_s }
  payload
end"#;

    assert_eq!(tracked_method_type(source), RubyType::Unknown);
}

#[test]
fn freeze_preserves_only_the_outer_shape_key_stability() {
    let source = r#"def build
  payload = { nested: { count: 1 } }
  copy = payload
  copy.freeze
  payload
end"#;

    assert_eq!(
        tracked_method_type(source).to_string(),
        "frozen { nested: { count: Integer } }"
    );
}

#[test]
fn mutable_escape_retains_the_machine_readable_unknown_reason() {
    let source = r#"def build
  payload = { count: 1 }
  copy = payload
  dynamic_sink(copy)
  payload
end"#;
    let parse = ruby_prism::parse(source.as_bytes());
    let definition = parse
        .node()
        .as_program_node()
        .unwrap()
        .statements()
        .body()
        .iter()
        .next()
        .unwrap()
        .as_def_node()
        .unwrap();
    let mut tracker = TypeTracker::new().with_local_read_types();

    tracker.track_method(&definition);
    let read = exact_local_read(&mut tracker, source, "payload\nend");

    assert_eq!(read.ruby_type, RubyType::Unknown);
    assert_eq!(
        read.unknown_reason,
        Some(UnknownReason::MutableShapeInvalidated)
    );
}

#[test]
fn nonlocal_storage_invalidates_the_local_shape_identity() {
    for write in [
        "@stored = payload",
        "@@stored = payload",
        "$stored = payload",
        "STORED = payload",
        "Container::STORED = payload",
    ] {
        let source = format!("def build\n  payload = {{ count: 1 }}\n  {write}\n  payload\nend");
        assert_eq!(
            tracked_method_type(&source),
            RubyType::Unknown,
            "nonlocal write `{write}` must invalidate the escaped mutable identity"
        );
    }
}

#[test]
fn exceeding_the_fixed_alias_bound_fails_closed() {
    let source = r#"def build
  original = { count: 1 }
  alias_1 = original
  alias_2 = original
  alias_3 = original
  alias_4 = original
  alias_5 = original
  alias_6 = original
  alias_7 = original
  alias_8 = original
  original
end"#;

    assert_eq!(tracked_method_type(source), RubyType::Unknown);
}

#[test]
fn rebinding_an_alias_releases_it_from_the_identity_bound() {
    let source = r#"def build
  original = { count: 1 }
  alias_1 = original
  alias_2 = original
  alias_3 = original
  alias_4 = original
  alias_5 = original
  alias_6 = original
  alias_7 = original
  alias_1 = 1
  alias_8 = original
  original
end"#;

    assert_eq!(
        tracked_method_type(source).to_string(),
        "{ count: Integer }"
    );
}
