use lsp_types::{
    SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokensFullOptions,
    SemanticTokensLegend, SemanticTokensOptions, WorkDoneProgressOptions,
};
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

// Define token types for our legend - these need to be in the same order as in semantic_tokens_options
const TOKEN_TYPE_NAMESPACE: u32 = 0;
const TOKEN_TYPE_TYPE: u32 = 1;
const TOKEN_TYPE_CLASS: u32 = 2;
const TOKEN_TYPE_ENUM: u32 = 3;
const TOKEN_TYPE_INTERFACE: u32 = 4;
const TOKEN_TYPE_STRUCT: u32 = 5;
const TOKEN_TYPE_TYPE_PARAMETER: u32 = 6;
const TOKEN_TYPE_PARAMETER: u32 = 7;
const TOKEN_TYPE_VARIABLE: u32 = 8;
const TOKEN_TYPE_PROPERTY: u32 = 9;
const TOKEN_TYPE_ENUM_MEMBER: u32 = 10;
const TOKEN_TYPE_EVENT: u32 = 11;
const TOKEN_TYPE_FUNCTION: u32 = 12;
const TOKEN_TYPE_METHOD: u32 = 13;
const TOKEN_TYPE_MACRO: u32 = 14;
const TOKEN_TYPE_KEYWORD: u32 = 15;
const TOKEN_TYPE_MODIFIER: u32 = 16;
const TOKEN_TYPE_COMMENT: u32 = 17;
const TOKEN_TYPE_STRING: u32 = 18;
const TOKEN_TYPE_NUMBER: u32 = 19;
const TOKEN_TYPE_REGEXP: u32 = 20;
const TOKEN_TYPE_OPERATOR: u32 = 21;
const TOKEN_TYPE_DECORATOR: u32 = 22;

/// Returns the semantic token options for the LSP server.
/// This defines the token types and modifiers that will be used for highlighting.
pub fn semantic_tokens_options() -> SemanticTokensOptions {
    // Define semantic token types and modifiers for Ruby
    let token_types = vec![
        SemanticTokenType::NAMESPACE,      // 0
        SemanticTokenType::TYPE,           // 1
        SemanticTokenType::CLASS,          // 2
        SemanticTokenType::ENUM,           // 3
        SemanticTokenType::INTERFACE,      // 4
        SemanticTokenType::STRUCT,         // 5
        SemanticTokenType::TYPE_PARAMETER, // 6
        SemanticTokenType::PARAMETER,      // 7
        SemanticTokenType::VARIABLE,       // 8
        SemanticTokenType::PROPERTY,       // 9
        SemanticTokenType::ENUM_MEMBER,    // 10
        SemanticTokenType::EVENT,          // 11
        SemanticTokenType::FUNCTION,       // 12
        SemanticTokenType::METHOD,         // 13
        SemanticTokenType::MACRO,          // 14
        SemanticTokenType::KEYWORD,        // 15
        SemanticTokenType::MODIFIER,       // 16
        SemanticTokenType::COMMENT,        // 17
        SemanticTokenType::STRING,         // 18
        SemanticTokenType::NUMBER,         // 19
        SemanticTokenType::REGEXP,         // 20
        SemanticTokenType::OPERATOR,       // 21
        SemanticTokenType::DECORATOR,      // 22
    ];

    let token_modifiers = vec![
        SemanticTokenModifier::DECLARATION,
        SemanticTokenModifier::DEFINITION,
        SemanticTokenModifier::READONLY,
        SemanticTokenModifier::STATIC,
        SemanticTokenModifier::DEPRECATED,
        SemanticTokenModifier::ABSTRACT,
        SemanticTokenModifier::ASYNC,
        SemanticTokenModifier::MODIFICATION,
        SemanticTokenModifier::DOCUMENTATION,
        SemanticTokenModifier::DEFAULT_LIBRARY,
    ];

    // Create the semantic tokens legend
    let legend = SemanticTokensLegend {
        token_types,
        token_modifiers,
    };

    SemanticTokensOptions {
        work_done_progress_options: WorkDoneProgressOptions::default(),
        legend,
        range: Some(true),
        full: Some(SemanticTokensFullOptions::Delta { delta: Some(true) }),
    }
}

/// Maps tree-sitter highlight scopes to LSP semantic token types.
/// This is a crucial function that determines how each syntax element is highlighted.
fn map_highlight_to_token_type(highlight_name: &str) -> Option<u32> {
    match highlight_name {
        "variable" | "variable.builtin" => Some(TOKEN_TYPE_VARIABLE),
        "variable.parameter" | "parameter" => Some(TOKEN_TYPE_PARAMETER),
        "property" | "property.builtin" | "instance_variable" | "class_variable" => {
            Some(TOKEN_TYPE_PROPERTY)
        }
        "function" | "function.builtin" => Some(TOKEN_TYPE_FUNCTION),
        "method" | "method.builtin" => Some(TOKEN_TYPE_METHOD),
        "keyword" | "keyword.type" => Some(TOKEN_TYPE_KEYWORD),
        "comment" => Some(TOKEN_TYPE_COMMENT),
        "string" | "string.special" => Some(TOKEN_TYPE_STRING),
        "number" => Some(TOKEN_TYPE_NUMBER),
        "regexp" | "string.regexp" => Some(TOKEN_TYPE_REGEXP),
        "operator" => Some(TOKEN_TYPE_OPERATOR),
        "constant" | "constant.builtin" => Some(TOKEN_TYPE_TYPE),
        "constructor" => Some(TOKEN_TYPE_FUNCTION),
        "attribute" => Some(TOKEN_TYPE_PROPERTY),
        "embedded" => Some(TOKEN_TYPE_MACRO),
        "tag" => Some(TOKEN_TYPE_DECORATOR),
        "module" => Some(TOKEN_TYPE_NAMESPACE),
        "type" | "type.builtin" => Some(TOKEN_TYPE_TYPE),
        "class" => Some(TOKEN_TYPE_CLASS),
        _ => None,
    }
}

/// Generate semantic tokens from Ruby code using tree-sitter-highlight
/// This function uses the tree-sitter-highlight library to parse and highlight Ruby code,
/// then converts the highlighting events into LSP semantic tokens.
pub fn generate_semantic_tokens(content: &str) -> Result<Vec<SemanticToken>, String> {
    // Define highlight names that we'll recognize
    let highlight_names: Vec<&str> = vec![
        "attribute",
        "comment",
        "constant",
        "constant.builtin",
        "constructor",
        "embedded",
        "function",
        "function.builtin",
        "keyword",
        "method",
        "method.builtin",
        "module",
        "number",
        "operator",
        "parameter",
        "property",
        "property.builtin",
        "regexp",
        "string",
        "string.special",
        "tag",
        "type",
        "type.builtin",
        "variable",
        "variable.parameter",
        "instance_variable",
        "class_variable",
        "string.regexp", // Add string.regexp for regular expressions
        "keyword.type",  // Add keyword.type for module declarations
    ];

    // Create highlighter and configuration
    let mut highlighter = Highlighter::new();
    let ruby_language = tree_sitter_ruby::LANGUAGE;

    let mut ruby_config = HighlightConfiguration::new(
        ruby_language.into(),
        "ruby",
        tree_sitter_ruby::HIGHLIGHTS_QUERY,
        "", // No injections query needed for now
        tree_sitter_ruby::LOCALS_QUERY,
    )
    .map_err(|e| format!("Failed to create highlight configuration: {}", e))?;

    ruby_config.configure(&highlight_names);

    // Generate highlight events
    let highlight_events = highlighter
        .highlight(&ruby_config, content.as_bytes(), None, |_| None)
        .map_err(|e| format!("Failed to highlight code: {}", e))?;

    // Convert highlight events to semantic tokens
    let mut tokens = Vec::new();
    let mut line = 0;
    let mut column = 0;
    let mut current_token_type: Option<u32> = None;
    let mut token_start_line = 0;
    let mut token_start_column = 0;

    // Helper function to calculate line and column from byte position
    let get_position = |pos: usize, content: &str| -> (u32, u32) {
        let preceding = &content[..pos];
        let line = preceding.chars().filter(|&c| c == '\n').count() as u32;
        let last_newline = preceding.rfind('\n').map_or(0, |i| i + 1);
        let column = preceding[last_newline..].chars().count() as u32;
        (line, column)
    };

    // Keep track of previous token positions for delta calculations
    let mut prev_line = 0;
    let mut prev_col = 0;

    for event_result in highlight_events {
        let event = event_result.map_err(|e| format!("Error in highlight event: {}", e))?;

        match event {
            HighlightEvent::Source { start, end } => {
                // Calculate position for source span
                let start_pos = get_position(start, content);
                let end_pos = get_position(end, content);

                // Only process if we have a current token type and the span is on a single line
                if start_pos.0 == end_pos.0 && current_token_type.is_some() {
                    line = start_pos.0;
                    column = start_pos.1;

                    let length = end_pos.1.saturating_sub(start_pos.1);
                    if length > 0 {
                        let token_type = current_token_type.unwrap();

                        // Calculate delta line and start from previous token
                        let delta_line = if tokens.is_empty() {
                            line
                        } else {
                            line.saturating_sub(prev_line)
                        };

                        let delta_start = if tokens.is_empty() || delta_line > 0 {
                            column
                        } else {
                            column.saturating_sub(prev_col)
                        };

                        tokens.push(SemanticToken {
                            delta_line,
                            delta_start,
                            length,
                            token_type,
                            token_modifiers_bitset: 0,
                        });

                        // Update previous token position
                        prev_line = line;
                        prev_col = column;
                    }
                }
            }
            HighlightEvent::HighlightStart(highlight) => {
                // Map highlight to token type
                if let Some(highlight_name) = highlight_names.get(highlight.0) {
                    current_token_type = map_highlight_to_token_type(highlight_name);
                }
            }
            HighlightEvent::HighlightEnd => {
                current_token_type = None;
            }
        }
    }

    Ok(tokens)
}

/// Generate semantic tokens for a specific range of text
/// This function extracts the text within the given range and generates tokens for it,
/// adjusting the token positions relative to the range start.
pub fn generate_semantic_tokens_for_range(
    content: &str,
    range: &lsp_types::Range,
) -> Result<Vec<SemanticToken>, String> {
    // Extract the text within the requested range
    let lines: Vec<&str> = content.lines().collect();
    let start_line = range.start.line as usize;
    let end_line = range.end.line as usize;

    if start_line >= lines.len() || end_line >= lines.len() {
        return Ok(Vec::new());
    }

    // Build the content for the requested range
    let range_content = if start_line == end_line {
        // Single line case
        let line = lines[start_line];
        let start_char = range.start.character as usize;
        let end_char = range.end.character as usize;
        if start_char >= line.len() || end_char > line.len() {
            return Ok(Vec::new());
        }
        line[start_char..end_char].to_string()
    } else {
        // Multi-line case
        let mut range_lines = Vec::new();

        // First line from start character
        let first_line = lines[start_line];
        let start_char = range.start.character as usize;
        if start_char <= first_line.len() {
            range_lines.push(&first_line[start_char..]);
        }

        // Middle lines complete
        range_lines.extend(&lines[start_line + 1..end_line]);

        // Last line up to end character
        let last_line = lines[end_line];
        let end_char = range.end.character as usize;
        if end_char <= last_line.len() {
            range_lines.push(&last_line[..end_char]);
        }

        range_lines.join("\n")
    };

    // Generate tokens for the range content
    let mut tokens = generate_semantic_tokens(&range_content)?;

    // Adjust token positions relative to the start of the range
    if !tokens.is_empty() {
        // For the first token, we need to adjust its position relative to the range start
        tokens[0].delta_line = 0;
        tokens[0].delta_start = range.start.character;

        // For subsequent tokens, we need to maintain their relative positions
        let mut current_line = 0;
        for i in 1..tokens.len() {
            current_line += tokens[i].delta_line;
            if current_line == 0 {
                tokens[i].delta_start += range.start.character;
            }
        }
    }

    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Creates a sample Ruby file for testing
    fn setup_test_file() -> String {
        let code = r#"#!/usr/bin/env ruby

# This is a test comment
module TestModule
  # Module-level constant
  VERSION = '1.0.0'

  # Mixin module
  module Helpers
    def helper_method
      puts 'Helper method called'
    end
  end

  # Main class definition
  class TestClass
    include Helpers
    extend Enumerable

    # Class-level instance variable
    @instances = 0

    # Property accessors with symbols
    attr_accessor :name, :age
    attr_reader :created_at

    # Class variable
    @@total_instances = 0

    # Regular expression constant
    EMAIL_REGEX = /^[a-zA-Z0-9_.+-]+@[a-zA-Z0-9-]+\.[a-zA-Z0-9-.]+$/

    # Initialize method with default parameter
    def initialize(name, age = 30)
      @name = name        # Instance variable
      @age = age         # Instance variable
      @created_at = Time.now
      @@total_instances += 1
      self.class.increment_instances
    end

    # Class method using self
    def self.increment_instances
      @instances += 1
    end

    # Instance method with string interpolation
    def greet
      # Heredoc usage
      message = <<~HEREDOC
        Hello, #{@name}!
        You are #{@age} years old.
        You were created at #{@created_at}.
      HEREDOC
      puts message
    end

    # Method with block parameter
    def self.create_many(count, &block)
      count.times.map do |i|
        instance = new("Person #{i}", 20 + i)
        block.call(instance) if block
        instance
      end
    end

    # Method with keyword arguments
    def update(name: nil, age: nil)
      @name = name if name
      @age = age if age
    end

    # Method demonstrating various Ruby operators
    def calculate(x, y)
      result = case x <=> y
               when -1 then :less
               when 0 then :equal
               when 1 then :greater
               end

      # Parallel assignment
      a, b = y, x

      # Range operator
      (a..b).each { |n| puts n }

      # Safe navigation operator
      result&.to_s
    end

    # Method with rescue clause
    def risky_operation
      raise 'Error' if @age < 0
    rescue StandardError => e
      puts "Caught error: #{e.message}"
    ensure
      puts 'Always executed'
    end
  end
end

# Create instances and demonstrate usage
if __FILE__ == $PROGRAM_NAME
  # Array of symbols
  valid_names = %i[john jane alice bob]

  # Hash with symbol keys
  config = {
    enabled: true,
    max_retries: 3,
    timeout: 30
  }

  # Block usage with do..end
  TestModule::TestClass.create_many(3) do |person|
    person.greet
  end

  # Block usage with braces
  %w[Alice Bob].each { |name|
    person = TestModule::TestClass.new(name)
    person.update(age: 25)
  }

  # Regular expression matching
  name = 'test@example.com'
  if name =~ TestModule::TestClass::EMAIL_REGEX
    puts 'Valid email'
  end
end"#;

        // Write the code to a file for persistence
        let file_path = "test_method.rb";
        fs::write(file_path, code).expect("Failed to write test file");
        code.to_string()
    }

    #[test]
    fn test_semantic_token_generation() {
        // Setup test file with comments
        let content = setup_test_file();

        // Generate semantic tokens
        let tokens =
            generate_semantic_tokens(&content).expect("Failed to generate semantic tokens");

        // Test that tokens were generated
        assert!(!tokens.is_empty(), "No semantic tokens were generated");

        // Print tokens for debugging
        println!("Generated tokens:");
        let mut line = 0;
        let mut col = 0;
        for token in &tokens {
            line += token.delta_line;
            if token.delta_line > 0 {
                col = 0;
            }
            col += token.delta_start;
            println!(
                "Line {}, Col {}, Len {}, Type {}",
                line, col, token.length, token.token_type
            );
        }

        // Validate basic token types
        let has_keywords = tokens.iter().any(|t| t.token_type == TOKEN_TYPE_KEYWORD);
        assert!(has_keywords, "No keyword tokens found");

        let has_methods = tokens
            .iter()
            .any(|t| t.token_type == TOKEN_TYPE_METHOD || t.token_type == TOKEN_TYPE_FUNCTION);
        assert!(has_methods, "No method/function tokens found");

        let has_comments = tokens.iter().any(|t| t.token_type == TOKEN_TYPE_COMMENT);
        assert!(has_comments, "No comment tokens found");

        // Validate Ruby-specific token types
        let has_constants = tokens.iter().any(|t| t.token_type == TOKEN_TYPE_TYPE);
        assert!(has_constants, "No constant tokens found");

        let has_instance_vars = tokens.iter().any(|t| t.token_type == TOKEN_TYPE_PROPERTY);
        assert!(has_instance_vars, "No instance variable tokens found");

        let has_operators = tokens.iter().any(|t| t.token_type == TOKEN_TYPE_OPERATOR);
        assert!(has_operators, "No operator tokens found");

        // Validate token format
        for token in &tokens {
            assert!(token.length > 0, "Token has zero length");
        }
    }

    #[test]
    fn test_semantic_token_generation_for_range() {
        // Setup test file
        let content = setup_test_file();

        println!("\nTest file content:\n{}", content);

        // Test single-line range containing a module definition
        let single_line_range = lsp_types::Range {
            start: lsp_types::Position {
                line: 3,
                character: 0,
            },
            end: lsp_types::Position {
                line: 4,
                character: 0,
            },
        };

        // Print the exact line we're trying to tokenize
        let lines: Vec<&str> = content.lines().collect();
        println!(
            "\nTargeting line {}:\n{}",
            single_line_range.start.line, lines[single_line_range.start.line as usize]
        );

        let tokens = generate_semantic_tokens_for_range(&content, &single_line_range)
            .expect("Failed to generate tokens for single line range");

        println!("\nGenerated tokens for single line range:");
        for token in &tokens {
            println!(
                "Token: delta_line={}, delta_start={}, length={}, type={}",
                token.delta_line, token.delta_start, token.length, token.token_type
            );
        }

        // Verify tokens for "module TestModule" line
        assert!(
            !tokens.is_empty(),
            "No tokens generated for single line range"
        );

        // Test multi-line range containing module and constant
        let multi_line_range = lsp_types::Range {
            start: lsp_types::Position {
                line: 3,
                character: 0,
            },
            end: lsp_types::Position {
                line: 6,
                character: 0,
            },
        };

        println!("\nTargeting multi-line range:");
        for i in single_line_range.start.line as usize..=multi_line_range.end.line as usize {
            println!("Line {}: {}", i, lines[i]);
        }

        let tokens = generate_semantic_tokens_for_range(&content, &multi_line_range)
            .expect("Failed to generate tokens for multi line range");

        println!("\nGenerated tokens for multi-line range:");
        for token in &tokens {
            println!(
                "Token: delta_line={}, delta_start={}, length={}, type={}",
                token.delta_line, token.delta_start, token.length, token.token_type
            );
        }

        assert!(
            !tokens.is_empty(),
            "No tokens generated for multi line range"
        );

        // Verify we have both keyword and constant tokens
        let has_keyword = tokens.iter().any(|t| t.token_type == TOKEN_TYPE_KEYWORD);
        let has_constant = tokens.iter().any(|t| t.token_type == TOKEN_TYPE_TYPE);
        assert!(has_keyword, "No keyword token found in multi-line range");
        assert!(has_constant, "No constant token found in multi-line range");

        // Test invalid range
        let invalid_range = lsp_types::Range {
            start: lsp_types::Position {
                line: 1000,
                character: 0,
            },
            end: lsp_types::Position {
                line: 1001,
                character: 10,
            },
        };

        let tokens = generate_semantic_tokens_for_range(&content, &invalid_range)
            .expect("Failed to handle invalid range");
        assert!(tokens.is_empty(), "Tokens generated for invalid range");
    }
}
