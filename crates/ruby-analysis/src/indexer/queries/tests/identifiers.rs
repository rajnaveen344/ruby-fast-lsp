use super::*;

#[test]
fn index_call_reference_locations_exclude_arguments() {
    use ruby_prism::{CallNode, Visit};

    #[derive(Default)]
    struct Calls(Vec<(String, String)>);

    impl<'a> Visit<'a> for Calls {
        fn visit_call_node(&mut self, node: &CallNode<'a>) {
            if matches!(node.name().as_slice(), b"[]" | b"[]=") {
                let location =
                    call_reference_location(node).expect("index calls have a reference token");
                self.0.push((
                    utf8_str(node.name().as_slice()).to_string(),
                    utf8_str(location.as_slice()).to_string(),
                ));
            }
            ruby_prism::visit_call_node(self, node);
        }
    }

    let source = "table[key]\ntable[key] = value\ntable.[](key)\ntable.[]=(key, value)\ntable&.[](key)\ntable[]\ntable[key] { value }\n";
    let parsed = ruby_prism::parse(source.as_bytes());
    assert_eq!(parsed.errors().count(), 0, "the fixture must be valid Ruby");
    let mut calls = Calls::default();
    calls.visit(&parsed.node());
    assert_eq!(
        calls.0,
        [
            ("[]", "["),
            ("[]=", "["),
            ("[]", "[]"),
            ("[]=", "[]="),
            ("[]", "[]"),
            ("[]", "["),
            ("[]", "["),
        ]
        .map(|(method, token)| (method.to_string(), token.to_string()))
    );
}

#[test]
fn analyzer_position_adapter_uses_utf16_before_same_line_identifier() {
    let analyzer = create_analyzer("\"😀\"; value = 1; value\n");

    let (identifier, _, _, _, _) = analyzer.get_identifier_at_position(Position::new(0, 21));

    assert!(matches!(
        identifier,
        Some(Identifier::RubyLocalVariable { ref name, .. }) if name == "value"
    ));
}

#[test]
fn persisted_execution_context_changes_method_receiver_but_not_lexical_constants() {
    use crate::core::{
        ExecutionContextFact, ExecutionScopeMode, FullyQualifiedName, GeneratedOwnerId,
        NamespaceKind, SourceFileId, TextRange,
    };

    let source = "module Lexical\n  describe do\n    helper\n    VALUE\n  end\nend\n";
    let owner = FullyQualifiedName::namespace_with_kind(
        vec![RubyConstant::generated_owner(
            GeneratedOwnerId::new("rspec-ruby", "file:///dummy.rb", "group:1:2").unwrap(),
        )],
        NamespaceKind::Instance,
    );
    let analyzer = create_analyzer(source).with_execution_context(ExecutionContextFact {
        range: TextRange::new(SourceFileId(0), 26, 55),
        lexical_namespace: FullyQualifiedName::namespace(vec![
            RubyConstant::new("Lexical").unwrap()
        ]),
        implicit_receiver: owner.clone(),
        method_definition_owner: owner.clone(),
        lexical_scope: ExecutionScopeMode::Preserve,
        local_scope: ExecutionScopeMode::Preserve,
        extension_id: "rspec-ruby".to_string(),
    });

    let (method, _, _, _, _) = analyzer.get_identifier_at_position(Position::new(2, 5));
    let Identifier::RubyMethod { namespace, .. } = method.expect("helper must be identified")
    else {
        panic!("helper must be a method identifier")
    };
    assert_eq!(namespace, owner.namespace_parts());

    let (constant, _, lexical, _, _) = analyzer.get_identifier_at_position(Position::new(3, 6));
    assert!(matches!(constant, Some(Identifier::RubyConstant { .. })));
    assert_eq!(lexical, vec![RubyConstant::new("Lexical").unwrap()]);
}

#[test]
fn test_helper_functions_demonstration() {
    // Test constant helper
    let content = "Foo::Bar::BAZ";
    let analyzer = create_analyzer(content);
    let position = Position::new(0, 10); // Position at "BAZ"
    let (identifier_opt, ancestors, _) = get_identifier_for_test(&analyzer, position);
    let identifier = identifier_opt.expect("Expected identifier");

    // Demonstrate constant helper usage
    assert_constant_identifier(&identifier, &["Foo", "Bar", "BAZ"]);
    assert_namespace_context(&ancestors, &[]);

    // Test method helper (this would need a method call context)
    let method_content = r#"
class TestClass
  def test_method
    some_method
  end
end
"#;
    let method_analyzer = create_analyzer(method_content);
    let method_position = Position::new(3, 6); // Position at "some_method"
    let (method_identifier_opt, method_ancestors, _) =
        get_identifier_for_test(&method_analyzer, method_position);

    if let Some(method_identifier) = method_identifier_opt {
        // Demonstrate method helper usage
        assert_method_identifier(&method_identifier, "some_method", MethodReceiver::None);
        assert_namespace_context(&method_ancestors, &["TestClass"]);
    }

    // Test variable helper
    let variable_content = r#"
class TestClass
  def test_method
    local_var = 42
    local_var
  end
end
"#;
    let variable_analyzer = create_analyzer(variable_content);
    let variable_position = Position::new(4, 6); // Position at "local_var" usage
    let (variable_identifier_opt, _, _) =
        get_identifier_for_test(&variable_analyzer, variable_position);

    if let Some(variable_identifier) = variable_identifier_opt {
        // Demonstrate variable helper usage
        assert_variable_identifier(&variable_identifier, "local_var");
    }
}
