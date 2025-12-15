use crate::{
    indexer::entry::entry_kind::EntryKind,
    test::unit::definitions::visit_code,
    types::{fully_qualified_name::FullyQualifiedName, ruby_namespace::RubyConstant},
};

// ---------------------------------------------------------------------------
//  module_node – happy path (module has a body)
// ---------------------------------------------------------------------------
#[test]
fn module_node_with_body() {
    let code = r#"module Foo\n  def bar; end\nend\n"#;
    let visitor = visit_code(code);

    let expected_fqn = FullyQualifiedName::from(vec![RubyConstant::try_from("Foo").unwrap()]);

    let index_lock = visitor.index.lock();
    let entries = index_lock
        .get(&expected_fqn)
        .expect("Foo module entry missing");

    assert_eq!(entries.len(), 1);
    assert!(matches!(entries[0].kind, EntryKind::Module(_)));

    // After visitor completion the namespace stack must be empty (no artificial prefix)
    assert!(visitor.scope_tracker.get_ns_stack().is_empty());
}

// ---------------------------------------------------------------------------
//  module_node – namespaced module via ConstantPathNode (e.g. A::B) with body
// ---------------------------------------------------------------------------
#[test]
fn module_node_namespaced_constant_path_with_body() {
    let code = "module A::B\n  def foo; end\nend";
    let visitor = visit_code(code);

    let expected_fqn = FullyQualifiedName::from(vec![
        RubyConstant::try_from("A").unwrap(),
        RubyConstant::try_from("B").unwrap(),
    ]);

    let index_lock = visitor.index.lock();
    let entries = index_lock
        .get(&expected_fqn)
        .expect("A::B module entry missing");

    assert_eq!(entries.len(), 1);
    assert!(matches!(entries[0].kind, EntryKind::Module(_)));

    // Namespace stack should be empty after visiting (no artificial prefix)
    assert!(visitor.scope_tracker.get_ns_stack().is_empty());
}

// ---------------------------------------------------------------------------
//  module_node – deep namespaced module A::B::C (recursive ConstantPathNode)
// ---------------------------------------------------------------------------
#[test]
fn module_node_deep_namespaced_constant_path() {
    let code = "module A::B::C; end";
    let visitor = visit_code(code);

    let expected_fqn = FullyQualifiedName::from(vec![
        RubyConstant::try_from("A").unwrap(),
        RubyConstant::try_from("B").unwrap(),
        RubyConstant::try_from("C").unwrap(),
    ]);

    let index_lock = visitor.index.lock();
    let entries = index_lock
        .get(&expected_fqn)
        .expect("A::B::C module entry missing");

    assert_eq!(entries.len(), 1);
    assert!(matches!(entries[0].kind, EntryKind::Module(_)));
    assert!(visitor.scope_tracker.get_ns_stack().is_empty());
}

// ---------------------------------------------------------------------------
//  module_node – empty body (Nil body branch)
// ---------------------------------------------------------------------------
#[test]
fn module_node_without_body() {
    let code = "module Foo; end";
    let visitor = visit_code(code);

    let expected_fqn = FullyQualifiedName::from(vec![RubyConstant::try_from("Foo").unwrap()]);

    let index_lock = visitor.index.lock();
    let entries = index_lock
        .get(&expected_fqn)
        .expect("Foo module entry missing");

    assert_eq!(entries.len(), 1);
    assert!(matches!(entries[0].kind, EntryKind::Module(_)));
}

// ---------------------------------------------------------------------------
//  module_node – invalid constant name should be handled gracefully
// ---------------------------------------------------------------------------
#[test]
fn module_node_invalid_constant() {
    // lowercase module name is invalid and should be handled gracefully
    let code = "module foo; end";
    let visitor = visit_code(code);

    // The visitor should complete without panicking
    // No entries should be created for invalid module names
    let index_lock = visitor.index.lock();

    // Should not contain any entries for invalid module names
    assert!(
        index_lock.definitions_len() == 0
            || !index_lock
                .definitions()
                .any(|(fqn, _)| fqn.to_string().contains("foo"))
    );
}
