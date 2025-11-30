//! # Generators
//!
//! Proptest strategies for generating Ruby content, valid edits,
//! positions, and transition sequences.

use super::{LspModel, Transition};
use proptest::prelude::*;
use tower_lsp::lsp_types::{Position, Range};

// =============================================================================
// Ruby Content Generators
// =============================================================================

/// Ruby keywords to avoid in identifier generation
const RUBY_KEYWORDS: &[&str] = &[
    "def",
    "class",
    "module",
    "end",
    "if",
    "else",
    "elsif",
    "unless",
    "case",
    "when",
    "while",
    "until",
    "for",
    "do",
    "begin",
    "rescue",
    "ensure",
    "raise",
    "return",
    "yield",
    "super",
    "self",
    "nil",
    "true",
    "false",
    "and",
    "or",
    "not",
    "in",
    "then",
    "alias",
    "defined",
    "BEGIN",
    "END",
    "__FILE__",
    "__LINE__",
    "__ENCODING__",
];

/// Generate a valid Ruby identifier (snake_case)
pub fn ruby_identifier() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,15}".prop_filter("not a keyword", |s| !RUBY_KEYWORDS.contains(&s.as_str()))
}

/// Generate a valid Ruby class/module name (PascalCase)
pub fn ruby_class_name() -> impl Strategy<Value = String> {
    "[A-Z][a-z]{2,10}".prop_filter("not a keyword", |s| !RUBY_KEYWORDS.contains(&s.as_str()))
}

/// Generate a valid Ruby filename
pub fn ruby_filename() -> impl Strategy<Value = String> {
    ruby_identifier().prop_map(|name| format!("{}.rb", name))
}

/// Generate a simple Ruby method definition
pub fn ruby_method() -> impl Strategy<Value = String> {
    (
        ruby_identifier(),
        prop::collection::vec(ruby_identifier(), 0..3),
    )
        .prop_map(|(name, params)| {
            let params_str = params.join(", ");
            format!("  def {}({})\n    nil\n  end", name, params_str)
        })
}

/// Generate a simple Ruby class
pub fn ruby_class() -> impl Strategy<Value = String> {
    (
        ruby_class_name(),
        prop::option::of(ruby_class_name()),
        prop::collection::vec(ruby_method(), 0..3),
    )
        .prop_map(|(name, superclass, methods)| {
            let extends = superclass.map(|s| format!(" < {}", s)).unwrap_or_default();
            let body = methods.join("\n\n");
            format!("class {}{}\n{}\nend", name, extends, body)
        })
}

/// Generate a simple Ruby module
pub fn ruby_module() -> impl Strategy<Value = String> {
    (
        ruby_class_name(),
        prop::collection::vec(ruby_method(), 0..3),
    )
        .prop_map(|(name, methods)| {
            let body = methods.join("\n\n");
            format!("module {}\n{}\nend", name, body)
        })
}

/// Generate random Ruby content (may have syntax errors - that's OK for fuzzing!)
pub fn ruby_content() -> impl Strategy<Value = String> {
    prop_oneof![
        // Valid class
        ruby_class(),
        // Valid module
        ruby_module(),
        // Simple valid Ruby
        ruby_identifier().prop_map(|name| format!("# comment\n{} = 42", name)),
        // Empty file
        Just("".to_string()),
        // Just a comment
        "[a-z ]{0,50}".prop_map(|text| format!("# {}", text)),
        // Potentially invalid Ruby (tests error recovery)
        "[a-z{}()\\[\\]<>\\n ]{0,100}",
    ]
}

// =============================================================================
// Position and Range Generators
// =============================================================================

/// Generate a random position (may be invalid - will be clamped)
pub fn random_position() -> impl Strategy<Value = Position> {
    (0..100u32, 0..200u32).prop_map(|(line, character)| Position { line, character })
}

/// Generate a random range (start <= end)
pub fn random_range() -> impl Strategy<Value = Range> {
    (random_position(), random_position()).prop_map(|(a, b)| {
        if a.line < b.line || (a.line == b.line && a.character <= b.character) {
            Range { start: a, end: b }
        } else {
            Range { start: b, end: a }
        }
    })
}

/// Generate a valid position within the given content
pub fn valid_position_for(content: &str) -> impl Strategy<Value = Position> {
    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let line_count = lines.len().max(1);

    (0..line_count).prop_flat_map(move |line| {
        let line_len = lines.get(line).map(|l| l.len()).unwrap_or(0).max(1);
        (Just(line), 0..=line_len).prop_map(|(l, c)| Position {
            line: l as u32,
            character: c as u32,
        })
    })
}

/// Generate a valid edit range and replacement text for the given content
pub fn valid_edit_for(content: &str) -> impl Strategy<Value = (Range, String)> {
    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let line_count = lines.len().max(1);

    (0..line_count, 0..line_count)
        .prop_flat_map(move |(start_line, end_line)| {
            let (start_line, end_line) = if start_line <= end_line {
                (start_line, end_line)
            } else {
                (end_line, start_line)
            };

            let start_line_len = lines.get(start_line).map(|l| l.len()).unwrap_or(0);
            let end_line_len = lines.get(end_line).map(|l| l.len()).unwrap_or(0);

            (
                Just(start_line),
                0..=start_line_len,
                Just(end_line),
                0..=end_line_len,
                "[a-z \\n]{0,30}", // replacement text
            )
        })
        .prop_map(|(sl, sc, el, ec, text)| {
            (
                Range {
                    start: Position {
                        line: sl as u32,
                        character: sc as u32,
                    },
                    end: Position {
                        line: el as u32,
                        character: ec as u32,
                    },
                },
                text,
            )
        })
}

// =============================================================================
// Transition Generators
// =============================================================================

/// Generate a transition that requires no open files
pub fn initial_transition() -> impl Strategy<Value = Transition> {
    (ruby_filename(), ruby_content())
        .prop_map(|(filename, content)| Transition::DidOpen { filename, content })
}

/// Generate a transition given the current model state
pub fn transition_for_state(model: &LspModel) -> BoxedStrategy<Transition> {
    let open_files = model.open_files();

    if open_files.is_empty() {
        // Must open a file first
        initial_transition().boxed()
    } else {
        // Clone data needed for the strategy
        let files_for_open = open_files.clone();
        let files_for_edit: Vec<(String, String)> = open_files
            .iter()
            .filter_map(|f| model.get_content(f).map(|c| (f.clone(), c.to_string())))
            .collect();
        let files_for_close = open_files.clone();
        let files_for_queries = open_files.clone();

        prop_oneof![
            // === Document Lifecycle (30% weight) ===

            // Open new file (10%)
            10 => (ruby_filename(), ruby_content())
                .prop_map(|(filename, content)| Transition::DidOpen { filename, content }),

            // Edit existing file - THE CRITICAL TEST (15%)
            15 => {
                if files_for_edit.is_empty() {
                    initial_transition().boxed()
                } else {
                    prop::sample::select(files_for_edit)
                        .prop_flat_map(|(filename, content)| {
                            valid_edit_for(&content)
                                .prop_map(move |(range, new_text)| Transition::DidChange {
                                    filename: filename.clone(),
                                    range,
                                    new_text,
                                })
                        })
                        .boxed()
                }
            },

            // Save file (2%)
            2 => prop::sample::select(files_for_open.clone())
                .prop_map(|filename| Transition::DidSave { filename }),

            // Close file (3%)
            3 => prop::sample::select(files_for_close)
                .prop_map(|filename| Transition::DidClose { filename }),

            // === Navigation Queries (20%) ===

            // Go to definition (10%)
            10 => prop::sample::select(files_for_queries.clone())
                .prop_flat_map(|filename| {
                    random_position().prop_map(move |position| Transition::GotoDefinition {
                        filename: filename.clone(),
                        position,
                    })
                }),

            // Find references (10%)
            10 => prop::sample::select(files_for_queries.clone())
                .prop_flat_map(|filename| {
                    (random_position(), any::<bool>()).prop_map(move |(position, include_declaration)| {
                        Transition::FindReferences {
                            filename: filename.clone(),
                            position,
                            include_declaration,
                        }
                    })
                }),

            // === Intelligence Queries (25%) ===

            // Completion (10%)
            10 => prop::sample::select(files_for_queries.clone())
                .prop_flat_map(|filename| {
                    random_position().prop_map(move |position| Transition::Completion {
                        filename: filename.clone(),
                        position,
                    })
                }),

            // Hover (5%)
            5 => prop::sample::select(files_for_queries.clone())
                .prop_flat_map(|filename| {
                    random_position().prop_map(move |position| Transition::Hover {
                        filename: filename.clone(),
                        position,
                    })
                }),

            // Inlay hints (5%)
            5 => prop::sample::select(files_for_queries.clone())
                .prop_flat_map(|filename| {
                    random_range().prop_map(move |range| Transition::InlayHints {
                        filename: filename.clone(),
                        range,
                    })
                }),

            // Semantic tokens (5%)
            5 => prop::sample::select(files_for_queries.clone())
                .prop_map(|filename| Transition::SemanticTokens { filename }),

            // === Document Structure (20%) ===

            // Document symbols (8%)
            8 => prop::sample::select(files_for_queries.clone())
                .prop_map(|filename| Transition::DocumentSymbols { filename }),

            // Workspace symbols (4%)
            4 => "[a-zA-Z]{0,10}".prop_map(|query| Transition::WorkspaceSymbols { query }),

            // Folding ranges (4%)
            4 => prop::sample::select(files_for_queries.clone())
                .prop_map(|filename| Transition::FoldingRange { filename }),

            // Code lens (4%)
            4 => prop::sample::select(files_for_queries.clone())
                .prop_map(|filename| Transition::CodeLens { filename }),

            // === Formatting (5%) ===

            // On-type formatting (5%)
            5 => prop::sample::select(files_for_queries)
                .prop_flat_map(|filename| {
                    (random_position(), prop_oneof![Just('\n'), Just('d'), Just('e')])
                        .prop_map(move |(position, character)| Transition::OnTypeFormatting {
                            filename: filename.clone(),
                            position,
                            character,
                        })
                }),
        ]
        .boxed()
    }
}
