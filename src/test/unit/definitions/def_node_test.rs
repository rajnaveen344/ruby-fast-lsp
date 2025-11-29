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
    let fqn = FullyQualifiedName::method(vec![RubyConstant::try_from("Foo").unwrap()], method);

    let defs = &visitor.index.lock().definitions;
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
    let fqn = FullyQualifiedName::method(vec![RubyConstant::try_from("Foo").unwrap()], method);

    let defs = &visitor.index.lock().definitions;
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
    let fqn = FullyQualifiedName::method(vec![RubyConstant::try_from("Foo").unwrap()], method);

    let defs = &visitor.index.lock().definitions;
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
    let fqn = FullyQualifiedName::method(vec![RubyConstant::try_from("Foo").unwrap()], method);

    let defs = &visitor.index.lock().definitions;
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
    let fqn = FullyQualifiedName::method(vec![RubyConstant::try_from("Foo").unwrap()], method);
    let defs = &visitor.index.lock().definitions;
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
            RubyConstant::try_from("A").unwrap(),
            RubyConstant::try_from("B").unwrap(),
        ],
        method,
    );
    let defs = &visitor.index.lock().definitions;
    assert!(defs.get(&fqn).is_none());
}

// ---------------------------------------------------------------------------
//  def_node – invalid method name should be skipped (no entry)
// ---------------------------------------------------------------------------
#[test]
fn def_node_invalid_method_name() {
    let code = "def 123invalid; end"; // starts with number, truly invalid
    let visitor = visit_code(code);

    // Index should have no method entries for 123invalid
    let defs = &visitor.index.lock().definitions;
    // Ensure none of the FQNs contain 123invalid
    assert!(defs
        .keys()
        .all(|fqn| !fqn.to_string().contains("123invalid")));
}

// ---------------------------------------------------------------------------
//  def_node – uppercase method names should be supported (though unconventional)
// ---------------------------------------------------------------------------
#[test]
fn def_node_uppercase_method_name() {
    let code = "class Foo\n  def UppercaseMethod; end\nend";
    let visitor = visit_code(code);

    let method = RubyMethod::new("UppercaseMethod", MethodKind::Instance).unwrap();
    let fqn = FullyQualifiedName::method(vec![RubyConstant::try_from("Foo").unwrap()], method);

    let defs = &visitor.index.lock().definitions;
    let entries = defs.get(&fqn).expect("UppercaseMethod entry missing");
    assert_eq!(entries.len(), 1);
    assert!(matches!(entries[0].kind, EntryKind::Method { .. }));
}

// ---------------------------------------------------------------------------
//  def_node – Ruby special method names should be supported
// ---------------------------------------------------------------------------
#[test]
fn def_node_special_method_names() {
    let code = r#"
        class TestClass
          def []
            "array access"
          end

          def ^
            "xor operator"
          end

          def ==
            "equality operator"
          end

          def +@
            "unary plus"
          end

          def -@
            "unary minus"
          end

          def <=>
            "spaceship operator"
          end

          def []=(index, value)
            "array assignment"
          end
        end
    "#;
    let visitor = visit_code(code);

    let index_lock = visitor.index.lock();
    let defs = &index_lock.definitions;

    // Check that all special methods are indexed
    let special_methods = ["[]", "^", "==", "+@", "-@", "<=>", "[]="];

    for method_name in special_methods {
        let found = defs.iter().any(|(fqn, _)| {
            fqn.to_string()
                .contains(&format!("TestClass#{}", method_name))
        });
        assert!(found, "Special method '{}' should be indexed", method_name);
    }
}
