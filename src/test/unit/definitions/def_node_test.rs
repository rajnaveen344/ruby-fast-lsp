use crate::indexer::entry::MethodKind;
use crate::{
    indexer::entry::entry_kind::EntryKind,
    test::unit::definitions::visit_code,
    types::{
        fully_qualified_name::FullyQualifiedName, ruby_method::RubyMethod,
        ruby_namespace::RubyConstant,
    },
};

// ---------------------------------------------------------------------------
//  def_node – instance method inside class Foo
// ---------------------------------------------------------------------------
#[test]
fn def_node_instance_method() {
    let code = "class Foo\n  def bar; end\nend";
    let visitor = visit_code(code);

    let method = RubyMethod::new("bar", MethodKind::Instance).unwrap();
    let fqn = FullyQualifiedName::method(
        vec![
            RubyConstant::new("Object").unwrap(),
            RubyConstant::try_from("Foo").unwrap(),
        ],
        method,
    );

    let defs = &visitor.index.lock().unwrap().definitions;
    let entries = defs.get(&fqn).expect("bar method entry missing");
    assert_eq!(entries.len(), 1);
    assert!(matches!(entries[0].kind, EntryKind::Method { .. }));
}

// ---------------------------------------------------------------------------
//  def_node – class method via self receiver
// ---------------------------------------------------------------------------
#[test]
fn def_node_class_method_self() {
    let code = "class Foo\n  def self.baz; end\nend";
    let visitor = visit_code(code);

    let method = RubyMethod::new("baz", MethodKind::Class).unwrap();
    let fqn = FullyQualifiedName::method(
        vec![
            RubyConstant::new("Object").unwrap(),
            RubyConstant::try_from("Foo").unwrap(),
        ],
        method,
    );

    let defs = &visitor.index.lock().unwrap().definitions;
    let entries = defs.get(&fqn).expect("baz method entry missing");
    assert_eq!(entries.len(), 1);
    assert!(matches!(entries[0].kind, EntryKind::Method { .. }));
}

// ---------------------------------------------------------------------------
//  def_node – initialize mapped to new class method
// ---------------------------------------------------------------------------
#[test]
fn def_node_initialize_as_new() {
    let code = "class Foo\n  def initialize; end\nend";
    let visitor = visit_code(code);

    let method = RubyMethod::new("new", MethodKind::Class).unwrap();
    let fqn = FullyQualifiedName::method(
        vec![
            RubyConstant::new("Object").unwrap(),
            RubyConstant::try_from("Foo").unwrap(),
        ],
        method,
    );

    let defs = &visitor.index.lock().unwrap().definitions;
    let entries = defs.get(&fqn).expect("new method entry missing");
    assert_eq!(entries.len(), 1);
    assert!(matches!(entries[0].kind, EntryKind::Method { .. }));
}

// ---------------------------------------------------------------------------
//  def_node – method inside singleton class (class << self)
// ---------------------------------------------------------------------------
#[test]
fn def_node_singleton_class_method() {
    let code = "class Foo\n  class << self\n    def qux; end\n  end\nend";
    let visitor = visit_code(code);

    let method = RubyMethod::new("qux", MethodKind::Class).unwrap();
    let fqn = FullyQualifiedName::method(
        vec![
            RubyConstant::new("Object").unwrap(),
            RubyConstant::try_from("Foo").unwrap(),
        ],
        method,
    );

    let defs = &visitor.index.lock().unwrap().definitions;
    let entries = defs.get(&fqn).expect("qux method entry missing");
    assert_eq!(entries.len(), 1);
    assert!(matches!(entries[0].kind, EntryKind::Method { .. }));
}

// ---------------------------------------------------------------------------
//  def_node – class method with constant receiver (Foo.bar) – should be skipped
// ---------------------------------------------------------------------------
#[test]
fn def_node_constant_receiver_class_method() {
    let code = "def Foo.bar; end";
    let visitor = visit_code(code);

    let method = RubyMethod::new("bar", MethodKind::Class).unwrap();
    let fqn = FullyQualifiedName::method(
        vec![
            RubyConstant::new("Object").unwrap(),
            RubyConstant::try_from("Foo").unwrap(),
        ],
        method,
    );
    let defs = &visitor.index.lock().unwrap().definitions;
    assert!(defs.get(&fqn).is_none());
}

// ---------------------------------------------------------------------------
//  def_node – class method with namespaced constant receiver (A::B.baz) – should be skipped
// ---------------------------------------------------------------------------
#[test]
fn def_node_namespaced_constant_receiver_class_method() {
    let code = "def A::B.baz; end";
    let visitor = visit_code(code);

    let method = RubyMethod::new("baz", MethodKind::Class).unwrap();
    let fqn = FullyQualifiedName::method(
        vec![
            RubyConstant::new("Object").unwrap(),
            RubyConstant::try_from("A").unwrap(),
            RubyConstant::try_from("B").unwrap(),
        ],
        method,
    );
    let defs = &visitor.index.lock().unwrap().definitions;
    assert!(defs.get(&fqn).is_none());
}

// ---------------------------------------------------------------------------
//  def_node – invalid method name should be skipped (no entry)
// ---------------------------------------------------------------------------
#[test]
fn def_node_invalid_method_name() {
    let code = "def InvalidName; end"; // starts with uppercase, invalid
    let visitor = visit_code(code);

    // Index should have no method entries for InvalidName
    let defs = &visitor.index.lock().unwrap().definitions;
    // Ensure none of the FQNs contain InvalidName
    assert!(defs
        .keys()
        .into_iter()
        .all(|fqn| !fqn.to_string().contains("InvalidName")));
}
