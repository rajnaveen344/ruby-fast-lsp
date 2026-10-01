use super::*;

// =========================================================================
// Standard YARD Format Tests: @param name [Type] description
// =========================================================================

#[test]
fn test_parse_param() {
    // Standard YARD format: @param name [Type] description
    let comment = "# @param name [String] the user's name";
    let doc = YardParser::parse(comment);

    assert_eq!(doc.params.len(), 1);
    assert_eq!(doc.params[0].name, "name");
    assert_eq!(doc.params[0].types, vec!["String"]);
    assert_eq!(
        doc.params[0].description,
        Some("the user's name".to_string())
    );
}

#[test]
fn test_parse_multiple_params() {
    let comment = r#"
# @param name [String] the user's name
# @param age [Integer] the user's age
# @return [Boolean] whether the user is valid
"#;
    let doc = YardParser::parse(comment);

    assert_eq!(doc.params.len(), 2);
    assert_eq!(doc.params[0].name, "name");
    assert_eq!(doc.params[0].types, vec!["String"]);
    assert_eq!(doc.params[1].name, "age");
    assert_eq!(doc.params[1].types, vec!["Integer"]);

    assert_eq!(doc.returns.len(), 1);
    assert_eq!(doc.returns[0].types, vec!["Boolean"]);
}

#[test]
fn test_parse_union_types() {
    // Standard format with union types
    let comment = "# @param name [String, nil] optional name";
    let doc = YardParser::parse(comment);

    assert_eq!(doc.params.len(), 1);
    assert_eq!(doc.params[0].name, "name");
    assert_eq!(doc.params[0].types, vec!["String", "nil"]);
}

#[test]
fn test_parse_return_type() {
    let comment = "# @return [Array<String>] list of names";
    let doc = YardParser::parse(comment);

    assert_eq!(doc.returns.len(), 1);
    assert_eq!(doc.returns[0].types, vec!["Array<String>"]);
    assert_eq!(
        doc.returns[0].description,
        Some("list of names".to_string())
    );
}

#[test]
fn test_parse_with_description() {
    let comment = r#"
# Greets a user by name
#
# @param name [String] the user's name
# @return [String] the greeting message
"#;
    let doc = YardParser::parse(comment);

    assert_eq!(doc.description, Some("Greets a user by name".to_string()));
    assert_eq!(doc.params.len(), 1);
    assert_eq!(doc.returns.len(), 1);
}

// =========================================================================
// @option Tag Tests
// =========================================================================

#[test]
fn test_parse_option_tag() {
    let comment = r#"
# @param opts [Hash] configuration options
# @option opts [String] :url ('localhost') The server URL
# @option opts [Integer] :retries (3) Number of retry attempts
"#;
    let doc = YardParser::parse(comment);

    assert_eq!(doc.params.len(), 1);
    assert_eq!(doc.params[0].name, "opts");
    assert_eq!(doc.params[0].types, vec!["Hash"]);

    assert_eq!(doc.options.len(), 2);

    // First option
    assert_eq!(doc.options[0].param_name, "opts");
    assert_eq!(doc.options[0].key_name, "url");
    assert_eq!(doc.options[0].types, vec!["String"]);
    assert_eq!(doc.options[0].default, Some("'localhost'".to_string()));
    assert_eq!(
        doc.options[0].description,
        Some("The server URL".to_string())
    );

    // Second option
    assert_eq!(doc.options[1].param_name, "opts");
    assert_eq!(doc.options[1].key_name, "retries");
    assert_eq!(doc.options[1].types, vec!["Integer"]);
    assert_eq!(doc.options[1].default, Some("3".to_string()));
}

#[test]
fn test_parse_option_without_default() {
    let comment = "# @option config [Boolean] :force force execution";
    let doc = YardParser::parse(comment);

    assert_eq!(doc.options.len(), 1);
    assert_eq!(doc.options[0].param_name, "config");
    assert_eq!(doc.options[0].key_name, "force");
    assert_eq!(doc.options[0].types, vec!["Boolean"]);
    assert_eq!(doc.options[0].default, None);
    assert_eq!(
        doc.options[0].description,
        Some("force execution".to_string())
    );
}

// =========================================================================
// Hash Type Syntax Tests
// =========================================================================

#[test]
fn test_parse_hash_brace_syntax() {
    // Standard YARD Hash syntax: Hash{KeyType => ValueType}
    let comment = "# @return [Hash{Symbol => String}] user settings";
    let doc = YardParser::parse(comment);

    assert_eq!(doc.returns.len(), 1);
    assert_eq!(doc.returns[0].types, vec!["Hash{Symbol => String}"]);
}

#[test]
fn test_parse_hash_angle_bracket_syntax() {
    // Alternative syntax: Hash<K, V>
    let comment = "# @return [Hash<Symbol, String>] user settings";
    let doc = YardParser::parse(comment);

    assert_eq!(doc.returns.len(), 1);
    assert_eq!(doc.returns[0].types, vec!["Hash<Symbol, String>"]);
}

#[test]
fn test_parse_hash_with_union_inside_braces() {
    // Hash with union in value type
    let comment = "# @return [Hash{Symbol => String, Integer}] mixed values";
    let doc = YardParser::parse(comment);

    assert_eq!(doc.returns.len(), 1);
    // The entire Hash type should be preserved as one type
    assert_eq!(
        doc.returns[0].types,
        vec!["Hash{Symbol => String, Integer}"]
    );
}

// =========================================================================
// Yield Tests
// =========================================================================

#[test]
fn test_parse_yield_params() {
    let comment = r#"
# @yieldparam index [Integer] the current index
# @yieldreturn [Boolean] whether to continue
"#;
    let doc = YardParser::parse(comment);

    assert_eq!(doc.yield_params.len(), 1);
    assert_eq!(doc.yield_params[0].name, "index");
    assert_eq!(doc.yield_params[0].types, vec!["Integer"]);

    assert_eq!(doc.yield_returns.len(), 1);
    assert_eq!(doc.yield_returns[0].types, vec!["Boolean"]);
}

// =========================================================================
// Other Tag Tests
// =========================================================================

#[test]
fn test_parse_deprecated() {
    let comment = "# @deprecated Use new_method instead";
    let doc = YardParser::parse(comment);

    assert_eq!(doc.deprecated, Some("Use new_method instead".to_string()));
}

#[test]
fn test_parse_unavailable() {
    let comment = "# @unavailable JRuby does not implement process forking on the JVM.";
    let doc = YardParser::parse(comment);

    assert_eq!(
        doc.unavailable,
        Some("JRuby does not implement process forking on the JVM.".to_string())
    );
}

#[test]
fn test_extract_unavailable_without_description_or_type_tags() {
    let source =
        "module Process\n  # @unavailable Not supported here.\n  def self.fork\n  end\nend\n";
    let method_start = source
        .find("def self.fork")
        .expect("test method must exist");
    let doc = YardParser::extract_from_source(source, method_start)
        .expect("@unavailable alone must retain method metadata");

    assert_eq!(doc.unavailable, Some("Not supported here.".to_string()));
}

#[test]
fn test_parse_absent() {
    let comment = "# @absent JRuby does not expose ObjectSpace.dump.";
    let doc = YardParser::parse(comment);

    assert_eq!(
        doc.absent,
        Some("JRuby does not expose ObjectSpace.dump.".to_string())
    );
}

#[test]
fn test_parse_raises() {
    let comment = "# @raise [ArgumentError] if name is nil";
    let doc = YardParser::parse(comment);

    assert_eq!(doc.raises.len(), 1);
    assert_eq!(doc.raises[0], "ArgumentError");
}

// =========================================================================
// Extract from Source Tests
// =========================================================================

#[test]
fn test_extract_from_source_standard_format() {
    let source = r#"
class User
  # Creates a new user
  # @param name [String] the user's name
  # @param age [Integer] the user's age
  # @return [User] the new user instance
  def initialize(name, age)
    @name = name
    @age = age
  end
end
"#;

    // Find the offset of "def initialize"
    let method_start = source.find("def initialize").unwrap();
    let doc = YardParser::extract_from_source(source, method_start).unwrap();

    assert_eq!(doc.description, Some("Creates a new user".to_string()));
    assert_eq!(doc.params.len(), 2);
    assert_eq!(doc.params[0].name, "name");
    assert_eq!(doc.params[0].types, vec!["String"]);
    assert_eq!(doc.params[1].name, "age");
    assert_eq!(doc.params[1].types, vec!["Integer"]);
    assert_eq!(doc.returns.len(), 1);
}

#[test]
fn test_line_aware_extraction_matches_public_path() {
    let sources = [
            "# @param name [String] description\ndef call(name)\nend\n",
            "module Example\r\n  # Description 😀\r\n\r\n  # @return [String] value\r\n  def call\r\n  end\r\nend\r\n",
            "  # @deprecated use new_call\n  def call\n  end\n",
        ];

    for source in sources {
        let method_start = source.find("def call").expect("fixture method must exist");
        let method_line = u32::try_from(
            source[..method_start]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count(),
        )
        .expect("fixture line must fit u32");
        assert_eq!(
            YardParser::extract_from_source_at_line(source, method_start, method_line),
            YardParser::extract_from_source(source, method_start)
        );
    }
}

#[test]
fn test_extract_from_source_with_options() {
    let source = r#"
class Server
  # Connect to the server
  # @param opts [Hash] configuration options
  # @option opts [String] :url ('localhost') The server URL
  # @option opts [Integer] :port (8080) The port number
  # @return [Boolean] connection status
  def connect(opts = {})
  end
end
"#;

    let method_start = source.find("def connect").unwrap();
    let doc = YardParser::extract_from_source(source, method_start).unwrap();

    assert_eq!(doc.params.len(), 1);
    assert_eq!(doc.params[0].name, "opts");
    assert_eq!(doc.params[0].types, vec!["Hash"]);

    assert_eq!(doc.options.len(), 2);
    assert_eq!(doc.options[0].key_name, "url");
    assert_eq!(doc.options[1].key_name, "port");
}

#[test]
fn test_extract_no_yard_comment() {
    let source = r#"
class User
  def greet
    puts "Hello"
  end
end
"#;

    let method_start = source.find("def greet").unwrap();
    let doc = YardParser::extract_from_source(source, method_start);

    assert!(doc.is_none());
}

#[test]
fn test_format_signature_hint() {
    let comment = r#"
# @param name [String]
# @param age [Integer]
# @return [Boolean]
"#;
    let doc = YardParser::parse(comment);
    let hint = doc.format_signature_hint();

    assert_eq!(hint, "(name: String, age: Integer) -> Boolean");
}

#[test]
fn test_parse_type_list_simple() {
    assert_eq!(parse_type_list("String"), vec!["String"]);
    assert_eq!(
        parse_type_list("String, Integer"),
        vec!["String", "Integer"]
    );
    assert_eq!(parse_type_list("String, nil"), vec!["String", "nil"]);
}

#[test]
fn test_parse_type_list_with_generics() {
    // Single generic type
    assert_eq!(parse_type_list("Array<String>"), vec!["Array<String>"]);

    // Generic type with union inside
    assert_eq!(
        parse_type_list("Hash<Symbol, String>"),
        vec!["Hash<Symbol, String>"]
    );

    // Multiple types including generic
    assert_eq!(
        parse_type_list("Array<String>, nil"),
        vec!["Array<String>", "nil"]
    );

    // Hash with union value types
    assert_eq!(
        parse_type_list("Hash<Symbol, String, Integer>"),
        vec!["Hash<Symbol, String, Integer>"]
    );
}

#[test]
fn test_parse_type_list_nested_generics() {
    // Nested generics
    assert_eq!(
        parse_type_list("Array<Hash<Symbol, String>>"),
        vec!["Array<Hash<Symbol, String>>"]
    );

    // Multiple nested with union
    assert_eq!(
        parse_type_list("Array<Hash<Symbol, String>>, nil"),
        vec!["Array<Hash<Symbol, String>>", "nil"]
    );
}

#[test]
fn test_parse_hash_type_format() {
    // Standard Hash<K, V> format
    let comment = "# @return [Hash<Symbol, String>] user settings";
    let doc = YardParser::parse(comment);
    assert_eq!(doc.returns[0].types, vec!["Hash<Symbol, String>"]);

    // Hash with union value types for kwargs (standard format)
    let comment = "# @param options [Hash<Symbol, String, Integer>] mixed value types";
    let doc = YardParser::parse(comment);
    assert_eq!(doc.params[0].types, vec!["Hash<Symbol, String, Integer>"]);
}

#[test]
fn test_parse_type_list_hash_brace_syntax() {
    // YARD standard Hash{K => V} syntax
    assert_eq!(
        parse_type_list("Hash{Symbol => String}"),
        vec!["Hash{Symbol => String}"]
    );

    // Hash with union value type
    assert_eq!(
        parse_type_list("Hash{Symbol => String, Integer}"),
        vec!["Hash{Symbol => String, Integer}"]
    );

    // Hash type with union outside
    assert_eq!(
        parse_type_list("Hash{Symbol => String}, nil"),
        vec!["Hash{Symbol => String}", "nil"]
    );
}

#[test]
fn test_parse_rest_args_union_types() {
    // *args can accept multiple types (standard format)
    let comment = "# @param items [Array<String, Integer>] can be strings or integers";
    let doc = YardParser::parse(comment);
    assert_eq!(doc.params[0].name, "items");
    assert_eq!(doc.params[0].types, vec!["Array<String, Integer>"]);
}

// =========================================================================
// Position Tracking Tests
// =========================================================================

#[test]
fn test_extract_from_source_with_positions() {
    let source = r#"class User
  # Creates a new user
  # @param name [String] the user's name
  # @param age [Integer] the user's age
  # @return [User] the new user instance
  def initialize(name, age)
    @name = name
  end
end
"#;

    // Find the offset of "def initialize"
    let method_start = source.find("def initialize").unwrap();
    let doc = YardParser::extract_from_source(source, method_start).unwrap();

    // Check that params have position information
    assert_eq!(doc.params.len(), 2);

    // First param should have range info
    let name_param = &doc.params[0];
    assert_eq!(name_param.name, "name");
    assert!(name_param.range.is_some(), "name param should have range");
    let name_range = name_param.range.unwrap();
    assert_eq!(name_range.start.line, 2); // Line 2 (0-indexed)

    // Second param should have range info
    let age_param = &doc.params[1];
    assert_eq!(age_param.name, "age");
    assert!(age_param.range.is_some(), "age param should have range");
    let age_range = age_param.range.unwrap();
    assert_eq!(age_range.start.line, 3); // Line 3 (0-indexed)
}

#[test]
fn test_find_unmatched_params() {
    let source = r#"class User
  # @param wrong_name [String] this doesn't exist
  # @param correct_name [String] this exists
  # @param also_wrong [Integer] this doesn't exist
  def my_method(correct_name)
  end
end
"#;

    let method_start = source.find("def my_method").unwrap();
    let doc = YardParser::extract_from_source(source, method_start).unwrap();

    // Find unmatched params
    let actual_params = vec!["correct_name"];
    let unmatched = doc.find_unmatched_params(&actual_params);

    // Should have 2 unmatched params
    assert_eq!(unmatched.len(), 2);

    let unmatched_names: Vec<&str> = unmatched.iter().map(|(p, _)| p.name.as_str()).collect();
    assert!(unmatched_names.contains(&"wrong_name"));
    assert!(unmatched_names.contains(&"also_wrong"));
    assert!(!unmatched_names.contains(&"correct_name"));
}

#[test]
fn test_all_params_matched() {
    let source = r#"class User
  # @param name [String] the name
  # @param age [Integer] the age
  def my_method(name, age)
  end
end
"#;

    let method_start = source.find("def my_method").unwrap();
    let doc = YardParser::extract_from_source(source, method_start).unwrap();

    // All params match
    let actual_params = vec!["name", "age"];
    let unmatched = doc.find_unmatched_params(&actual_params);

    assert!(unmatched.is_empty(), "All params should match");
}

// =========================================================================
// Summary Table from YARD Cheat Sheet
// =========================================================================
// | Argument Type   | Ruby Syntax  | YARD Tag  | YARD Name Rule | Type Example         |
// |-----------------|--------------|-----------|----------------|----------------------|
// | Standard        | arg          | @param    | arg            | [String]             |
// | Multiple Types  | arg          | @param    | arg            | [String, Integer]    |
// | Array           | arg          | @param    | arg            | [Array<String>]      |
// | Rest (Splat)    | *args        | @param    | args (no *)    | [Array<Object>]      |
// | Key Rest (Splat)| **kwargs     | @param    | kwargs (no **) | [Hash]               |
// | Hash Option     | opts         | @option   | opts           | [Type] :key          |

#[test]
fn test_yard_cheat_sheet_formats() {
    let comment = r#"
# Method demonstrating all YARD parameter formats
# @param user_id [Integer] standard param
# @param id [Integer, String] multiple types
# @param names [Array<String>] array type
# @param numbers [Array<Integer>] rest args (no * in name)
# @param config [Hash] keyword rest (no ** in name)
# @option config [Boolean] :force (false) force execution
# @option config [String] :prefix prefix for output
# @return [Hash{Symbol => Object}] result
"#;
    let doc = YardParser::parse(comment);

    // Standard param
    assert_eq!(doc.params[0].name, "user_id");
    assert_eq!(doc.params[0].types, vec!["Integer"]);

    // Multiple types
    assert_eq!(doc.params[1].name, "id");
    assert_eq!(doc.params[1].types, vec!["Integer", "String"]);

    // Array type
    assert_eq!(doc.params[2].name, "names");
    assert_eq!(doc.params[2].types, vec!["Array<String>"]);

    // Rest args (splat)
    assert_eq!(doc.params[3].name, "numbers");
    assert_eq!(doc.params[3].types, vec!["Array<Integer>"]);

    // Keyword rest (double splat)
    assert_eq!(doc.params[4].name, "config");
    assert_eq!(doc.params[4].types, vec!["Hash"]);

    // Options
    assert_eq!(doc.options.len(), 2);
    assert_eq!(doc.options[0].param_name, "config");
    assert_eq!(doc.options[0].key_name, "force");
    assert_eq!(doc.options[0].types, vec!["Boolean"]);
    assert_eq!(doc.options[0].default, Some("false".to_string()));

    assert_eq!(doc.options[1].param_name, "config");
    assert_eq!(doc.options[1].key_name, "prefix");
    assert_eq!(doc.options[1].types, vec!["String"]);
    assert_eq!(doc.options[1].default, None);

    // Return type with Hash{K => V} syntax
    assert_eq!(doc.returns[0].types, vec!["Hash{Symbol => Object}"]);
}
