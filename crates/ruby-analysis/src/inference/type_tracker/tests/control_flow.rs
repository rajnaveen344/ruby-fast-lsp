use super::*;

#[test]
fn short_circuit_and_joins_executed_and_skipped_assignment_paths() {
    let source = r#"def foo(flag)
  value = 1
  flag && (value = "fallback")
  value
end"#;
    let parse_result = ruby_prism::parse(source.as_bytes());
    let def_node = parse_result
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
            tracker.track_method(&def_node),
            RubyType::union([RubyType::integer(), RubyType::string()]),
            "an unknown left operand makes both the skipped and executed right-operand states reachable"
        );
}

#[test]
fn short_circuit_or_joins_executed_and_skipped_assignment_paths() {
    let source = r#"def foo(flag)
  value = 1
  flag or (value = "fallback")
  value
end"#;
    let parse_result = ruby_prism::parse(source.as_bytes());
    let def_node = parse_result
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
            tracker.track_method(&def_node),
            RubyType::union([RubyType::integer(), RubyType::string()]),
            "an unknown left operand makes both the skipped and executed right-operand states reachable"
        );
}

#[test]
fn rescue_entry_joins_values_before_and_after_each_protected_assignment() {
    let source = r#"def foo
  value = Product.new
  begin
    value = Text.new
    dangerous
  rescue
    value
  end
end"#;
    let parse_result = ruby_prism::parse(source.as_bytes());
    let def_node = parse_result
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

    tracker.track_method(&def_node);

    assert_eq!(
        exact_local_read_type(&mut tracker, source, "value\n  end"),
        RubyType::union([instance_type("Product"), instance_type("Text")])
    );
}

#[test]
fn rescue_modifier_uses_the_same_assignment_prefix_join() {
    let source = r#"def foo
  value = Product.new
  ((value = Text.new; dangerous) rescue value)
end"#;
    let parse_result = ruby_prism::parse(source.as_bytes());
    let def_node = parse_result
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

    tracker.track_method(&def_node);

    assert_eq!(
        exact_local_read_type(&mut tracker, source, "value)"),
        RubyType::union([instance_type("Product"), instance_type("Text")])
    );
}

#[test]
fn unresolved_protected_assignment_absorbs_the_rescue_receiver_proof() {
    let source = r#"def foo
  value = Product.new
  begin
    value = dynamic_value
    dangerous
  rescue
    value
  end
end"#;
    let parse_result = ruby_prism::parse(source.as_bytes());
    let def_node = parse_result
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

    tracker.track_method(&def_node);

    assert_eq!(
        exact_local_read_type(&mut tracker, source, "value\n  end"),
        RubyType::Unknown,
        "one unproven assignment value must absorb every concrete rescue-entry alternative"
    );
}

#[test]
fn test_if_with_else() {
    let source = r#"def foo
  if true
    x = 5
  else
    x = "hello"
  end
  x
end"#;
    let mut tracker = TypeTracker::new();

    let parse_result = ruby_prism::parse(source.as_bytes());
    let root = parse_result.node();
    let program = root.as_program_node().unwrap();
    let stmts = program.statements();
    let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

    tracker.track_method(&def_node);
    let var_types = tracker.into_var_types();

    // After the if statement, x should be Integer | String
    let after_if = source.find("end\n  x").unwrap() + "end".len();
    let x_type = get_var_type_at(&var_types, after_if, "x");

    // Should be a union type containing both Integer and String
    assert!(x_type.is_some());
    let x_type = x_type.unwrap();
    assert!(matches!(x_type, RubyType::Union(_)));
}

#[test]
fn test_if_without_else() {
    let source = r#"def foo
  if true
    x = 5
  end
  x
end"#;
    let mut tracker = TypeTracker::new();

    let parse_result = ruby_prism::parse(source.as_bytes());
    let root = parse_result.node();
    let program = root.as_program_node().unwrap();
    let stmts = program.statements();
    let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

    tracker.track_method(&def_node);
    let var_types = tracker.into_var_types();

    // After the if statement, x should be Integer | NilClass (might not be defined)
    let after_if = source.find("end\n  x").unwrap() + "end".len();
    let x_type = get_var_type_at(&var_types, after_if, "x");

    // Should be a union type containing Integer and NilClass
    assert!(x_type.is_some());
    let x_type = x_type.unwrap();
    assert!(matches!(x_type, RubyType::Union(_)));
}

#[test]
fn test_unless_statement() {
    let source = r#"def foo
  unless false
    x = 5
  else
    x = "hello"
  end
  x
end"#;
    let mut tracker = TypeTracker::new();

    let parse_result = ruby_prism::parse(source.as_bytes());
    let root = parse_result.node();
    let program = root.as_program_node().unwrap();
    let stmts = program.statements();
    let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

    tracker.track_method(&def_node);
    let var_types = tracker.into_var_types();

    // After the unless statement, x should be Integer | String
    let after_unless = source.find("end\n  x").unwrap() + "end".len();
    let x_type = get_var_type_at(&var_types, after_unless, "x");

    assert!(x_type.is_some());
    let x_type = x_type.unwrap();
    assert!(matches!(x_type, RubyType::Union(_)));
}

#[test]
fn test_elsif_chain() {
    let source = r#"def foo
  if true
    x = 5
  elsif false
    x = "hello"
  else
    x = 3.14
  end
  x
end"#;
    let mut tracker = TypeTracker::new();

    let parse_result = ruby_prism::parse(source.as_bytes());
    let root = parse_result.node();
    let program = root.as_program_node().unwrap();
    let stmts = program.statements();
    let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

    tracker.track_method(&def_node);
    let var_types = tracker.into_var_types();

    // After the if/elsif/else, x should be a union of all three types
    let after_if = source.find("end\n  x").unwrap() + "end".len();
    let x_type = get_var_type_at(&var_types, after_if, "x");

    assert!(x_type.is_some());
    let x_type = x_type.unwrap();
    assert!(matches!(x_type, RubyType::Union(_)));
}

#[test]
fn test_case_with_else() {
    let source = r#"def foo
  case value
  when 1
    x = 5
  when 2
    x = "hello"
  else
    x = 3.14
  end
  x
end"#;
    let mut tracker = TypeTracker::new();

    let parse_result = ruby_prism::parse(source.as_bytes());
    let root = parse_result.node();
    let program = root.as_program_node().unwrap();
    let stmts = program.statements();
    let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

    tracker.track_method(&def_node);
    let var_types = tracker.into_var_types();

    // After the case statement, x should be Integer | String | Float
    let after_case = source.find("end\n  x").unwrap() + "end".len();
    let x_type = get_var_type_at(&var_types, after_case, "x");

    assert!(x_type.is_some());
    let x_type = x_type.unwrap();
    assert!(matches!(x_type, RubyType::Union(_)));
}

#[test]
fn test_case_without_else() {
    let source = r#"def foo
  case value
  when 1
    x = 5
  when 2
    x = "hello"
  end
  x
end"#;
    let mut tracker = TypeTracker::new();

    let parse_result = ruby_prism::parse(source.as_bytes());
    let root = parse_result.node();
    let program = root.as_program_node().unwrap();
    let stmts = program.statements();
    let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

    tracker.track_method(&def_node);
    let var_types = tracker.into_var_types();

    // After the case statement, x should be Integer | String | NilClass
    let after_case = source.find("end\n  x").unwrap() + "end".len();
    let x_type = get_var_type_at(&var_types, after_case, "x");

    assert!(x_type.is_some());
    let x_type = x_type.unwrap();
    assert!(matches!(x_type, RubyType::Union(_)));
}

#[test]
fn case_without_else_preserves_the_pre_case_binding_on_the_unmatched_path() {
    let source = r#"def choose(value)
  result = 1
  case value
  when :ready
    result = "ready"
  end
  result
end"#;
    let mut tracker = TypeTracker::new();
    let parse_result = ruby_prism::parse(source.as_bytes());
    let definition = parse_result
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

    let return_type = tracker.track_method(&definition);
    let var_types = tracker.into_var_types();
    let after_case = source.find("end\n  result").unwrap() + "end".len();
    let expected = RubyType::union([RubyType::integer(), RubyType::string()]);

    assert_eq!(return_type, expected);
    assert_eq!(
        get_var_type_at(&var_types, after_case, "result"),
        Some(expected)
    );
}

#[test]
fn case_without_else_includes_the_unmatched_nil_result() {
    let source = r#"def choose(value)
  case value
  when :ready
    "ready"
  end
end"#;
    let mut tracker = TypeTracker::new();
    let parse_result = ruby_prism::parse(source.as_bytes());
    let definition = parse_result
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

    assert_eq!(
        tracker.track_method(&definition),
        RubyType::union([RubyType::nil_class(), RubyType::string()]),
        "an unmatched ordinary case path must contribute Ruby's nil result"
    );
}

#[test]
fn pattern_case_without_else_keeps_only_reaching_branch_types() {
    let source = r#"def choose
  case { name: "Ada" }
  in { name: value }
    value
  end
  value
end"#;
    let mut tracker = TypeTracker::new();
    let parse_result = ruby_prism::parse(source.as_bytes());
    let definition = parse_result
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

    let return_type = tracker.track_method(&definition);
    let var_types = tracker.into_var_types();
    let after_case = source.find("end\n  value").unwrap() + "end".len();

    assert_eq!(return_type, RubyType::string());
    assert_eq!(
        get_var_type_at(&var_types, after_case, "value"),
        Some(RubyType::string()),
        "the unmatched pattern path raises, so it cannot contribute NilClass at the join"
    );
}

#[test]
fn pattern_case_explicit_else_keeps_nil_for_an_unbound_capture() {
    let source = r#"def choose
  case { name: "Ada" }
  in { name: value }
    value
  else
    nil
  end
  value
end"#;
    let mut tracker = TypeTracker::new();
    let parse_result = ruby_prism::parse(source.as_bytes());
    let definition = parse_result
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
    let expected = RubyType::union([RubyType::nil_class(), RubyType::string()]);

    let return_type = tracker.track_method(&definition);
    let var_types = tracker.into_var_types();
    let after_case = source.find("end\n  value").unwrap() + "end".len();

    assert_eq!(return_type, expected);
    assert_eq!(
        get_var_type_at(&var_types, after_case, "value"),
        Some(expected),
        "an explicit else reaches the join without binding the pattern capture"
    );
}

#[test]
fn pattern_case_branch_specific_capture_remains_nilable() {
    let source = r#"def choose
  case { name: "Ada" }
  in { name: value }
    value
  in { age: age }
    age
  end
  value
end"#;
    let mut tracker = TypeTracker::new();
    let parse_result = ruby_prism::parse(source.as_bytes());
    let definition = parse_result
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
    let expected = RubyType::union([RubyType::nil_class(), RubyType::string()]);

    let return_type = tracker.track_method(&definition);
    let var_types = tracker.into_var_types();
    let after_case = source.find("end\n  value").unwrap() + "end".len();

    assert_eq!(return_type, expected);
    assert_eq!(
        get_var_type_at(&var_types, after_case, "value"),
        Some(expected),
        "a different reachable in-branch may leave this capture unbound"
    );
}

#[test]
fn test_case_single_branch() {
    let source = r#"def foo
  case value
  when 1
    x = 5
  end
  x
end"#;
    let mut tracker = TypeTracker::new();

    let parse_result = ruby_prism::parse(source.as_bytes());
    let root = parse_result.node();
    let program = root.as_program_node().unwrap();
    let stmts = program.statements();
    let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

    tracker.track_method(&def_node);
    let var_types = tracker.into_var_types();

    // After the case statement, x should be Integer | NilClass
    let after_case = source.find("end\n  x").unwrap() + "end".len();
    let x_type = get_var_type_at(&var_types, after_case, "x");

    assert!(x_type.is_some());
    let x_type = x_type.unwrap();
    assert!(matches!(x_type, RubyType::Union(_)));
}

#[test]
fn test_while_loop() {
    let source = r#"def foo
  x = 0
  while true
    x = 5
  end
  x
end"#;
    let mut tracker = TypeTracker::new();

    let parse_result = ruby_prism::parse(source.as_bytes());
    let root = parse_result.node();
    let program = root.as_program_node().unwrap();
    let stmts = program.statements();
    let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

    tracker.track_method(&def_node);
    let var_types = tracker.into_var_types();

    // After the while loop, x should be Integer (0 or 5)
    let after_while = source.find("end\n  x").unwrap() + "end".len();
    let x_type = get_var_type_at(&var_types, after_while, "x");

    assert!(x_type.is_some());
    // Type should still be Integer (union of Integer | Integer = Integer)
}

#[test]
fn test_until_loop() {
    let source = r#"def foo
  x = 0
  until false
    x = "hello"
  end
  x
end"#;
    let mut tracker = TypeTracker::new();

    let parse_result = ruby_prism::parse(source.as_bytes());
    let root = parse_result.node();
    let program = root.as_program_node().unwrap();
    let stmts = program.statements();
    let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

    tracker.track_method(&def_node);
    let var_types = tracker.into_var_types();

    // After the until loop, x should be Integer | String
    let after_until = source.find("end\n  x").unwrap() + "end".len();
    let x_type = get_var_type_at(&var_types, after_until, "x");

    assert!(x_type.is_some());
    let x_type = x_type.unwrap();
    assert!(matches!(x_type, RubyType::Union(_)));
}

#[test]
fn nested_loop_stabilization_is_linear_not_exponential() {
    let source = r#"def parse
  value = 1
  while outer
    value
    until middle
      value
      while inner
        value
      end
    end
  end
end"#;
    let parse_result = ruby_prism::parse(source.as_bytes());
    let root = parse_result.node();
    let def_node = root
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
    tracker.control_flow.max_loop_iterations = 3;

    tracker.track_method(&def_node);

    // Ordinary local-read evidence records each actual traversal before
    // repeated loop visits are collapsed for publication.
    let mut visits = BTreeMap::new();
    for read in &tracker.observations.local_reads {
        assert_eq!(read.name, "value");
        assert_eq!(read.ruby_type, RubyType::integer());
        *visits.entry(read.start_offset).or_insert(0usize) += 1;
    }
    assert_eq!(visits.len(), 3, "each loop body must retain its read");
    assert!(
        visits.values().all(|count| *count == 3),
        "only the outer loop may multiply visits"
    );
    assert_eq!(
        tracker.take_local_read_types().len(),
        3,
        "publication must collapse repeated visits"
    );
}
