use crate::{
    indexer::entry::entry_kind::EntryKind,
    test::unit::definitions::visit_code,
    types::{fully_qualified_name::FullyQualifiedName, ruby_namespace::RubyConstant},
};

// ---------------------------------------------------------------------------
//  class_node – happy path (class has a body)
// ---------------------------------------------------------------------------
#[test]
fn class_node_with_body() {
    let code = r#"class Foo\n  def bar; end\nend\n"#;
    let visitor = visit_code(code);

    // Build the expected fully-qualified name for `Foo`.
    let expected_fqn = FullyQualifiedName::from(vec![
        RubyConstant::new("Object").unwrap(),
        RubyConstant::try_from("Foo").unwrap(),
    ]);

    // The `RubyIndex` stores definitions keyed by FQN → Vec<Entry>.
    let index_lock = visitor.index.lock();
    let defs = &index_lock.definitions;
    let entries = defs.get(&expected_fqn).expect("Foo class entry missing");

    // Exactly one entry of kind `Class` should have been produced.
    assert_eq!(entries.len(), 1);
    assert!(matches!(entries[0].kind, EntryKind::Class { .. }));

    // Scope tracker – after visitor completion the namespace stack must be
    // empty (it is popped during `process_class_node_exit`).
    assert!(visitor.scope_tracker.get_ns_stack().len() == 1);
}

// ---------------------------------------------------------------------------
//  class_node – namespaced class via ConstantPathNode (e.g. A::B) with body
// ---------------------------------------------------------------------------
#[test]
fn class_node_namespaced_constant_path_with_body() {
    let code = "class A::B\n  def foo; end\nend";
    let visitor = visit_code(code);

    let expected_fqn = FullyQualifiedName::from(vec![
        RubyConstant::new("Object").unwrap(),
        RubyConstant::try_from("A").unwrap(),
        RubyConstant::try_from("B").unwrap(),
    ]);

    let index_lock = visitor.index.lock();
    let entries = index_lock
        .definitions
        .get(&expected_fqn)
        .expect("A::B class entry missing");

    assert_eq!(entries.len(), 1);
    assert!(matches!(entries[0].kind, EntryKind::Class { .. }));

    // After exit only top-level Object should remain in namespace stack
    assert_eq!(visitor.scope_tracker.get_ns_stack().len(), 1);
}

// ---------------------------------------------------------------------------
//  class_node – deep namespaced class A::B::C (recursive ConstantPathNode)
// ---------------------------------------------------------------------------
#[test]
fn class_node_deep_namespaced_constant_path() {
    let code = "class A::B::C; end";
    let visitor = visit_code(code);

    let expected_fqn = FullyQualifiedName::from(vec![
        RubyConstant::new("Object").unwrap(),
        RubyConstant::try_from("A").unwrap(),
        RubyConstant::try_from("B").unwrap(),
        RubyConstant::try_from("C").unwrap(),
    ]);

    let index_lock = visitor.index.lock();
    let entries = index_lock
        .definitions
        .get(&expected_fqn)
        .expect("A::B::C class entry missing");

    assert_eq!(entries.len(), 1);
    assert!(matches!(entries[0].kind, EntryKind::Class { .. }));
    assert_eq!(visitor.scope_tracker.get_ns_stack().len(), 1);
}

// ---------------------------------------------------------------------------
//  class_node – empty body (Nil body branch)
// ---------------------------------------------------------------------------
#[test]
fn class_node_without_body() {
    let code = "class Foo; end";
    let visitor = visit_code(code);

    let expected_fqn = FullyQualifiedName::from(vec![
        RubyConstant::new("Object").unwrap(),
        RubyConstant::try_from("Foo").unwrap(),
    ]);
    let index_lock = visitor.index.lock();

    let entries = index_lock
        .definitions
        .get(&expected_fqn)
        .expect("Foo class entry missing");
    assert_eq!(entries.len(), 1);
    assert!(matches!(entries[0].kind, EntryKind::Class { .. }));
}

// ---------------------------------------------------------------------------
//  class_node – invalid constant name should be handled gracefully
// ---------------------------------------------------------------------------
#[test]
fn class_node_invalid_constant() {
    // lowercase class name is invalid and should be handled gracefully
    let code = "class foo; end";
    let visitor = visit_code(code);
    
    // The visitor should complete without panicking
    // No entries should be created for invalid class names
    let index_lock = visitor.index.lock();
    let defs = &index_lock.definitions;
    
    // Should not contain any entries for invalid class names
    assert!(defs.is_empty() || !defs.iter().any(|(fqn, _)| fqn.to_string().contains("foo")));
}
