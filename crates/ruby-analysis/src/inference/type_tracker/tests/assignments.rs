use super::*;

#[test]
fn test_simple_method_tracking() {
    let source = "def foo\n  5\nend";
    let mut tracker = TypeTracker::new();

    let parse_result = ruby_prism::parse(source.as_bytes());
    let root = parse_result.node();
    let program = root.as_program_node().unwrap();
    let stmts = program.statements();
    let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

    let return_type = tracker.track_method(&def_node);

    assert_eq!(return_type, RubyType::integer());
}

#[test]
fn test_local_variable_assignment() {
    let source = "def foo\n  x = 5\n  x\nend";
    let mut tracker = TypeTracker::new();

    let parse_result = ruby_prism::parse(source.as_bytes());
    let root = parse_result.node();
    let program = root.as_program_node().unwrap();
    let stmts = program.statements();
    let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

    tracker.track_method(&def_node);
    let var_types = tracker.into_var_types();

    // Check that var_types were recorded
    assert!(!var_types.is_empty());

    // Find the assignment offset (after "x = 5")
    let assignment_end_offset = source.find("x = 5").unwrap() + "x = 5".len();

    // Query type after assignment
    let x_type = get_var_type_at(&var_types, assignment_end_offset, "x");
    assert_eq!(x_type, Some(RubyType::integer()));
}

#[test]
fn test_multiple_assignments() {
    let source = "def foo\n  x = 5\n  y = \"hello\"\n  x\nend";
    let mut tracker = TypeTracker::new();

    let parse_result = ruby_prism::parse(source.as_bytes());
    let root = parse_result.node();
    let program = root.as_program_node().unwrap();
    let stmts = program.statements();
    let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

    tracker.track_method(&def_node);
    let var_types = tracker.into_var_types();

    // Find offset after both assignments
    let second_assignment_end = source.find("y = \"hello\"").unwrap() + "y = \"hello\"".len();

    // Both variables should be in the environment
    let x_type = get_var_type_at(&var_types, second_assignment_end, "x");
    let y_type = get_var_type_at(&var_types, second_assignment_end, "y");

    assert_eq!(x_type, Some(RubyType::integer()));
    assert_eq!(y_type, Some(RubyType::string()));
}

#[test]
fn test_reassignment_changes_type() {
    let source = "def foo\n  x = 5\n  x = \"hello\"\n  x\nend";
    let mut tracker = TypeTracker::new();

    let parse_result = ruby_prism::parse(source.as_bytes());
    let root = parse_result.node();
    let program = root.as_program_node().unwrap();
    let stmts = program.statements();
    let def_node = stmts.body().iter().next().unwrap().as_def_node().unwrap();

    tracker.track_method(&def_node);
    let var_types = tracker.into_var_types();

    // After first assignment, should be Integer
    let first_assignment_end = source.find("x = 5").unwrap() + "x = 5".len();
    let x_type_1 = get_var_type_at(&var_types, first_assignment_end, "x");
    assert_eq!(x_type_1, Some(RubyType::integer()));

    // After second assignment, should be String
    let second_assignment_end = source.find("x = \"hello\"").unwrap() + "x = \"hello\"".len();
    let x_type_2 = get_var_type_at(&var_types, second_assignment_end, "x");
    assert_eq!(x_type_2, Some(RubyType::string()));
}
