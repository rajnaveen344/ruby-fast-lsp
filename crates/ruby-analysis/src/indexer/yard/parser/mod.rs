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
use crate::core::{SourcePosition as Position, SourceRange as Range};
use log::debug;
use regex::Regex;
use std::sync::LazyLock;

// =============================================================================
// Regex Patterns
// =============================================================================

/// @param name [Type] description (standard YARD format)
/// Groups: 1=name, 2=type, 3=description
static PARAM_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@param\s+(\w+)\s+\[([^\]]+)\]\s*(.*)").expect("Invalid param regex")
});

/// @param[Type] name description (alternative format - type before name)
/// Groups: 1=type, 2=name, 3=description
static PARAM_ALT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"@param\s*\[([^\]]+)\]\s*(\w+)\s*(.*)").expect("Invalid param alt regex")
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
static UNAVAILABLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"@unavailable\s*(.*)").expect("Invalid unavailable regex"));
static ABSENT_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"@absent\s*(.*)").expect("Invalid absent regex"));

// =============================================================================
// Helper Types
// =============================================================================

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
        assert!(
            method_start_offset <= content.len() && content.is_char_boundary(method_start_offset),
            "INVARIANT VIOLATED: YARD method offset is outside the source or not on a UTF-8 boundary. This is a bug because parser locations must be valid source byte offsets. Fix: pass the exact method location from the parse tree."
        );
        let method_start_line = u32::try_from(
            content[..method_start_offset]
                .bytes()
                .filter(|byte| *byte == b'\n')
                .count(),
        )
        .expect(
            "INVARIANT VIOLATED: YARD method line exceeded u32. This is a bug because editor protocol positions use u32 line numbers. Fix: reject or segment files with more than u32::MAX lines.",
        );
        Self::extract_from_source_at_line(content, method_start_offset, method_start_line)
    }

    /// Extract YARD documentation when the caller already owns the method's
    /// zero-indexed line. This avoids rebuilding every preceding source line
    /// for each method in a file and visits only the attached comment block.
    pub fn extract_from_source_at_line(
        content: &str,
        method_start_offset: usize,
        method_start_line: u32,
    ) -> Option<YardMethodDoc> {
        assert!(
            method_start_offset <= content.len() && content.is_char_boundary(method_start_offset),
            "INVARIANT VIOLATED: YARD method offset is outside the source or not on a UTF-8 boundary. This is a bug because parser locations must be valid source byte offsets. Fix: pass the exact method location from the parse tree."
        );
        let content_before = &content[..method_start_offset];
        if content_before.is_empty() {
            return None;
        }

        let comment_lines = Self::collect_preceding_comments(content_before, method_start_line);
        if comment_lines.is_empty() {
            return None;
        }

        let doc = Self::parse_lines(&comment_lines, true);

        if doc.has_type_info()
            || doc.description.is_some()
            || doc.deprecated.is_some()
            || doc.unavailable.is_some()
            || doc.absent.is_some()
            || !doc.raises.is_empty()
        {
            Some(doc)
        } else {
            None
        }
    }

    /// Collect comment lines immediately preceding a method definition.
    fn collect_preceding_comments(
        content_before: &str,
        method_start_line: u32,
    ) -> Vec<CommentLineInfo<'_>> {
        let mut comment_lines = Vec::new();
        let mut line_number = if content_before.ends_with('\n') {
            method_start_line.checked_sub(1).expect(
                "INVARIANT VIOLATED: source before a line-zero method ended with a newline. This is a bug because the supplied method line disagrees with its byte offset. Fix: derive both values from the same parsed source document.",
            )
        } else {
            method_start_line
        };

        for original_line in content_before.lines().rev() {
            let trimmed = original_line.trim();

            if trimmed.starts_with('#') {
                let leading_ws = original_line.len() - original_line.trim_start().len();
                let content = trimmed.trim_start_matches('#').trim_start();
                let hash_and_space = trimmed.len() - content.len();

                comment_lines.push(CommentLineInfo {
                    content,
                    line_number,
                    content_start_char: (leading_ws + hash_and_space) as u32,
                    line_length: original_line.len() as u32,
                });
            } else if !trimmed.is_empty() {
                break;
            }

            if line_number == 0 {
                break;
            }
            line_number -= 1;
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

        // Try standard format: @param name [Type] description
        // Then try alternative format: @param[Type] name description
        let param_match = PARAM_REGEX
            .captures(line)
            .map(|caps| {
                let name = caps
                    .get(1)
                    .map(|m| m.as_str().to_string())
                    .unwrap_or_default();
                let type_match = caps.get(2);
                let types = parse_type_list(type_match.map(|m| m.as_str()).unwrap_or(""));
                let desc = non_empty_string(caps.get(3).map(|m| m.as_str().trim()));
                (name, types, desc, type_match)
            })
            .or_else(|| {
                PARAM_ALT_REGEX.captures(line).map(|caps| {
                    let type_match = caps.get(1);
                    let types = parse_type_list(type_match.map(|m| m.as_str()).unwrap_or(""));
                    let name = caps
                        .get(2)
                        .map(|m| m.as_str().to_string())
                        .unwrap_or_default();
                    let desc = non_empty_string(caps.get(3).map(|m| m.as_str().trim()));
                    (name, types, desc, type_match)
                })
            });

        if let Some((name, types, desc, type_match)) = param_match {
            // Calculate types range (just the [Type] portion)
            let types_range = if track_positions {
                type_match.map(|m| {
                    let bracket_start = m.start().saturating_sub(1);
                    let bracket_end = m.end() + 1;
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

            let types_range = if track_positions {
                caps.get(1).map(|m| {
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
                doc.returns
                    .push(YardReturn::with_ranges(types, desc, r, tr));
            } else {
                doc.returns.push(YardReturn::new(types, desc));
            }
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
        } else if let Some(caps) = UNAVAILABLE_REGEX.captures(line) {
            let reason = non_empty_string(caps.get(1).map(|m| m.as_str().trim()));
            doc.unavailable =
                Some(reason.unwrap_or_else(|| {
                    "This API is unavailable in the selected runtime.".to_string()
                }));
        } else if let Some(caps) = ABSENT_REGEX.captures(line) {
            let reason = non_empty_string(caps.get(1).map(|m| m.as_str().trim()));
            doc.absent =
                Some(reason.unwrap_or_else(|| {
                    "This API is absent from the selected runtime.".to_string()
                }));
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
mod tests;
