//! # Source Locator
//!
//! Dynamic position finding using the "Find, Don't Remember" principle from
//! the Graph Growth strategy.

use tower_lsp::lsp_types::Position;

/// A locator that finds positions dynamically by searching for unique names and anchors.
///
/// This is the core of the "Find, Don't Remember" principle: instead of storing
/// line/column numbers that can become stale after edits, we store unique
/// identifiers and search for them when needed.
///
/// ## Usage
///
/// ```rust
/// let locator = SourceLocator::new(source_code);
///
/// // Find a unique symbol like "Class_42" or "method_17"
/// let class_pos = locator.find_token("Class_42");
///
/// // Find an anchor comment like "# <REF:99>"
/// let ref_pos = locator.find_anchor("REF:99");
/// ```
pub struct SourceLocator<'a> {
    source: &'a str,
    /// Cached line starts for efficient line/column conversion
    line_starts: Vec<usize>,
}

impl<'a> SourceLocator<'a> {
    /// Create a new SourceLocator for the given source code
    pub fn new(source: &'a str) -> Self {
        let mut line_starts = vec![0];
        for (i, c) in source.char_indices() {
            if c == '\n' {
                line_starts.push(i + 1);
            }
        }
        Self {
            source,
            line_starts,
        }
    }

    /// Convert a byte offset to an LSP Position (0-indexed line and column)
    fn byte_to_position(&self, offset: usize) -> Position {
        let offset = offset.min(self.source.len());

        // Binary search to find the line
        let line_idx = match self.line_starts.binary_search(&offset) {
            Ok(idx) => idx,
            Err(idx) => idx.saturating_sub(1),
        };

        let line_start = self.line_starts.get(line_idx).copied().unwrap_or(0);
        let character = (offset - line_start) as u32;

        Position {
            line: line_idx as u32,
            character,
        }
    }

    /// Find the position of a unique token (symbol name like "Class_N" or "var_N")
    ///
    /// Returns the position at the start of the token, or None if not found.
    pub fn find_token(&self, unique_name: &str) -> Option<Position> {
        // Search for the unique name as a word (not as part of another identifier)
        let mut search_start = 0;
        while let Some(rel_offset) = self.source[search_start..].find(unique_name) {
            let offset = search_start + rel_offset;
            let end_offset = offset + unique_name.len();

            // Check if this is a complete token (not part of a larger identifier)
            let char_before = if offset > 0 {
                self.source[..offset].chars().last()
            } else {
                None
            };
            let char_after = self.source[end_offset..].chars().next();

            let valid_start =
                char_before.map_or(true, |c| !c.is_alphanumeric() && c != '_' && c != '@');
            let valid_end = char_after.map_or(true, |c| {
                !c.is_alphanumeric() && c != '_' && c != '!' && c != '?'
            });

            if valid_start && valid_end {
                return Some(self.byte_to_position(offset));
            }

            search_start = end_offset;
        }
        None
    }

    /// Find the position immediately before an anchor comment.
    ///
    /// Anchors have the form `# <ID:N>` where ID is a category and N is a number.
    /// This returns the position of the code immediately preceding the anchor.
    ///
    /// Example: `Class_0.new # <REF:42>` returns position of 'w' in 'new'
    pub fn find_anchor(&self, anchor_id: &str) -> Option<Position> {
        let comment = format!("# <{}>", anchor_id);
        let offset = self.source.find(&comment)?;

        // Find the position just before the comment (skip whitespace before #)
        let mut pos = offset;
        while pos > 0 && self.source[..pos].ends_with(' ') {
            pos -= 1;
        }

        if pos > 0 {
            Some(self.byte_to_position(pos.saturating_sub(1)))
        } else {
            Some(self.byte_to_position(0))
        }
    }

    /// Find the line number containing an anchor (for diagnostics verification)
    pub fn find_anchor_line(&self, anchor_id: &str) -> Option<u32> {
        let comment = format!("# <{}>", anchor_id);
        let offset = self.source.find(&comment)?;
        Some(self.byte_to_position(offset).line)
    }

    /// Get the position at the end of a unique token (for range queries)
    pub fn find_token_end(&self, unique_name: &str) -> Option<Position> {
        let start = self.find_token(unique_name)?;
        Some(Position {
            line: start.line,
            character: start.character + unique_name.len() as u32,
        })
    }

    /// Find the position right after a dot (.) on the same line as an anchor.
    ///
    /// This is specifically for completion testing - we want the position
    /// immediately after `var.` where completion would be triggered.
    pub fn find_completion_position(&self, anchor_id: &str) -> Option<Position> {
        let comment = format!("# <{}>", anchor_id);
        let anchor_offset = self.source.find(&comment)?;
        let anchor_pos = self.byte_to_position(anchor_offset);

        // Get the line content
        let line_content = self.source.lines().nth(anchor_pos.line as usize)?;

        // Find the dot before the anchor on this line
        // The pattern is usually: "var_name. # <ANCHOR>"
        // We want the position right after the dot
        let anchor_col = anchor_pos.character as usize;

        // Search backwards from anchor for the dot
        for i in (0..anchor_col).rev() {
            if line_content.chars().nth(i) == Some('.') {
                // Return position after the dot
                return Some(Position {
                    line: anchor_pos.line,
                    character: (i + 1) as u32,
                });
            }
        }

        // If no dot found, return position just before anchor
        Some(anchor_pos)
    }

    /// Find a token specifically on a given line
    ///
    /// This is used when we have an anchor on a specific line and want to
    /// find the reference target on that same line.
    pub fn find_token_on_line(&self, token: &str, target_line: u32) -> Option<Position> {
        let line_content = self.source.lines().nth(target_line as usize)?;
        // Verify line exists (we don't need the offset for position calculation)
        let _ = self.line_starts.get(target_line as usize)?;

        // Find the token within this line (as a complete word)
        let mut search_start = 0;
        while let Some(rel_offset) = line_content[search_start..].find(token) {
            let char_offset = search_start + rel_offset;
            let end_offset = char_offset + token.len();

            // Check word boundaries
            let char_before = if char_offset > 0 {
                line_content[..char_offset].chars().last()
            } else {
                None
            };
            let char_after = line_content[end_offset..].chars().next();

            let valid_start =
                char_before.map_or(true, |c| !c.is_alphanumeric() && c != '_' && c != '@');
            let valid_end = char_after.map_or(true, |c| {
                !c.is_alphanumeric() && c != '_' && c != '!' && c != '?'
            });

            if valid_start && valid_end {
                return Some(Position {
                    line: target_line,
                    character: char_offset as u32,
                });
            }

            search_start = end_offset;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_token_basic() {
        let source = "class Class_0\n  def method_0\n  end\nend";
        let locator = SourceLocator::new(source);

        let class_pos = locator.find_token("Class_0");
        assert_eq!(
            class_pos,
            Some(Position {
                line: 0,
                character: 6
            })
        );

        let method_pos = locator.find_token("method_0");
        assert_eq!(
            method_pos,
            Some(Position {
                line: 1,
                character: 6
            })
        );
    }

    #[test]
    fn test_find_token_word_boundary() {
        let source = "Class_0 Class_0_extended\nClass_0";
        let locator = SourceLocator::new(source);

        // Should find the first standalone Class_0, not Class_0_extended
        let pos = locator.find_token("Class_0");
        assert_eq!(
            pos,
            Some(Position {
                line: 0,
                character: 0
            })
        );
    }

    #[test]
    fn test_find_token_not_found() {
        let source = "class Foo\nend";
        let locator = SourceLocator::new(source);

        let pos = locator.find_token("Class_0");
        assert_eq!(pos, None);
    }

    #[test]
    fn test_find_anchor() {
        let source = "_ = Class_0.new # <REF:42>\nother line";
        let locator = SourceLocator::new(source);

        let anchor_line = locator.find_anchor_line("REF:42");
        assert_eq!(anchor_line, Some(0));
    }

    #[test]
    fn test_find_token_on_line() {
        let source = "class Class_0\nend\n\n_ = Class_0.new # <REF:1>";
        let locator = SourceLocator::new(source);

        // Class_0 appears on line 0 (definition) and line 3 (reference)
        let pos_line_0 = locator.find_token_on_line("Class_0", 0);
        assert_eq!(
            pos_line_0,
            Some(Position {
                line: 0,
                character: 6
            })
        );

        let pos_line_3 = locator.find_token_on_line("Class_0", 3);
        assert_eq!(
            pos_line_3,
            Some(Position {
                line: 3,
                character: 4
            })
        );

        // Class_0 doesn't appear on line 1
        let pos_line_1 = locator.find_token_on_line("Class_0", 1);
        assert_eq!(pos_line_1, None);
    }
}

