use super::*;

#[test]
fn arrays_recursively_retain_local_shape_evidence() {
    let source = r#"def build
  label = "ready"
  [{ label: label }]
end"#;

    assert_eq!(
        tracked_method_type(source).to_string(),
        "Array<{ label: String }>"
    );
}

#[test]
fn array_element_read_preserves_the_contained_shape_identity() {
    let source = r#"def read
  child = { count: 1 }
  items = [child]
  extracted = items.first
  extracted[:count] = "many"
  child[:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::string());
}

#[test]
fn array_literal_index_read_preserves_the_contained_shape_identity() {
    let source = r#"def read
  child = { count: 1 }
  items = [child]
  extracted = items[0]
  extracted[:count] = "many"
  child[:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::string());
}

#[test]
fn array_at_negative_index_preserves_the_contained_shape_identity() {
    let source = r#"def read
  first = { count: 1 }
  last = { count: 2 }
  items = [first, last]
  extracted = items.at(-1)
  extracted[:count] = "many"
  last[:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::string());
}

#[test]
fn array_fetch_preserves_the_contained_shape_identity() {
    let source = r#"def read
  first = { count: 1 }
  second = { count: 2 }
  items = [first, second]
  extracted = items.fetch(1)
  extracted[:count] = "many"
  second[:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::string());
}

#[test]
fn dynamic_array_index_invalidates_possible_contained_shape_aliases() {
    let source = r#"def read(index)
  first = { count: 1 }
  second = { count: 2 }
  items = [first, second]
  extracted = items[index]
  extracted[:count] = "many"
  first[:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::Unknown);
}

#[test]
fn array_slice_read_invalidates_possible_contained_shape_aliases() {
    let source = r#"def read
  child = { count: 1 }
  items = [child]
  copy = items.first(1)
  extracted = copy.first
  extracted[:count] = "many"
  child[:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::Unknown);
}

#[test]
fn inline_shape_array_element_receives_a_stable_identity() {
    let source = r#"def read
  items = [{ count: 1 }]
  extracted = items.first
  extracted[:count] = "many"
  items.first[:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::string());
}

#[test]
fn local_array_alias_preserves_contained_shape_identities() {
    let source = r#"def read
  child = { count: 1 }
  items = [child]
  copy = items
  extracted = copy.last
  extracted[:count] = "many"
  child[:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::string());
}

#[test]
fn array_to_a_preserves_contained_shape_identities() {
    let source = r#"def read
  child = { count: 1 }
  items = [child]
  copy = items.to_a
  extracted = copy.first
  extracted[:count] = "many"
  child[:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::string());
}

#[test]
fn contained_shape_mutation_updates_the_array_element_projection() {
    let source = r#"def read
  child = { count: 1 }
  items = [child]
  child[:count] = "many"
  items
end"#;

    assert_eq!(
        tracked_method_type(source).to_string(),
        "Array<{ count: String }>"
    );
}

#[test]
fn contained_shape_escape_invalidates_the_array_element_projection() {
    let source = r#"def read
  child = { count: 1 }
  items = [child]
  dynamic_sink(child)
  items
end"#;

    assert_eq!(
        tracked_method_type(source),
        RubyType::Array(vec![RubyType::Unknown])
    );
}

#[test]
fn branch_invalidation_retains_only_the_array_constructor_and_unknown_reason() {
    let source = r#"def read(condition)
  child = { count: 1 }
  items = [child]
  if condition
    dynamic_sink(child)
  end
  items
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

    assert_eq!(
        tracker.track_method(&definition),
        RubyType::Array(vec![RubyType::Unknown])
    );
    let read = exact_local_read(&mut tracker, source, "items\nend");
    assert_eq!(read.ruby_type, RubyType::Array(vec![RubyType::Unknown]));
    assert_eq!(
        read.unknown_reason,
        Some(UnknownReason::MutableShapeInvalidated)
    );
}

#[test]
fn exceeding_the_array_positional_shape_bound_fails_closed() {
    let source = r#"def read
  child = { count: 1 }
  items = [child, child, child, child, child, child, child, child, child]
  items.first[:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::Unknown);
}

#[test]
fn escaping_an_array_invalidates_its_contained_shape_identities() {
    let source = r#"def read
  child = { count: 1 }
  items = [child]
  dynamic_sink(items)
  child[:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::Unknown);
}

#[test]
fn unsupported_array_mutation_invalidates_positional_shape_evidence() {
    let source = r#"def read
  first_child = { count: 1 }
  second_child = { count: "two" }
  items = [first_child, second_child]
  items.reverse!
  extracted = items.first
  extracted[:count] = true
  first_child[:count]
end"#;

    assert_eq!(tracked_method_type(source), RubyType::Unknown);
}
