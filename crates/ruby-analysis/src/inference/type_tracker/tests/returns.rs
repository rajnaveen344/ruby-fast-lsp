use super::*;

#[test]
fn direct_recursive_return_uses_the_least_proven_fixed_point() {
    let source = r#"def count_down(n)
  return 0 if n == 0
  count_down(n - 1)
end"#;
    let mut tracker = TypeTracker::new();
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

    let outcome = tracker.track_method_outcome(&def_node);

    assert_eq!(outcome.proven_type(), Some(&RubyType::integer()));
    assert_eq!(outcome.unknown_reason(), None);
}

#[test]
fn recursive_cycle_without_a_base_stays_explainable_unknown() {
    let source = "def forever\n  forever\nend";
    let mut tracker = TypeTracker::new();
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

    let outcome = tracker.track_method_outcome(&def_node);

    assert_eq!(outcome.proven_type(), None);
    assert_eq!(
        outcome.unknown_reason(),
        Some(UnknownReason::UnprovenRecursiveCycle)
    );
}

#[test]
fn mutual_return_equations_do_not_require_preinserted_method_facts() {
    let source = r#"def left
  right
end

def right
  left
end"#;
    let parse_result = ruby_prism::parse(source.as_bytes());
    let statements = parse_result.node().as_program_node().unwrap().statements();
    let left = FullyQualifiedName::method(
        Vec::new(),
        RubyMethod::new("left").expect("test method name must be valid"),
    );
    let right = FullyQualifiedName::method(
        Vec::new(),
        RubyMethod::new("right").expect("test method name must be valid"),
    );
    let mut equations = Vec::new();
    for (node, method) in statements.body().iter().zip([left.clone(), right.clone()]) {
        let definition = node
            .as_def_node()
            .expect("test statement must be a method definition");
        equations.push(TypeTracker::new().track_method_equation(
            &definition,
            method,
            Arc::new(HashSet::new()),
        ));
    }

    let solved = crate::inference::method::recursive::solve_method_return_equations(&equations);

    assert_eq!(
        solved
            .get(&left)
            .and_then(TypeInferenceOutcome::unknown_reason),
        Some(UnknownReason::UnprovenRecursiveCycle)
    );
    assert_eq!(
        solved
            .get(&right)
            .and_then(TypeInferenceOutcome::unknown_reason),
        Some(UnknownReason::UnprovenRecursiveCycle)
    );
}

#[test]
fn modeled_block_result_is_not_replaced_by_callee_return_dependency() {
    let source = r#"def label
  with_value { 1 }
end"#;
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
    let with_value = FullyQualifiedName::method(
        Vec::new(),
        RubyMethod::new("with_value").expect("test method name must be valid"),
    );
    let label = FullyQualifiedName::method(
        Vec::new(),
        RubyMethod::new("label").expect("test method name must be valid"),
    );
    let mut yield_types = HashMap::new();
    yield_types.insert(with_value.clone(), vec![RubyType::integer()]);
    let equation = TypeTracker::new()
        .with_yield_param_types(yield_types)
        .track_method_equation(&definition, label, Arc::new(HashSet::from([with_value])));

    assert_eq!(
        equation.immediate_outcome().proven_type(),
        Some(&RubyType::integer())
    );
}

#[test]
fn unresolved_recursive_base_stays_explainable_unknown() {
    let source = r#"def resolve(flag)
  return missing_value if flag
  resolve(flag)
end"#;
    let mut tracker = TypeTracker::new();
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

    let outcome = tracker.track_method_outcome(&def_node);

    assert_eq!(outcome.proven_type(), None);
    assert_eq!(
        outcome.unknown_reason(),
        Some(UnknownReason::UnprovenRecursiveCycle)
    );
}

#[test]
fn explicit_and_fallthrough_returns_are_both_inferred() {
    let source = r#"def value(flag)
  return 1 if flag
  "text"
end"#;
    let mut tracker = TypeTracker::new();
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

    assert_eq!(
        tracker.track_method(&def_node),
        RubyType::union([RubyType::integer(), RubyType::string()])
    );
}

#[test]
fn short_circuit_result_keeps_only_the_left_members_that_return() {
    let and_source = "def foo(flag)\n  flag && \"fallback\"\nend";
    let and_parse = ruby_prism::parse(and_source.as_bytes());
    let and_method = and_parse
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
    let mut and_tracker = TypeTracker::new()
        .with_parameter_types(HashMap::from([("flag".to_string(), RubyType::boolean())]));
    assert_eq!(
        and_tracker.track_method(&and_method),
        RubyType::union([RubyType::false_class(), RubyType::string()])
    );

    let or_source = "def foo(flag)\n  flag || \"fallback\"\nend";
    let or_parse = ruby_prism::parse(or_source.as_bytes());
    let or_method = or_parse
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
    let mut or_tracker = TypeTracker::new()
        .with_parameter_types(HashMap::from([("flag".to_string(), RubyType::boolean())]));
    assert_eq!(
        or_tracker.track_method(&or_method),
        RubyType::union([RubyType::string(), RubyType::true_class()])
    );
}
