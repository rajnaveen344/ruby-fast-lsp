use super::*;

#[test]
fn key_presence_guard_narrows_complete_shape_variants() {
    let source = r#"def read(condition)
  payload = if condition
    { count: 1 }
  else
    { label: "ready" }
  end
  if payload.key?(:count)
    payload[:count]
  else
    payload[:label]
  end
end"#;

    assert_eq!(
        tracked_method_type(source),
        RubyType::union([RubyType::integer(), RubyType::string()])
    );
}

#[test]
fn literal_discriminator_narrows_correlated_shape_variants_on_both_paths() {
    let source = r#"def read(condition)
  result = if condition
    { kind: :number, value: 1 }
  else
    { kind: :text, value: "ready" }
  end
  if result[:kind] == :number
    result[:value]
  else
    result[:value]
  end
end"#;
    let parse = ruby_prism::parse(source.as_bytes());
    let definition = parse
        .node()
        .as_program_node()
        .expect("test source must parse as a program")
        .statements()
        .body()
        .iter()
        .next()
        .expect("test source must contain a method")
        .as_def_node()
        .expect("test source must begin with a method definition");
    let mut tracker = TypeTracker::new().with_local_read_types();
    tracker.track_method(&definition);
    let reads = tracker.take_local_read_types();
    let read_type_at = |needle: &str| {
        let start_offset = source.find(needle).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: discriminator test needle `{needle}` is absent. This is a bug because the fixture and assertion must identify the same branch read. Fix: keep the needle synchronized with the source."
                )
            });
        reads
                .iter()
                .find(|read| read.start_offset == start_offset)
                .unwrap_or_else(|| {
                    panic!(
                        "INVARIANT VIOLATED: discriminator branch read at {start_offset} was not retained. This is a bug because flow evidence is enabled after an if join. Fix: record the exact local receiver read on every reachable branch."
                    )
                })
                .ruby_type
                .clone()
    };

    assert_eq!(
        read_type_at("result[:value]\n  else").to_string(),
        "{ kind: :number, value: Integer }"
    );
    assert_eq!(
        read_type_at("result[:value]\n  end").to_string(),
        "{ kind: :text, value: String }"
    );
}

#[test]
fn reversed_inequality_discriminator_preserves_true_false_semantics() {
    let source = r#"def read(condition)
  result = if condition
    { kind: :number, value: 1 }
  else
    { kind: :text, value: "ready" }
  end
  if :number != result[:kind]
    result[:value]
  else
    result[:value]
  end
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
    let reads = tracker.take_local_read_types();
    let read_type_at = |needle: &str| {
        let start_offset = source.find(needle).unwrap();
        reads
            .iter()
            .find(|read| read.start_offset == start_offset)
            .unwrap()
            .ruby_type
            .clone()
    };

    assert_eq!(
        read_type_at("result[:value]\n  else").to_string(),
        "{ kind: :text, value: String }"
    );
    assert_eq!(
        read_type_at("result[:value]\n  end").to_string(),
        "{ kind: :number, value: Integer }"
    );
}

#[test]
fn optional_and_rest_discriminators_remain_on_both_inconclusive_paths() {
    let key = LiteralKey::symbol("kind");
    let number = LiteralValue::symbol("number");
    let optional = RubyType::Shape(Box::new(
        ShapeType::try_new(
            [ShapeField::optional(
                key.clone(),
                RubyType::Literal(Box::new(number.clone())),
            )],
            None,
            ShapeExactness::Open,
            ShapeStability::TrackedMutable,
        )
        .expect("test optional shape must satisfy canonical bounds"),
    ));
    let rest = RubyType::Shape(Box::new(
        ShapeType::try_new(
            [],
            Some(ShapeRest::new(RubyType::symbol(), RubyType::symbol())),
            ShapeExactness::Open,
            ShapeStability::TrackedMutable,
        )
        .expect("test rest shape must satisfy canonical bounds"),
    ));
    let variants = RubyType::union([optional.clone(), rest.clone()]);

    for require_match in [true, false] {
        assert_eq!(
            narrow_shape_literal_type(&variants, &key, &number, require_match),
            Ok(Some(variants.clone())),
            "an optional field or rest contract cannot prove either discriminator path"
        );
    }
}

#[test]
fn case_literal_discriminators_narrow_each_branch_and_the_else_path() {
    let source = r#"def read(condition)
  result = if condition
    { kind: :number, value: 1 }
  else
    { kind: :text, value: "ready" }
  end
  case result[:kind]
  when :number
    result[:value]
  when :text
    result[:value]
  else
    result
  end
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
    let reads = tracker.take_local_read_types();
    let read_type_at = |needle: &str| {
        let start_offset = source.find(needle).unwrap();
        reads
            .iter()
            .find(|read| read.start_offset == start_offset)
            .unwrap()
            .ruby_type
            .clone()
    };

    assert_eq!(
        read_type_at("result[:value]\n  when :text").to_string(),
        "{ kind: :number, value: Integer }"
    );
    assert_eq!(
        read_type_at("result[:value]\n  else").to_string(),
        "{ kind: :text, value: String }"
    );
}

#[test]
fn hash_patterns_narrow_variants_and_bind_correlated_field_types() {
    let source = r#"def read(condition)
  result = if condition
    { kind: :number, value: 1 }
  else
    { kind: :text, value: "ready" }
  end
  case result
  in { kind: :number, value: captured }
    captured
  in { kind: :text, value: captured }
    captured
  end
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
    let reads = tracker.take_local_read_types();
    let first = source.find("captured\n  in").unwrap();
    let second = source.rfind("captured\n  end").unwrap();
    assert_eq!(
        reads
            .iter()
            .find(|read| read.start_offset == first)
            .unwrap()
            .ruby_type,
        RubyType::integer()
    );
    assert_eq!(
        reads
            .iter()
            .find(|read| read.start_offset == second)
            .unwrap()
            .ruby_type,
        RubyType::string()
    );
}
