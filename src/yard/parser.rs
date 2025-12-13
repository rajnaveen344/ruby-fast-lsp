//! YARD Documentation Parser
//!
//! Parses YARD documentation comments from Ruby source code and extracts
//! type annotations for methods, parameters, and return values.
//!
//! ## Supported YARD Formats
//!
//! ### Parameters (@param)
//! Format: `@param name [Type] description`
//! - `@param user_id [Integer] The unique identifier`
//! - `@param id [Integer, String] Can be int or string`
//! - `@param names [Array<String>] A list of names`
//! - `@param scores [Hash{Symbol => Integer}] Player scores`
//!
//! ### Options (@option)
//! For hash options: `@option hash_name [Type] :key_name (default) description`
//! - `@option opts [String] :url ('localhost') The server URL`
//!
//! ### Return Types (@return)
//! Format: `@return [Type] description`
//! - `@return [Boolean] Whether successful`
//! - `@return [String, nil] The result or nil`

use super::types::{YardMethodDoc, YardOption, YardParam, YardReturn};
use log::debug;
use regex::Regex;
use std::sync::LazyLock;
use tower_lsp::lsp_types::{Position, Range};

// =============================================================================
// Regex Patterns
// =============================================================================

/// @param name [Type] description
/// Groups: 1=name, 2=type, 3=description
static PARAM_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@param\s+(\w+)\s+\[([^\]]+)\]\s*(.*)").expect("Invalid param regex")
});

/// @return [Type] description
/// Groups: 1=type, 2=description
static RETURN_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"@return\s+\[([^\]]+)\]\s*(.*)").expect("Invalid return regex"));

/// @yieldparam name [Type] description
/// Groups: 1=name, 2=type, 3=description
static YIELD_PARAM_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@yieldparam\s+(\w+)\s+\[([^\]]+)\]\s*(.*)").expect("Invalid yieldparam regex")
});

/// @yieldreturn [Type] description
/// Groups: 1=type, 2=description
static YIELD_RETURN_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@yieldreturn\s+\[([^\]]+)\]\s*(.*)").expect("Invalid yieldreturn regex")
});

/// @option hash_name [Type] :key_name (default) description
/// Groups: 1=hash_name, 2=type, 3=key_name, 4=default (optional), 5=description
static OPTION_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@option\s+(\w+)\s+\[([^\]]+)\]\s+:(\w+)(?:\s+\(([^)]*)\))?\s*(.*)")
        .expect("Invalid option regex")
});

/// @raise [ExceptionType] description
/// Groups: 1=exception_type
static RAISE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"@raise\s+\[([^\]]+)\]").expect("Invalid raise regex"));

/// @deprecated reason
/// Groups: 1=reason
static DEPRECATED_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"@deprecated\s*(.*)").expect("Invalid deprecated regex"));

// =============================================================================
// Helper Types
// =============================================================================

/// Information about a comment line for position tracking
/// Information about a comment line for position tracking
#[derive(Debug, Clone)]
pub struct CommentLineInfo<'a> {
    pub content: &'a str,
    pub line_number: u32,
    pub content_start_char: u32,
    pub line_length: u32,
}

// =============================================================================
// Parser
// =============================================================================

/// Information about a YARD type reference at a specific position
#[derive(Debug, Clone)]
pub struct YardTypeAtPosition {
    /// The type name at the position
    pub type_name: String,
    /// The range of the type name in the document
    pub range: Range,
}

/// Regex to find type references within square brackets
static TYPE_BRACKET_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([^\]]+)\]").expect("Invalid type bracket regex"));

/// Parser for YARD documentation comments
pub struct YardParser;

impl YardParser {
    /// Find the YARD type at a specific position in the source code.
    /// Returns the type name and its range if the position is on a type in a YARD comment.
    pub fn find_type_at_position(content: &str, position: Position) -> Option<YardTypeAtPosition> {
        let lines: Vec<&str> = content.lines().collect();
        let line_idx = position.line as usize;

        if line_idx >= lines.len() {
            return None;
        }

        let line = lines[line_idx];

        // Check if this line is a comment
        let trimmed = line.trim_start();
        if !trimmed.starts_with('#') {
            return None;
        }

        // Look for type brackets [Type] on this line
        for caps in TYPE_BRACKET_REGEX.captures_iter(line) {
            let full_match = caps.get(0)?;
            let types_match = caps.get(1)?;

            let bracket_start = full_match.start();
            let bracket_end = full_match.end();

            // Check if position is within the brackets
            let char_pos = position.character as usize;
            if char_pos >= bracket_start && char_pos < bracket_end {
                // Position is inside [Type], find which type
                let types_str = types_match.as_str();
                let types_start = types_match.start();

                // Parse individual types (handling commas for union types)
                let relative_pos = char_pos.saturating_sub(types_start);

                // Split by comma but track positions
                let mut current_pos = 0;
                for type_part in Self::split_types_with_positions(types_str) {
                    let type_start = type_part.0;
                    let type_end = type_part.1;
                    let type_name = type_part.2.trim();

                    if relative_pos >= type_start && relative_pos < type_end {
                        // Found the type at position
                        // Extract just the base type name (without generics)
                        let base_type = Self::extract_base_type(type_name);

                        if !base_type.is_empty() {
                            let range = Range::new(
                                Position::new(position.line, (types_start + type_start) as u32),
                                Position::new(position.line, (types_start + type_end) as u32),
                            );
                            return Some(YardTypeAtPosition {
                                type_name: base_type,
                                range,
                            });
                        }
                    }

                    current_pos = type_end;
                }

                // If we're past all types but still in brackets, check last type
                if relative_pos >= current_pos && !types_str.is_empty() {
                    let base_type = Self::extract_base_type(types_str.trim());
                    if !base_type.is_empty() {
                        let range = Range::new(
                            Position::new(position.line, types_start as u32),
                            Position::new(position.line, (types_start + types_str.len()) as u32),
                        );
                        return Some(YardTypeAtPosition {
                            type_name: base_type,
                            range,
                        });
                    }
                }
            }
        }

        None
    }

    /// Split type string by commas, tracking positions
    fn split_types_with_positions(types_str: &str) -> Vec<(usize, usize, &str)> {
        let mut result = Vec::new();
        let mut start = 0;
        let mut depth = 0;

        for (i, c) in types_str.char_indices() {
            match c {
                '<' | '{' => depth += 1,
                '>' | '}' => depth -= 1,
                ',' if depth == 0 => {
                    result.push((start, i, &types_str[start..i]));
                    start = i + 1;
                }
                _ => {}
            }
        }

        // Add last part
        if start < types_str.len() {
            result.push((start, types_str.len(), &types_str[start..]));
        }

        result
    }

    /// Extract base type name from a type string (removes generics)
    /// "Array<String>" -> "Array"
    /// "Hash{Symbol => String}" -> "Hash"
    /// "String" -> "String"
    fn extract_base_type(type_str: &str) -> String {
        let trimmed = type_str.trim();

        // Find the first < or {
        if let Some(pos) = trimmed.find(['<', '{']) {
            trimmed[..pos].trim().to_string()
        } else {
            trimmed.to_string()
        }
    }

    /// Parse YARD documentation from a comment string.
    /// The comment should be the raw comment text including # characters.
    /// This method does NOT track positions (for simple parsing use cases).
    pub fn parse(comment: &str) -> YardMethodDoc {
        let lines: Vec<CommentLineInfo> = comment
            .lines()
            .map(|line| {
                let trimmed = line.trim().trim_start_matches('#').trim();
                CommentLineInfo {
                    content: trimmed,
                    line_number: 0,
                    content_start_char: 0,
                    line_length: 0,
                }
            })
            .collect();

        Self::parse_lines(&lines, false)
    }

    /// Extract YARD documentation from comments preceding a method definition.
    /// Includes position information for each @param tag for diagnostics.
    pub fn extract_from_source(content: &str, method_start_offset: usize) -> Option<YardMethodDoc> {
        let content_before = &content[..method_start_offset];
        let lines_before: Vec<&str> = content_before.lines().collect();

        if lines_before.is_empty() {
            return None;
        }

        let comment_lines = Self::collect_preceding_comments(&lines_before);
        if comment_lines.is_empty() {
            return None;
        }

        let doc = Self::parse_lines(&comment_lines, true);

        if doc.has_type_info() || doc.description.is_some() {
            Some(doc)
        } else {
            None
        }
    }

    /// Collect comment lines immediately preceding a method definition.
    fn collect_preceding_comments<'a>(lines_before: &[&'a str]) -> Vec<CommentLineInfo<'a>> {
        let mut comment_lines: Vec<CommentLineInfo<'a>> = Vec::new();
        let mut i = lines_before.len() - 1;

        // Skip trailing whitespace lines
        while i > 0 && lines_before[i].trim().is_empty() {
            i -= 1;
        }

        // Collect comment lines (bottom-up)
        loop {
            let original_line = lines_before[i];
            let trimmed = original_line.trim();

            if trimmed.starts_with('#') {
                let leading_ws = original_line.len() - original_line.trim_start().len();
                let content = trimmed.trim_start_matches('#').trim_start();
                let hash_and_space = trimmed.len() - content.len();

                comment_lines.push(CommentLineInfo {
                    content,
                    line_number: i as u32,
                    content_start_char: (leading_ws + hash_and_space) as u32,
                    line_length: original_line.len() as u32,
                });
            } else if !trimmed.is_empty() {
                break;
            }

            if i == 0 {
                break;
            }
            i -= 1;
        }

        comment_lines.reverse();
        comment_lines
    }

    /// Core parsing logic shared between `parse` and `extract_from_source`.
    pub fn parse_lines(lines: &[CommentLineInfo], track_positions: bool) -> YardMethodDoc {
        let mut doc = YardMethodDoc::new();
        let mut description_lines: Vec<&str> = Vec::new();
        let mut in_description = true;

        for line_info in lines {
            let line = line_info.content;
            if line.is_empty() {
                continue;
            }

            if line.starts_with('@') {
                in_description = false;
                Self::parse_tag(line, &mut doc, line_info, track_positions);
            } else if in_description {
                description_lines.push(line);
            }
        }

        if !description_lines.is_empty() {
            doc.description = Some(description_lines.join(" "));
        }

        debug!("Parsed YARD doc: {:?}", doc);
        doc
    }

    /// Parse a single YARD tag line and add it to the document.
    fn parse_tag(
        line: &str,
        doc: &mut YardMethodDoc,
        line_info: &CommentLineInfo,
        track_positions: bool,
    ) {
        // Calculate line range (entire @param line)
        let line_range = if track_positions {
            Some(Range {
                start: Position {
                    line: line_info.line_number,
                    character: line_info.content_start_char,
                },
                end: Position {
                    line: line_info.line_number,
                    character: line_info.line_length,
                },
            })
        } else {
            None
        };

        if let Some(caps) = PARAM_REGEX.captures(line) {
            let name = caps
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let types = parse_type_list(caps.get(2).map(|m| m.as_str()).unwrap_or(""));
            let desc = non_empty_string(caps.get(3).map(|m| m.as_str().trim()));

            // Calculate types range (just the [Type] portion)
            let types_range = if track_positions {
                caps.get(2).map(|m| {
                    // m.start() and m.end() are byte offsets within `line`
                    // We need to account for the opening [ and include the closing ]
                    let bracket_start = m.start().saturating_sub(1); // include '['
                    let bracket_end = m.end() + 1; // include ']'
                    Range {
                        start: Position {
                            line: line_info.line_number,
                            character: line_info.content_start_char + bracket_start as u32,
                        },
                        end: Position {
                            line: line_info.line_number,
                            character: line_info.content_start_char + bracket_end as u32,
                        },
                    }
                })
            } else {
                None
            };

            if let (Some(r), Some(tr)) = (line_range, types_range) {
                doc.params
                    .push(YardParam::with_ranges(name, types, desc, r, tr));
            } else if let Some(r) = line_range {
                doc.params.push(YardParam::with_range(name, types, desc, r));
            } else {
                doc.params.push(YardParam::new(name, types, desc));
            }
        } else if let Some(caps) = OPTION_REGEX.captures(line) {
            let param_name = caps
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let types = parse_type_list(caps.get(2).map(|m| m.as_str()).unwrap_or(""));
            let key_name = caps
                .get(3)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let default = non_empty_string(caps.get(4).map(|m| m.as_str().trim()));
            let desc = non_empty_string(caps.get(5).map(|m| m.as_str().trim()));

            if let Some(r) = line_range {
                doc.options.push(YardOption::with_range(
                    param_name, key_name, types, default, desc, r,
                ));
            } else {
                doc.options
                    .push(YardOption::new(param_name, key_name, types, default, desc));
            }
        } else if let Some(caps) = RETURN_REGEX.captures(line) {
            let types = parse_type_list(caps.get(1).map(|m| m.as_str()).unwrap_or(""));
            let desc = non_empty_string(caps.get(2).map(|m| m.as_str().trim()));
            doc.returns.push(YardReturn::new(types, desc));
        } else if let Some(caps) = YIELD_PARAM_REGEX.captures(line) {
            let name = caps
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            let types = parse_type_list(caps.get(2).map(|m| m.as_str()).unwrap_or(""));
            let desc = non_empty_string(caps.get(3).map(|m| m.as_str().trim()));
            doc.yield_params.push(YardParam::new(name, types, desc));
        } else if let Some(caps) = YIELD_RETURN_REGEX.captures(line) {
            let types = parse_type_list(caps.get(1).map(|m| m.as_str()).unwrap_or(""));
            let desc = non_empty_string(caps.get(2).map(|m| m.as_str().trim()));
            doc.yield_returns.push(YardReturn::new(types, desc));
        } else if let Some(caps) = RAISE_REGEX.captures(line) {
            if let Some(exc) = caps.get(1) {
                doc.raises.push(exc.as_str().to_string());
            }
        } else if let Some(caps) = DEPRECATED_REGEX.captures(line) {
            let reason = non_empty_string(caps.get(1).map(|m| m.as_str().trim()));
            doc.deprecated = Some(reason.unwrap_or_else(|| "Deprecated".to_string()));
        }
        // @example tags are intentionally skipped (multi-line, complex)
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Convert an optional string to None if empty.
fn non_empty_string(s: Option<&str>) -> Option<String> {
    s.filter(|s| !s.is_empty()).map(|s| s.to_string())
}

/// Parse a comma-separated list of types from YARD format
/// Handles nested generic types and both YARD Hash syntaxes:
/// - `Hash<K, V>` (generic style)
/// - `Hash{K => V}` (YARD standard style)
///
/// Examples:
/// - "String, Integer, nil" -> ["String", "Integer", "nil"]
/// - "Hash<Symbol, String>" -> ["Hash<Symbol, String>"]
/// - "Hash{Symbol => String}" -> ["Hash{Symbol => String}"]
/// - "Array<String>, nil" -> ["Array<String>", "nil"]
fn parse_type_list(types_str: &str) -> Vec<String> {
    if types_str.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::new();
    let mut current = String::new();
    let mut angle_depth = 0; // Track nesting level of < >
    let mut brace_depth = 0; // Track nesting level of { }

    for ch in types_str.chars() {
        match ch {
            '<' => {
                angle_depth += 1;
                current.push(ch);
            }
            '>' if angle_depth > 0 => {
                // Only treat > as closing bracket if we're inside angle brackets
                // This prevents `=>` in Hash{K => V} from being misinterpreted
                angle_depth -= 1;
                current.push(ch);
            }
            '{' => {
                brace_depth += 1;
                current.push(ch);
            }
            '}' if brace_depth > 0 => {
                // Only treat } as closing bracket if we're inside braces
                brace_depth -= 1;
                current.push(ch);
            }
            ',' if angle_depth == 0 && brace_depth == 0 => {
                // Only split on comma when not inside angle brackets or braces
                let trimmed = current.trim().to_string();
                if !trimmed.is_empty() {
                    result.push(trimmed);
                }
                current.clear();
            }
            _ => {
                current.push(ch);
            }
        }
    }

    // Don't forget the last type
    let trimmed = current.trim().to_string();
    if !trimmed.is_empty() {
        result.push(trimmed);
    }

    result
}

#[cfg(test)]
mod tests {
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
}
