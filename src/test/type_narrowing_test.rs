//! Integration tests for CFG-based type narrowing.

#[cfg(test)]
mod type_narrowing_tests {
    use tower_lsp::lsp_types::Url;

    use crate::type_inference::TypeNarrowingEngine;

    fn create_engine() -> TypeNarrowingEngine {
        TypeNarrowingEngine::new()
    }

    fn test_uri() -> Url {
        Url::parse("file:///test.rb").unwrap()
    }

    #[test]
    fn test_simple_method_analysis() {
        let engine = create_engine();
        let uri = test_uri();

        let source = r#"
def foo
  x = 1
  y = "hello"
end
"#;

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        let methods = engine.get_method_cfgs(&uri);
        assert_eq!(methods.len(), 1);
        assert_eq!(methods[0].0, "foo");
    }

    #[test]
    fn test_if_nil_narrowing() {
        let engine = create_engine();
        let uri = test_uri();

        let source = r#"
def process(value)
  if value.nil?
    # value is nil here
    puts "nil"
  else
    # value is not nil here
    value.upcase
  end
end
"#;

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        let methods = engine.get_method_cfgs(&uri);
        assert_eq!(methods.len(), 1);

        // The CFG should have been built with type guards
        assert!(engine.has_analysis(&uri));
    }

    #[test]
    fn test_is_a_narrowing() {
        let engine = create_engine();
        let uri = test_uri();

        let source = r#"
def process(value)
  if value.is_a?(String)
    value.upcase
  elsif value.is_a?(Integer)
    value + 1
  end
end
"#;

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        let methods = engine.get_method_cfgs(&uri);
        assert_eq!(methods.len(), 1);
    }

    #[test]
    fn test_case_when_narrowing() {
        let engine = create_engine();
        let uri = test_uri();

        let source = r#"
def process(value)
  case value
  when String
    value.upcase
  when Integer
    value + 1
  when nil
    "nil"
  end
end
"#;

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        let methods = engine.get_method_cfgs(&uri);
        assert_eq!(methods.len(), 1);
    }

    #[test]
    fn test_case_when_type_guards() {
        // Test that case/when properly creates type guards for String, Integer, and nil
        use crate::type_inference::cfg::{CfgBuilder, TypeGuard};

        let source = r#"
def process(value)
  case value
  when String
    value.upcase
  when Integer
    value + 1
  when nil
    "nil"
  end
end
"#;

        let result = ruby_prism::parse(source.as_bytes());
        let node = result.node();
        let program = node.as_program_node().expect("Expected program node");

        let method = program
            .statements()
            .body()
            .iter()
            .find_map(|node| node.as_def_node())
            .expect("No method found");

        let builder = CfgBuilder::new(source.as_bytes());
        let cfg = builder.build_from_method(&method);

        // Check that we have blocks with the correct type guards
        let mut found_string_guard = false;
        let mut found_integer_guard = false;
        let mut found_nil_guard = false;

        for block in cfg.blocks.values() {
            for guard in &block.entry_guards {
                match guard {
                    TypeGuard::CaseMatch {
                        variable,
                        pattern_type,
                    } if variable == "value" => {
                        let type_str = pattern_type.to_string();
                        if type_str == "String" {
                            found_string_guard = true;
                        } else if type_str == "Integer" {
                            found_integer_guard = true;
                        } else if type_str == "NilClass" {
                            found_nil_guard = true;
                        }
                    }
                    _ => {}
                }
            }
        }

        assert!(
            found_string_guard,
            "Should have a CaseMatch guard for String"
        );
        assert!(
            found_integer_guard,
            "Should have a CaseMatch guard for Integer"
        );
        assert!(found_nil_guard, "Should have a CaseMatch guard for nil");
    }

    #[test]
    fn test_unless_narrowing() {
        let engine = create_engine();
        let uri = test_uri();

        let source = r#"
def process(value)
  unless value.nil?
    # value is not nil here
    value.upcase
  end
end
"#;

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        let methods = engine.get_method_cfgs(&uri);
        assert_eq!(methods.len(), 1);
    }

    #[test]
    fn test_while_loop() {
        let engine = create_engine();
        let uri = test_uri();

        let source = r#"
def process(items)
  i = 0
  while i < items.length
    puts items[i]
    i += 1
  end
end
"#;

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        let methods = engine.get_method_cfgs(&uri);
        assert_eq!(methods.len(), 1);
    }

    #[test]
    fn test_begin_rescue() {
        let engine = create_engine();
        let uri = test_uri();

        let source = r#"
def process(value)
  begin
    result = value.upcase
  rescue NoMethodError
    result = "error"
  end
  result
end
"#;

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        let methods = engine.get_method_cfgs(&uri);
        assert_eq!(methods.len(), 1);
    }

    #[test]
    fn test_nested_class_methods() {
        let engine = create_engine();
        let uri = test_uri();

        let source = r#"
class Foo
  def bar
    x = 1
  end

  def baz
    y = 2
  end

  class Inner
    def inner_method
      z = 3
    end
  end
end
"#;

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        let methods = engine.get_method_cfgs(&uri);
        assert_eq!(methods.len(), 3);

        let method_names: Vec<_> = methods.iter().map(|(name, _, _)| name.as_str()).collect();
        assert!(method_names.contains(&"bar"));
        assert!(method_names.contains(&"baz"));
        assert!(method_names.contains(&"inner_method"));
    }

    #[test]
    fn test_module_methods() {
        let engine = create_engine();
        let uri = test_uri();

        let source = r#"
module Foo
  def bar
    x = 1
  end

  module Bar
    def baz
      y = 2
    end
  end
end
"#;

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        let methods = engine.get_method_cfgs(&uri);
        assert_eq!(methods.len(), 2);
    }

    #[test]
    fn test_file_change_reanalysis() {
        let engine = create_engine();
        let uri = test_uri();

        let source1 = "def foo; end";
        let source2 = "def foo; end\ndef bar; end";

        engine.on_file_open(&uri, source1);
        engine.analyze_file(&uri);

        let methods1 = engine.get_method_cfgs(&uri);
        assert_eq!(methods1.len(), 1);

        // Change file
        engine.on_file_change(&uri, source2);
        engine.analyze_file(&uri);

        let methods2 = engine.get_method_cfgs(&uri);
        assert_eq!(methods2.len(), 2);
    }

    #[test]
    fn test_file_close() {
        let engine = create_engine();
        let uri = test_uri();

        let source = "def foo; end";

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        assert!(engine.has_analysis(&uri));

        engine.on_file_close(&uri);

        // After close, analysis should be gone
        let methods = engine.get_method_cfgs(&uri);
        assert!(methods.is_empty());
    }

    #[test]
    fn test_stats() {
        let engine = create_engine();
        let uri1 = Url::parse("file:///test1.rb").unwrap();
        let uri2 = Url::parse("file:///test2.rb").unwrap();

        engine.on_file_open(&uri1, "def foo; end\ndef bar; end");
        engine.on_file_open(&uri2, "def baz; end");

        engine.analyze_file(&uri1);
        engine.analyze_file(&uri2);

        let (file_count, method_count) = engine.get_stats();
        assert_eq!(file_count, 2);
        assert_eq!(method_count, 3);
    }

    #[test]
    fn test_complex_control_flow() {
        let engine = create_engine();
        let uri = test_uri();

        let source = r#"
def complex_method(value, flag)
  if value.nil?
    return "nil"
  end

  result = if flag
    value.upcase
  else
    value.downcase
  end

  case result.length
  when 0
    "empty"
  when 1..5
    "short"
  else
    "long"
  end
end
"#;

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        let methods = engine.get_method_cfgs(&uri);
        assert_eq!(methods.len(), 1);
        assert!(engine.has_analysis(&uri));
    }

    #[test]
    fn test_boolean_guard_combinations() {
        let engine = create_engine();
        let uri = test_uri();

        let source = r#"
def process(a, b)
  if a.nil? && b.nil?
    "both nil"
  elsif a.nil? || b.nil?
    "one nil"
  else
    "neither nil"
  end
end
"#;

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        let methods = engine.get_method_cfgs(&uri);
        assert_eq!(methods.len(), 1);
    }

    #[test]
    fn test_respond_to_guard() {
        let engine = create_engine();
        let uri = test_uri();

        let source = r#"
def process(value)
  if value.respond_to?(:upcase)
    value.upcase
  else
    value.to_s
  end
end
"#;

        engine.on_file_open(&uri, source);
        engine.analyze_file(&uri);

        let methods = engine.get_method_cfgs(&uri);
        assert_eq!(methods.len(), 1);
    }
}
