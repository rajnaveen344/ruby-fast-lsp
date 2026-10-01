use crate::core::SourcePosition as Position;
use crate::indexer::MethodReceiver;

use super::*;
use url::Url;

// Helper function to test the full visitor behavior
fn test_visitor(code: &str, position: Position, expected_parts: Vec<&str>) {
    let uri = Url::parse("file:///test.rb").unwrap();
    let document = RubyDocument::new(uri, code.to_string(), 1);
    let mut visitor = IdentifierVisitor::new(document, position);
    let parse_result = ruby_prism::parse(code.as_bytes());

    // Use the full visitor pattern
    visitor.visit(&parse_result.node());

    // If expected_parts is empty and we're on a scope resolution operator,
    // we expect identifier to be None
    if expected_parts.is_empty() {
        assert!(
            visitor.is_result_set(),
            "Expected identifier to be None at position {:?}",
            position
        );
        return;
    }

    // Otherwise, check the identifier was found
    assert!(
        visitor.is_result_set(),
        "Expected to find an identifier at position {:?}",
        position
    );

    // Get the identifier for further processing
    let identifier = visitor.identifier.as_ref().unwrap();

    // Special case for root constants
    if code.starts_with("::") {
        if let Identifier::RubyConstant { namespace: _, iden } = identifier {
            // For root constants, we expect the identifier path to match expected_parts
            assert_eq!(
                iden.len(),
                expected_parts.len(),
                "Identifier parts count mismatch for root constant path"
            );
            for (i, expected_part) in expected_parts.iter().enumerate() {
                assert_eq!(
                    iden[i].to_string(),
                    *expected_part,
                    "Identifier part at index {} mismatch",
                    i
                );
            }
            return;
        }
    }

    // Get the parts from the identifier - could be either a namespace or a constant
    let parts = match identifier {
        Identifier::RubyConstant { namespace: _, iden } => iden.clone(),
        // This line is no longer needed with the combined RubyConstant type
        _ => panic!("Expected a Namespace or Constant FQN"),
    };

    // Verify the parts match
    assert_eq!(
        parts.len(),
        expected_parts.len(),
        "Namespace parts count mismatch"
    );
    for (i, expected_part) in expected_parts.iter().enumerate() {
        assert_eq!(
            parts[i].to_string(),
            *expected_part,
            "Namespace part at index {} mismatch",
            i
        );
    }
}

#[test]
fn test_simple_constant_path() {
    // Test case: Foo::Bar with cursor at Bar
    test_visitor("Foo::Bar", Position::new(0, 6), vec!["Foo", "Bar"]);
}

#[test]
fn test_nested_constant_path_at_middle() {
    // Test case: Foo::Bar::Baz with cursor at Bar
    test_visitor("Foo::Bar::Baz", Position::new(0, 6), vec!["Foo", "Bar"]);
}

#[test]
fn test_nested_constant_path_at_first() {
    // Test case: Foo::Bar::Baz with cursor at Foo
    test_visitor("Foo::Bar::Baz", Position::new(0, 1), vec!["Foo"]);
}

#[test]
fn test_nested_constant_path_at_last() {
    // Test case: Foo::Bar::Baz with cursor at Baz
    test_visitor(
        "Foo::Bar::Baz",
        Position::new(0, 11),
        vec!["Foo", "Bar", "Baz"],
    );
}

#[test]
fn test_nested_constant_path_at_scope_resolution() {
    // Test case: Foo::Bar::Baz with cursor at first "::"
    // When cursor is at "::", we only get the constant path up to that point
    test_visitor("Foo::Bar::Baz", Position::new(0, 3), vec!["Foo"]);
}

#[test]
fn test_nested_constant_path_at_scope_resolution_2() {
    // Test case: Foo::Bar::Baz with cursor at second "::"
    // When cursor is at "::", we only get the constant path up to that point
    test_visitor("Foo::Bar::Baz", Position::new(0, 8), vec!["Foo", "Bar"]);
}

#[test]
fn test_root_constant_read_node() {
    test_visitor(
        "::GLOBAL_CONSTANT",
        Position::new(0, 2),
        vec!["GLOBAL_CONSTANT"],
    );
}

#[test]
fn test_root_constant_path_node() {
    test_visitor(
        "::Foo::Bar::GLOBAL_CONSTANT",
        Position::new(0, 12),
        vec!["Foo", "Bar", "GLOBAL_CONSTANT"],
    );
}

#[test]
fn test_constant_in_method_call() {
    // Test case: Foo.bar with cursor at Foo
    let uri = Url::parse("file:///test.rb").unwrap();
    let document = RubyDocument::new(uri, "Foo.bar".to_string(), 1);
    let mut visitor = IdentifierVisitor::new(document, Position::new(0, 1));
    let parse_result = ruby_prism::parse("Foo.bar".as_bytes());
    visitor.visit(&parse_result.node());

    // Ensure we found an identifier
    let identifier = visitor.identifier.expect("Expected to find an identifier");

    match identifier {
        Identifier::RubyConstant {
            namespace: _,
            iden: parts,
        } => {
            assert_eq!(parts.len(), 1);
            assert_eq!(parts[0].to_string(), "Foo");
        }
        _ => panic!("Expected Namespace FQN, got {:?}", identifier),
    }
}

#[test]
fn test_constant_path_in_method_call() {
    // Test case: Foo::Bar.baz with cursor at Bar
    let uri = Url::parse("file:///test.rb").unwrap();
    let document = RubyDocument::new(uri, "Foo::Bar.baz".to_string(), 1);
    let mut visitor = IdentifierVisitor::new(document, Position::new(0, 6));
    let parse_result = ruby_prism::parse("Foo::Bar.baz".as_bytes());
    visitor.visit(&parse_result.node());

    // Ensure we found an identifier
    let identifier = visitor.identifier.expect("Expected to find an identifier");

    match identifier {
        Identifier::RubyConstant {
            namespace: _,
            iden: parts,
        } => {
            assert_eq!(parts.len(), 2);
            assert_eq!(parts[0].to_string(), "Foo");
            assert_eq!(parts[1].to_string(), "Bar");
        }
        _ => panic!("Expected Namespace FQN, got {:?}", identifier),
    }
}

#[test]
fn test_module_method_call() {
    // Test case: Foo::Bar.baz with cursor at baz (module method call)
    let uri = Url::parse("file:///test.rb").unwrap();
    let document = RubyDocument::new(uri, "Foo::Bar.baz".to_string(), 1);
    let mut visitor = IdentifierVisitor::new(document, Position::new(0, 10));
    let parse_result = ruby_prism::parse("Foo::Bar.baz".as_bytes());
    visitor.visit(&parse_result.node());

    // Ensure we found an identifier
    let identifier = visitor.identifier.expect("Expected to find an identifier");

    match identifier {
        Identifier::RubyMethod {
            namespace: parts,
            receiver,
            iden: method,
        } => {
            // The namespace should be empty at top-level (no artificial prefix)
            assert_eq!(parts.len(), 0);
            // The receiver should be Constant with [Foo, Bar]
            match receiver {
                MethodReceiver::Constant(receiver_parts) => {
                    assert_eq!(receiver_parts.len(), 2);
                    assert_eq!(receiver_parts[0].to_string(), "Foo");
                    assert_eq!(receiver_parts[1].to_string(), "Bar");
                }
                _ => panic!("Expected Constant receiver"),
            }
            assert_eq!(method.to_string(), "baz");
        }
        _ => panic!("Expected Method identifier, got {:?}", identifier),
    }
}

#[test]
fn test_namespace_in_method_call() {
    // Test case: Foo::Bar::Baz.foo with cursor at Bar
    let mut visitor = {
        let uri = Url::parse("file:///test.rb").unwrap();
        let document = RubyDocument::new(uri, "Foo::Bar::Baz.foo".to_string(), 1);
        IdentifierVisitor::new(document, Position::new(0, 6))
    };
    let parse_result = ruby_prism::parse("Foo::Bar::Baz.foo".as_bytes());
    visitor.visit(&parse_result.node());

    // Ensure we found an identifier
    let identifier = visitor.identifier.expect("Expected to find an identifier");

    match identifier {
        Identifier::RubyConstant {
            namespace: _,
            iden: parts,
        } => {
            assert_eq!(parts.len(), 2);
            assert_eq!(parts[0].to_string(), "Foo");
            assert_eq!(parts[1].to_string(), "Bar");
        }
        _ => panic!("Expected Namespace FQN, got {:?}", identifier),
    }
}

#[test]
fn test_constant_in_nested_expression() {
    // Test case: Foo::Bar::Baz::ABC with cursor at ABC
    let mut visitor = {
        let uri = Url::parse("file:///test.rb").unwrap();
        let document = RubyDocument::new(uri, "Foo::Bar::Baz::ABC".to_string(), 1);
        IdentifierVisitor::new(document, Position::new(0, 15))
    };
    let parse_result = ruby_prism::parse("Foo::Bar::Baz::ABC".as_bytes());
    visitor.visit(&parse_result.node());

    // Ensure we found an identifier
    let identifier = visitor.identifier.expect("Expected to find an identifier");

    match identifier {
        Identifier::RubyConstant {
            namespace: _,
            iden: parts,
        } => {
            assert_eq!(parts.len(), 4);
            assert_eq!(parts[0].to_string(), "Foo");
            assert_eq!(parts[1].to_string(), "Bar");
            assert_eq!(parts[2].to_string(), "Baz");
            assert_eq!(parts[3].to_string(), "ABC");
        }
        _ => panic!("Expected Constant FQN, got {:?}", identifier),
    }
}

#[test]
fn test_constant_in_method_arguments() {
    // Test case: method(Foo::Bar) with cursor at Bar
    let mut visitor = {
        let uri = Url::parse("file:///test.rb").unwrap();
        let document = RubyDocument::new(uri, "method(Foo::Bar)".to_string(), 1);
        IdentifierVisitor::new(document, Position::new(0, 12))
    };
    let parse_result = ruby_prism::parse("method(Foo::Bar)".as_bytes());
    visitor.visit(&parse_result.node());

    // Ensure we found an identifier
    let identifier = visitor.identifier.expect("Expected to find an identifier");

    match identifier {
        Identifier::RubyConstant {
            namespace: _,
            iden: parts,
        } => {
            assert_eq!(parts.len(), 2);
            assert_eq!(parts[0].to_string(), "Foo");
            assert_eq!(parts[1].to_string(), "Bar");
        }
        _ => panic!("Expected Namespace FQN, got {:?}", identifier),
    }
}

#[test]
fn test_nested_constant_in_method_arguments() {
    // Test case: method(A::B::C::D::CONST) with cursor at CONST
    let uri = Url::parse("file:///test.rb").unwrap();
    let document = RubyDocument::new(uri, "method(A::B::C::D::CONST)".to_string(), 1);
    let mut visitor = IdentifierVisitor::new(document, Position::new(0, 20));
    let parse_result = ruby_prism::parse("method(A::B::C::D::CONST)".as_bytes());
    visitor.visit(&parse_result.node());

    // Ensure we found an identifier
    let identifier = visitor.identifier.expect("Expected to find an identifier");

    match identifier {
        Identifier::RubyConstant {
            namespace: _,
            iden: parts,
        } => {
            assert_eq!(parts.len(), 5);
            assert_eq!(parts[0].to_string(), "A");
            assert_eq!(parts[1].to_string(), "B");
            assert_eq!(parts[2].to_string(), "C");
            assert_eq!(parts[3].to_string(), "D");
            assert_eq!(parts[4].to_string(), "CONST");
        }
        _ => panic!("Expected Constant FQN, got {:?}", identifier),
    }
}

#[test]
fn test_deeply_nested_call_node() {
    // Test case: a.b.c.d.e with cursor at d
    let uri = Url::parse("file:///test.rb").unwrap();
    let document = RubyDocument::new(uri, "a.b.c.d.e".to_string(), 1);
    let mut visitor = IdentifierVisitor::new(document, Position::new(0, 0));
    let parse_result = ruby_prism::parse("a.b.c.d.e".as_bytes());
    visitor.visit(&parse_result.node());

    // Ensure we found an identifier
    let identifier = visitor.identifier.expect("Expected to find an identifier");

    match identifier {
        Identifier::RubyMethod { iden: method, .. } => {
            assert_eq!(method.to_string(), "a");
        }
        _ => panic!("Expected InstanceMethod FQN, got {:?}", identifier),
    }
}

#[test]
fn test_constant_in_error_raising() {
    // Test case: raise Error::Type.new(Error::Messages::CONSTANT, Error::Codes::CODE)
    // with cursor at CONSTANT
    let code = "raise Error::Type.new(Error::Messages::CONSTANT, Error::Codes::CODE)";
    let uri = Url::parse("file:///test.rb").unwrap();
    let document = RubyDocument::new(uri, code.to_string(), 1);
    let mut visitor = IdentifierVisitor::new(document, Position::new(0, 40));
    let parse_result = ruby_prism::parse(code.as_bytes());
    visitor.visit(&parse_result.node());

    // Ensure we found an identifier
    let identifier = visitor.identifier.expect("Expected to find an identifier");

    match identifier {
        Identifier::RubyConstant {
            namespace: _,
            iden: parts,
        } => {
            assert_eq!(parts.len(), 3);
            assert_eq!(parts[0].to_string(), "Error");
            assert_eq!(parts[1].to_string(), "Messages");
            assert_eq!(parts[2].to_string(), "CONSTANT");
        }
        _ => panic!("Expected Constant FQN, got {:?}", identifier),
    }
}

#[test]
fn test_complex_error_raising() {
    // Test case with complex nested constant paths in error raising:
    // raise RubyLSP::Core::Errors::ValidationError.new(
    //       RubyLSP::Core::Constants::ErrorMessages::INVALID_SYNTAX,
    //       RubyLSP::Core::Constants::ErrorCodes::PARSE_ERROR
    //     )
    let code = String::from("raise RubyLSP::Core::Errors::ValidationError.new(\n")
        + "          RubyLSP::Core::Constants::ErrorMessages::INVALID_SYNTAX,\n"
        + "          RubyLSP::Core::Constants::ErrorCodes::PARSE_ERROR\n"
        + "        )";

    // Test with cursor on INVALID_SYNTAX
    let uri = Url::parse("file:///test.rb").unwrap();
    let document = RubyDocument::new(uri, code.to_string(), 1);
    let mut visitor = IdentifierVisitor::new(document, Position::new(1, 60));
    let parse_result = ruby_prism::parse(code.as_bytes());
    visitor.visit(&parse_result.node());

    // Ensure we found an identifier
    let identifier = visitor.identifier.expect("Expected to find an identifier");

    match identifier {
        Identifier::RubyConstant {
            namespace: _,
            iden: parts,
        } => {
            assert_eq!(parts.len(), 5);
            assert_eq!(parts[0].to_string(), "RubyLSP");
            assert_eq!(parts[1].to_string(), "Core");
            assert_eq!(parts[2].to_string(), "Constants");
            assert_eq!(parts[3].to_string(), "ErrorMessages");
            assert_eq!(parts[4].to_string(), "INVALID_SYNTAX");
        }
        _ => panic!("Expected Constant FQN, got {:?}", identifier),
    }

    // Test with cursor on PARSE_ERROR
    let uri = Url::parse("file:///test.rb").unwrap();
    let document = RubyDocument::new(uri, code.to_string(), 1);
    let mut visitor = IdentifierVisitor::new(document, Position::new(2, 55));
    let parse_result = ruby_prism::parse(code.as_bytes());
    visitor.visit(&parse_result.node());

    // Ensure we found an identifier
    let identifier = visitor.identifier.expect("Expected to find an identifier");

    match identifier {
        Identifier::RubyConstant {
            namespace: _,
            iden: parts,
        } => {
            assert_eq!(parts.len(), 5);
            assert_eq!(parts[0].to_string(), "RubyLSP");
            assert_eq!(parts[1].to_string(), "Core");
            assert_eq!(parts[2].to_string(), "Constants");
            assert_eq!(parts[3].to_string(), "ErrorCodes");
            assert_eq!(parts[4].to_string(), "PARSE_ERROR");
        }
        _ => panic!("Expected Constant FQN, got {:?}", identifier),
    }
}

#[test]
fn test_constant_in_block() {
    // Test case with constant paths in a block:
    // items.each do |item|
    //   raise Error::Type.new(
    //     Error::Messages::INVALID_ITEM,
    //     Error::Codes::ITEM_ERROR
    //   )
    // end
    let code = String::from("items.each do |item|\n")
        + "  raise Error::InvalidItem.new(\n"
        + "    Error::Messages::INVALID_ITEM,\n"
        + "    Error::Codes::ITEM_ERROR\n"
        + "  )\n"
        + "end";

    // Test with cursor on INVALID_ITEM
    let uri = Url::parse("file:///test.rb").unwrap();
    let document = RubyDocument::new(uri, code.to_string(), 1);
    let mut visitor = IdentifierVisitor::new(document, Position::new(2, 25));
    let parse_result = ruby_prism::parse(code.as_bytes());
    visitor.visit(&parse_result.node());

    // Ensure we found an identifier
    let identifier = visitor.identifier.expect("Expected to find an identifier");

    match identifier {
        Identifier::RubyConstant {
            namespace: _,
            iden: parts,
        } => {
            assert_eq!(parts.len(), 3);
            assert_eq!(parts[0].to_string(), "Error");
            assert_eq!(parts[1].to_string(), "Messages");
            assert_eq!(parts[2].to_string(), "INVALID_ITEM");
        }
        _ => panic!("Expected Constant FQN, got {:?}", identifier),
    }

    // Test with cursor on ITEM_ERROR
    let uri = Url::parse("file:///test.rb").unwrap();
    let document = RubyDocument::new(uri, code.to_string(), 1);
    let mut visitor = IdentifierVisitor::new(document, Position::new(3, 20));
    let parse_result = ruby_prism::parse(code.as_bytes());
    visitor.visit(&parse_result.node());

    // Ensure we found an identifier
    let identifier = visitor.identifier.expect("Expected to find an identifier");

    match identifier {
        Identifier::RubyConstant {
            namespace: _,
            iden: parts,
        } => {
            assert_eq!(parts.len(), 3);
            assert_eq!(parts[0].to_string(), "Error");
            assert_eq!(parts[1].to_string(), "Codes");
            assert_eq!(parts[2].to_string(), "ITEM_ERROR");
        }
        _ => panic!("Expected Constant FQN, got {:?}", identifier),
    }
}

/// While a method name is being retyped the def has no valid name. The visitor
/// skips that def and must not leave its scope stacks unbalanced.
#[test]
fn def_without_a_name_does_not_unbalance_scopes() {
    let code = "class Item\n  def\n    @sku = sku\n  end\nend\n";
    for line in 0..5 {
        let uri = Url::parse("file:///test.rb").unwrap();
        let document = RubyDocument::new(uri, code.to_string(), 1);
        let mut visitor = IdentifierVisitor::new(document, Position::new(line, 1));
        visitor.visit(&ruby_prism::parse(code.as_bytes()).node());
    }
}
