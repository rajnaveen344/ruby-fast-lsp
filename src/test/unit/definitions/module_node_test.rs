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

    let expected_fqn = FullyQualifiedName::from(vec![
        RubyConstant::new("Object").unwrap(),
        RubyConstant::try_from("Foo").unwrap(),
    ]);

    let index_lock = visitor.index.lock();
    let defs = &index_lock.definitions;
    let entries = defs.get(&expected_fqn).expect("Foo module entry missing");

    assert_eq!(entries.len(), 1);
    assert!(matches!(entries[0].kind, EntryKind::Module { .. }));

    // After visitor completion the namespace stack must contain only Object
    assert_eq!(visitor.scope_tracker.get_ns_stack().len(), 1);
}

// ---------------------------------------------------------------------------
//  module_node – namespaced module via ConstantPathNode (e.g. A::B) with body
// ---------------------------------------------------------------------------
#[test]
fn module_node_namespaced_constant_path_with_body() {
    let code = "module A::B\n  def foo; end\nend";
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
        .expect("A::B module entry missing");

    assert_eq!(entries.len(), 1);
    assert!(matches!(entries[0].kind, EntryKind::Module { .. }));

    // Namespace stack should be reset to just Object after visiting
    assert_eq!(visitor.scope_tracker.get_ns_stack().len(), 1);
}

// ---------------------------------------------------------------------------
//  module_node – deep namespaced module A::B::C (recursive ConstantPathNode)
// ---------------------------------------------------------------------------
#[test]
fn module_node_deep_namespaced_constant_path() {
    let code = "module A::B::C; end";
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
        .expect("A::B::C module entry missing");

    assert_eq!(entries.len(), 1);
    assert!(matches!(entries[0].kind, EntryKind::Module { .. }));
    assert_eq!(visitor.scope_tracker.get_ns_stack().len(), 1);
}

// ---------------------------------------------------------------------------
//  module_node – empty body (Nil body branch)
// ---------------------------------------------------------------------------
#[test]
fn module_node_without_body() {
    let code = "module Foo; end";
    let visitor = visit_code(code);

    let expected_fqn = FullyQualifiedName::from(vec![
        RubyConstant::new("Object").unwrap(),
        RubyConstant::try_from("Foo").unwrap(),
    ]);

    let index_lock = visitor.index.lock();
    let entries = index_lock
        .definitions
        .get(&expected_fqn)
        .expect("Foo module entry missing");

    assert_eq!(entries.len(), 1);
    assert!(matches!(entries[0].kind, EntryKind::Module { .. }));
}

// ---------------------------------------------------------------------------
//  module_node – invalid constant name should panic (EntryBuilder error path)
// ---------------------------------------------------------------------------
#[test]
#[should_panic]
fn module_node_invalid_constant() {
    // lowercase module name is invalid and will cause unwrap panic
    let code = "module foo; end";
    let _visitor = visit_code(code);
}
