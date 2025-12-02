//! # TrackedCodeV2
//!
//! Generated code with tracked expectations using the Graph Growth strategy.
//! Stores unique names and anchors, resolving positions dynamically using
//! `SourceLocator` during verification.

use super::source_locator::SourceLocator;
use super::state::GeneratorState;
use std::collections::HashMap;
use tower_lsp::lsp_types::{Position, Range};

/// Generated code with tracked expectations using the Graph Growth strategy.
///
/// Unlike legacy position-based tracking, this version stores unique names
/// and anchors. Positions are resolved dynamically using `SourceLocator`
/// during verification, making it robust to edits.
#[derive(Debug, Clone)]
pub struct TrackedCodeV2 {
    /// The generated Ruby source code
    pub code: String,
    /// The generator state with all pools and ledgers
    pub state: GeneratorState,
    /// Suggested filename
    pub filename: String,
    /// Count of edits applied (for debugging)
    pub edit_count: u32,
}

impl TrackedCodeV2 {
    /// Create a new TrackedCodeV2 from a generator state
    pub fn from_state(state: GeneratorState, filename: String) -> Self {
        let code = state.build_source();
        Self {
            code,
            state,
            filename,
            edit_count: 0,
        }
    }

    /// Get a SourceLocator for the current code
    pub fn locator(&self) -> SourceLocator<'_> {
        SourceLocator::new(&self.code)
    }

    /// Verify all reference anchors (returns list of failures)
    pub fn verify_references(
        &self,
        definition_resolver: impl Fn(&str, Position) -> Option<Position>,
    ) -> Vec<String> {
        let mut failures = Vec::new();
        let locator = self.locator();

        for (anchor_id, target_name) in &self.state.ref_ledger.anchors {
            // Find where the anchor is
            let Some(usage_pos) = locator.find_anchor(anchor_id) else {
                failures.push(format!("Anchor {} not found in source", anchor_id));
                continue;
            };

            // Find where the target definition is
            let Some(def_pos) = locator.find_token(target_name) else {
                failures.push(format!(
                    "Target {} for anchor {} not found",
                    target_name, anchor_id
                ));
                continue;
            };

            // Ask the LSP for definition at the usage position
            let Some(resolved_pos) = definition_resolver(&self.filename, usage_pos) else {
                failures.push(format!(
                    "No definition found for anchor {} (expected {})",
                    anchor_id, target_name
                ));
                continue;
            };

            // Check if resolved position matches expected definition
            // Allow some tolerance for line differences
            let line_diff = (resolved_pos.line as i32 - def_pos.line as i32).abs();
            if line_diff > 2 {
                failures.push(format!(
                    "Anchor {} resolved to line {} but {} is at line {}",
                    anchor_id, resolved_pos.line, target_name, def_pos.line
                ));
            }
        }

        failures
    }

    /// Get all defined class names
    pub fn classes(&self) -> &[String] {
        &self.state.classes
    }

    /// Get all defined module names
    pub fn modules(&self) -> &[String] {
        &self.state.modules
    }

    /// Get all expected completions for verification
    pub fn expected_completions(&self) -> &HashMap<String, Vec<String>> {
        &self.state.completion_ledger.expected_completions
    }

    /// Get all expected types for verification
    pub fn expected_types(&self) -> &HashMap<String, String> {
        &self.state.type_ledger.var_types
    }

    /// Apply an edit to the code (updates the source string)
    pub fn apply_edit(&mut self, range: &Range, new_text: &str) -> bool {
        let new_code = apply_edit_to_code(&self.code, range, new_text);
        self.code = new_code;
        self.edit_count += 1;
        true
    }

    /// Find a safe line for editing (no definitions on this line)
    pub fn find_safe_edit_line(&self) -> Option<u32> {
        let line_count = self.code.lines().count() as u32;

        // Find lines that don't have any of our unique identifiers
        for line in 0..line_count {
            let line_content = self.code.lines().nth(line as usize)?;

            // Skip lines with class/module definitions or method definitions
            if line_content.contains("class ")
                || line_content.contains("module ")
                || line_content.contains("def ")
                || line_content.contains("end")
            {
                continue;
            }

            // Skip lines with our generated identifiers
            let has_identifier = self.state.classes.iter().any(|c| line_content.contains(c))
                || self.state.modules.iter().any(|m| line_content.contains(m));

            if !has_identifier {
                return Some(line);
            }
        }

        // Default to appending at the end
        Some(line_count.saturating_sub(1))
    }
}

/// Apply an edit to a code string and return the result
pub fn apply_edit_to_code(code: &str, range: &Range, new_text: &str) -> String {
    let start_offset = position_to_byte_offset(code, &range.start);
    let end_offset = position_to_byte_offset(code, &range.end);

    let mut result = String::new();
    result.push_str(&code[..start_offset]);
    result.push_str(new_text);
    result.push_str(&code[end_offset..]);
    result
}

/// Convert a Position to a byte offset in the string
fn position_to_byte_offset(content: &str, position: &Position) -> usize {
    let mut offset = 0;
    for (line_num, line) in content.lines().enumerate() {
        if line_num == position.line as usize {
            let char_offset = (position.character as usize).min(line.len());
            return offset + char_offset;
        }
        offset += line.len() + 1; // +1 for newline
    }
    // If position is beyond content, return end
    content.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tracked_code_v2_creation() {
        let mut state = GeneratorState::new();

        // Create a simple class
        let class_name = state.make_class_name(); // Class_0
        state.define_base_class(&class_name);

        // Create a reference to it
        let (ref_code, anchor) = state.make_class_reference(&class_name);
        state.emit(&format!("_ = {}", ref_code));

        let v2 = TrackedCodeV2::from_state(state, "test.rb".to_string());

        // Verify we can find things via SourceLocator
        let locator = v2.locator();
        assert!(locator.find_token("Class_0").is_some());
        assert!(locator.find_anchor(&anchor).is_some());

        // Verify ledgers have entries
        assert!(!v2.state.ref_ledger.anchors.is_empty());
    }

    #[test]
    fn test_apply_edit() {
        let mut state = GeneratorState::new();
        let class_name = state.make_class_name();
        state.define_base_class(&class_name);

        let mut v2 = TrackedCodeV2::from_state(state, "test.rb".to_string());
        let original_len = v2.code.len();

        // Apply an edit
        let range = Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 0,
                character: 0,
            },
        };
        v2.apply_edit(&range, "# comment\n");

        assert!(v2.code.len() > original_len);
        assert_eq!(v2.edit_count, 1);
    }
}
