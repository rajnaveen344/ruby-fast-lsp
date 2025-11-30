//! # Transitions (User Actions)
//!
//! All possible LSP operations that can be simulated.
//! proptest generates random sequences of these.

use tower_lsp::lsp_types::{Position, Range};

/// All possible LSP operations that can be simulated.
///
/// These represent the "moves" a user can make. The simulation
/// generates random sequences of these and verifies invariants hold.
#[derive(Debug, Clone)]
pub enum Transition {
    // === Document Lifecycle (Mutations) ===
    /// Open a file with content
    DidOpen { filename: String, content: String },

    /// Edit an open file
    DidChange {
        filename: String,
        range: Range,
        new_text: String,
    },

    /// Save a file (no content change, but triggers events)
    DidSave { filename: String },

    /// Close a file
    DidClose { filename: String },

    // === Navigation Queries (Read-only) ===
    /// Go to definition at position
    GotoDefinition {
        filename: String,
        position: Position,
    },

    /// Find all references at position
    FindReferences {
        filename: String,
        position: Position,
        include_declaration: bool,
    },

    // === Intelligence Queries (Read-only) ===
    /// Get completion suggestions
    Completion {
        filename: String,
        position: Position,
    },

    /// Get hover information
    Hover {
        filename: String,
        position: Position,
    },

    /// Get inlay hints for a range
    InlayHints { filename: String, range: Range },

    /// Get semantic tokens for a file
    SemanticTokens { filename: String },

    // === Document Structure Queries (Read-only) ===
    /// Get document symbols
    DocumentSymbols { filename: String },

    /// Search workspace symbols
    WorkspaceSymbols { query: String },

    /// Get folding ranges
    FoldingRange { filename: String },

    /// Get code lens annotations
    CodeLens { filename: String },

    // === Formatting ===
    /// On-type formatting (triggered by keystroke)
    OnTypeFormatting {
        filename: String,
        position: Position,
        character: char,
    },
}

impl Transition {
    /// Returns true if this transition modifies state (vs read-only query)
    pub fn is_mutation(&self) -> bool {
        matches!(
            self,
            Transition::DidOpen { .. }
                | Transition::DidChange { .. }
                | Transition::DidSave { .. }
                | Transition::DidClose { .. }
        )
    }

    /// Returns the filename this transition operates on (if any)
    pub fn filename(&self) -> Option<&str> {
        match self {
            Transition::DidOpen { filename, .. }
            | Transition::DidChange { filename, .. }
            | Transition::DidSave { filename }
            | Transition::DidClose { filename }
            | Transition::GotoDefinition { filename, .. }
            | Transition::FindReferences { filename, .. }
            | Transition::Completion { filename, .. }
            | Transition::Hover { filename, .. }
            | Transition::InlayHints { filename, .. }
            | Transition::SemanticTokens { filename }
            | Transition::DocumentSymbols { filename }
            | Transition::FoldingRange { filename }
            | Transition::CodeLens { filename }
            | Transition::OnTypeFormatting { filename, .. } => Some(filename),
            Transition::WorkspaceSymbols { .. } => None,
        }
    }

    /// Human-readable description for logging
    pub fn description(&self) -> String {
        match self {
            Transition::DidOpen { filename, content } => {
                format!("DidOpen({}, {} bytes)", filename, content.len())
            }
            Transition::DidChange {
                filename,
                range,
                new_text,
            } => {
                format!(
                    "DidChange({}, {}:{}-{}:{}, {} chars)",
                    filename,
                    range.start.line,
                    range.start.character,
                    range.end.line,
                    range.end.character,
                    new_text.len()
                )
            }
            Transition::DidSave { filename } => format!("DidSave({})", filename),
            Transition::DidClose { filename } => format!("DidClose({})", filename),
            Transition::GotoDefinition { filename, position } => {
                format!(
                    "GotoDefinition({}, {}:{})",
                    filename, position.line, position.character
                )
            }
            Transition::FindReferences {
                filename, position, ..
            } => {
                format!(
                    "FindReferences({}, {}:{})",
                    filename, position.line, position.character
                )
            }
            Transition::Completion { filename, position } => {
                format!(
                    "Completion({}, {}:{})",
                    filename, position.line, position.character
                )
            }
            Transition::Hover { filename, position } => {
                format!(
                    "Hover({}, {}:{})",
                    filename, position.line, position.character
                )
            }
            Transition::InlayHints { filename, .. } => format!("InlayHints({})", filename),
            Transition::SemanticTokens { filename } => format!("SemanticTokens({})", filename),
            Transition::DocumentSymbols { filename } => format!("DocumentSymbols({})", filename),
            Transition::WorkspaceSymbols { query } => format!("WorkspaceSymbols({})", query),
            Transition::FoldingRange { filename } => format!("FoldingRange({})", filename),
            Transition::CodeLens { filename } => format!("CodeLens({})", filename),
            Transition::OnTypeFormatting {
                filename,
                character,
                ..
            } => {
                format!("OnTypeFormatting({}, '{}')", filename, character)
            }
        }
    }
}
