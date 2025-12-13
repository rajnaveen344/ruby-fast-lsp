//! # Safe Edit Generators
//!
//! Generators for edits that can be deterministically tracked without
//! destroying markers. Works with TrackedCodeV2 (Graph Growth strategy).

use super::tracked_v2::TrackedCodeV2;
use proptest::prelude::*;
use tower_lsp::lsp_types::{Position, Range};

// =============================================================================
// SAFE EDIT GENERATORS
// =============================================================================

/// Types of safe edits that won't destroy markers
#[derive(Debug, Clone)]
pub enum SafeEdit {
    /// Insert a blank line at a safe position
    InsertBlankLine { line: u32 },
    /// Insert a comment at a safe position
    InsertComment { line: u32, text: String },
    /// Insert a new method at the end of a class/module (before final `end`)
    InsertMethod {
        before_end_line: u32,
        method_name: String,
    },
    /// Append text to the end of the file
    AppendToFile { text: String },
}

impl SafeEdit {
    /// Convert this safe edit to a Range and new_text for applying
    pub fn to_edit(&self, code: &str) -> (Range, String) {
        match self {
            SafeEdit::InsertBlankLine { line } => {
                // Insert at the beginning of the line
                let insert_line = (*line).min(code.lines().count().saturating_sub(1) as u32);
                (
                    Range {
                        start: Position {
                            line: insert_line,
                            character: 0,
                        },
                        end: Position {
                            line: insert_line,
                            character: 0,
                        },
                    },
                    "\n".to_string(),
                )
            }
            SafeEdit::InsertComment { line, text } => {
                let insert_line = (*line).min(code.lines().count().saturating_sub(1) as u32);
                (
                    Range {
                        start: Position {
                            line: insert_line,
                            character: 0,
                        },
                        end: Position {
                            line: insert_line,
                            character: 0,
                        },
                    },
                    format!("# {}\n", text),
                )
            }
            SafeEdit::InsertMethod {
                before_end_line,
                method_name,
            } => {
                let insert_line =
                    (*before_end_line).min(code.lines().count().saturating_sub(1) as u32);
                (
                    Range {
                        start: Position {
                            line: insert_line,
                            character: 0,
                        },
                        end: Position {
                            line: insert_line,
                            character: 0,
                        },
                    },
                    format!("  def {}\n    nil\n  end\n\n", method_name),
                )
            }
            SafeEdit::AppendToFile { text } => {
                let line_count = code.lines().count() as u32;
                let last_line_len = code.lines().last().map(|l| l.len()).unwrap_or(0) as u32;
                (
                    Range {
                        start: Position {
                            line: line_count.saturating_sub(1),
                            character: last_line_len,
                        },
                        end: Position {
                            line: line_count.saturating_sub(1),
                            character: last_line_len,
                        },
                    },
                    format!("\n{}", text),
                )
            }
        }
    }

    /// Calculate the line delta caused by this edit (for position adjustment validation)
    pub fn line_delta(&self) -> i32 {
        match self {
            SafeEdit::InsertBlankLine { .. } => 1,
            SafeEdit::InsertComment { .. } => 1,
            SafeEdit::InsertMethod { .. } => 4, // def + body + end + blank
            SafeEdit::AppendToFile { text } => (text.matches('\n').count() + 1) as i32,
        }
    }
}

/// Generate a safe edit that inserts a blank line
pub fn safe_edit_blank_line(tracked: &TrackedCodeV2) -> impl Strategy<Value = SafeEdit> {
    let safe_line = tracked.find_safe_edit_line().unwrap_or(0);
    Just(SafeEdit::InsertBlankLine { line: safe_line })
}

/// Generate a safe edit that inserts a comment
pub fn safe_edit_comment(tracked: &TrackedCodeV2) -> impl Strategy<Value = SafeEdit> {
    let safe_line = tracked.find_safe_edit_line().unwrap_or(0);
    "[a-z ]{1,20}".prop_map(move |text| SafeEdit::InsertComment {
        line: safe_line,
        text,
    })
}

/// Generate a safe edit that appends to the file
pub fn safe_edit_append() -> impl Strategy<Value = SafeEdit> {
    prop_oneof![
        Just(SafeEdit::AppendToFile {
            text: "# comment".to_string()
        }),
        Just(SafeEdit::AppendToFile {
            text: "\n# another comment".to_string()
        }),
        Just(SafeEdit::AppendToFile {
            text: "\n\n".to_string()
        }),
    ]
}

/// Generate any safe edit for a given tracked code
pub fn safe_edit_for(tracked: &TrackedCodeV2) -> BoxedStrategy<SafeEdit> {
    let safe_line = tracked.find_safe_edit_line().unwrap_or(0);
    let line_count = tracked.code.lines().count() as u32;

    prop_oneof![
        3 => Just(SafeEdit::InsertBlankLine { line: safe_line }),
        3 => "[a-z ]{1,20}".prop_map(move |text| SafeEdit::InsertComment {
            line: safe_line,
            text,
        }),
        2 => "[a-z_]{3,10}".prop_map(move |method_name| SafeEdit::InsertMethod {
            before_end_line: line_count.saturating_sub(1),
            method_name,
        }),
        2 => safe_edit_append(),
    ]
    .boxed()
}
