//! # Ruby Content Generators
//!
//! Proptest strategies for generating valid Ruby code constructs.

use super::super::{LspModel, Transition};
use proptest::prelude::*;
use tower_lsp::lsp_types::{Position, Range};

// =============================================================================
// Ruby Content Generators
// =============================================================================

/// Ruby keywords to avoid in identifier generation
pub const RUBY_KEYWORDS: &[&str] = &[
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

// =============================================================================
// Method Generators
// =============================================================================

/// Generate a simple instance method definition
pub fn ruby_instance_method() -> impl Strategy<Value = String> {
    (
        ruby_identifier(),
        prop::collection::vec(ruby_identifier(), 0..3),
    )
        .prop_map(|(name, params)| {
            let params_str = params.join(", ");
            format!("  def {}({})\n    nil\n  end", name, params_str)
        })
}

/// Generate a class method definition (def self.foo)
pub fn ruby_class_method() -> impl Strategy<Value = String> {
    (
        ruby_identifier(),
        prop::collection::vec(ruby_identifier(), 0..2),
    )
        .prop_map(|(name, params)| {
            let params_str = params.join(", ");
            format!("  def self.{}({})\n    nil\n  end", name, params_str)
        })
}

/// Generate a method with visibility modifier
pub fn ruby_method_with_visibility() -> impl Strategy<Value = String> {
    (
        prop_oneof![Just("private"), Just("protected"), Just("public")],
        ruby_identifier(),
    )
        .prop_map(|(visibility, name)| format!("  {}\n  def {}\n    nil\n  end", visibility, name))
}

/// Generate attr_reader/attr_writer/attr_accessor
pub fn ruby_attr_accessor() -> impl Strategy<Value = String> {
    (
        prop_oneof![
            Just("attr_reader"),
            Just("attr_writer"),
            Just("attr_accessor")
        ],
        prop::collection::vec(ruby_identifier(), 1..4),
    )
        .prop_map(|(attr_type, names)| {
            let symbols = names
                .iter()
                .map(|n| format!(":{}", n))
                .collect::<Vec<_>>()
                .join(", ");
            format!("  {} {}", attr_type, symbols)
        })
}

/// Generate any kind of method (instance, class, with visibility, attr)
pub fn ruby_method() -> impl Strategy<Value = String> {
    prop_oneof![
        5 => ruby_instance_method(),
        2 => ruby_class_method(),
        2 => ruby_method_with_visibility(),
        1 => ruby_attr_accessor(),
    ]
}

// =============================================================================
// Instance Variable Generators
// =============================================================================

/// Generate instance variable assignment
pub fn ruby_instance_var() -> impl Strategy<Value = String> {
    (
        ruby_identifier(),
        prop_oneof![
            Just("nil"),
            Just("42"),
            Just("\"string\""),
            Just("[]"),
            Just("{}"),
        ],
    )
        .prop_map(|(name, value)| format!("    @{} = {}", name, value))
}

/// Generate class variable assignment
pub fn ruby_class_var() -> impl Strategy<Value = String> {
    (
        ruby_identifier(),
        prop_oneof![Just("0"), Just("[]"), Just("{}"),],
    )
        .prop_map(|(name, value)| format!("  @@{} = {}", name, value))
}

/// Generate constant assignment
pub fn ruby_constant() -> impl Strategy<Value = String> {
    (
        "[A-Z][A-Z0-9_]{0,10}",
        prop_oneof![Just("42"), Just("\"constant\""), Just("[]"), Just("{}"),],
    )
        .prop_map(|(name, value)| format!("  {} = {}", name, value))
}

// =============================================================================
// Mixin Generators
// =============================================================================

/// Generate an include statement
pub fn ruby_include() -> impl Strategy<Value = String> {
    ruby_class_name().prop_map(|name| format!("  include {}", name))
}

/// Generate an extend statement
pub fn ruby_extend() -> impl Strategy<Value = String> {
    ruby_class_name().prop_map(|name| format!("  extend {}", name))
}

/// Generate a prepend statement
pub fn ruby_prepend() -> impl Strategy<Value = String> {
    ruby_class_name().prop_map(|name| format!("  prepend {}", name))
}

/// Generate any mixin statement
pub fn ruby_mixin() -> impl Strategy<Value = String> {
    prop_oneof![
        3 => ruby_include(),
        2 => ruby_extend(),
        1 => ruby_prepend(),
    ]
}

// =============================================================================
// Class Body Generators
// =============================================================================

/// Generate a class body element (method, constant, attr, mixin, etc.)
pub fn ruby_class_body_element() -> impl Strategy<Value = String> {
    prop_oneof![
        5 => ruby_method(),
        2 => ruby_constant(),
        2 => ruby_class_var(),
        3 => ruby_mixin(),
    ]
}

/// Generate a simple Ruby class
pub fn ruby_class() -> impl Strategy<Value = String> {
    (
        ruby_class_name(),
        prop::option::of(ruby_class_name()),
        prop::collection::vec(ruby_class_body_element(), 0..5),
    )
        .prop_map(|(name, superclass, elements)| {
            let extends = superclass.map(|s| format!(" < {}", s)).unwrap_or_default();
            let body = elements.join("\n\n");
            format!("class {}{}\n{}\nend", name, extends, body)
        })
}

/// Generate a simple Ruby module
pub fn ruby_module() -> impl Strategy<Value = String> {
    (
        ruby_class_name(),
        prop::collection::vec(ruby_class_body_element(), 0..4),
    )
        .prop_map(|(name, elements)| {
            let body = elements.join("\n\n");
            format!("module {}\n{}\nend", name, body)
        })
}

// =============================================================================
// Nested Structure Generators
// =============================================================================

/// Generate a nested class inside a module
pub fn ruby_nested_class() -> impl Strategy<Value = String> {
    (
        ruby_class_name(),
        ruby_class_name(),
        prop::collection::vec(ruby_method(), 0..3),
    )
        .prop_map(|(module_name, class_name, methods)| {
            let body = methods.join("\n\n");
            format!(
                "module {}\n  class {}\n{}\n  end\nend",
                module_name,
                class_name,
                body.lines()
                    .map(|l| format!("  {}", l))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        })
}

/// Generate deeply nested modules (A::B::C style)
pub fn ruby_nested_modules() -> impl Strategy<Value = String> {
    (
        prop::collection::vec(ruby_class_name(), 2..4),
        prop::collection::vec(ruby_method(), 0..2),
    )
        .prop_map(|(names, methods)| {
            let body = methods.join("\n\n");
            let mut code = String::new();
            let mut indent = 0;

            for name in &names {
                code.push_str(&"  ".repeat(indent));
                code.push_str(&format!("module {}\n", name));
                indent += 1;
            }

            // Add body with proper indentation
            for line in body.lines() {
                code.push_str(&"  ".repeat(indent));
                code.push_str(line);
                code.push('\n');
            }

            // Close all modules
            for _ in 0..names.len() {
                indent -= 1;
                code.push_str(&"  ".repeat(indent));
                code.push_str("end\n");
            }

            code
        })
}

/// Generate singleton class (class << self)
pub fn ruby_singleton_class() -> impl Strategy<Value = String> {
    (
        ruby_class_name(),
        prop::collection::vec(ruby_instance_method(), 1..3),
    )
        .prop_map(|(class_name, methods)| {
            let singleton_methods = methods
                .iter()
                .map(|m| {
                    m.lines()
                        .map(|l| format!("  {}", l))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .collect::<Vec<_>>()
                .join("\n\n");

            format!(
                "class {}\n  class << self\n{}\n  end\nend",
                class_name, singleton_methods
            )
        })
}

// =============================================================================
// Complex Ruby Constructs
// =============================================================================

/// Generate a class with initialize method and instance variables
pub fn ruby_class_with_initialize() -> impl Strategy<Value = String> {
    (
        ruby_class_name(),
        prop::collection::vec(ruby_identifier(), 1..4),
        prop::collection::vec(ruby_instance_method(), 0..2),
    )
        .prop_map(|(name, ivars, methods)| {
            let params = ivars.join(", ");
            let assignments = ivars
                .iter()
                .map(|v| format!("    @{} = {}", v, v))
                .collect::<Vec<_>>()
                .join("\n");

            let other_methods = methods.join("\n\n");

            format!(
                "class {}\n  def initialize({})\n{}\n  end\n\n{}\nend",
                name, params, assignments, other_methods
            )
        })
}

/// Generate a module with mixins and methods
pub fn ruby_module_with_mixins() -> impl Strategy<Value = String> {
    (
        ruby_class_name(),
        prop::collection::vec(ruby_mixin(), 0..3),
        prop::collection::vec(ruby_method(), 1..4),
    )
        .prop_map(|(name, mixins, methods)| {
            let mixin_lines = mixins.join("\n");
            let method_lines = methods.join("\n\n");
            format!("module {}\n{}\n\n{}\nend", name, mixin_lines, method_lines)
        })
}

/// Generate a class that includes modules
pub fn ruby_class_with_includes() -> impl Strategy<Value = String> {
    (
        ruby_class_name(),
        prop::option::of(ruby_class_name()),
        prop::collection::vec(ruby_class_name(), 1..3),
        prop::collection::vec(ruby_method(), 0..3),
    )
        .prop_map(|(name, superclass, includes, methods)| {
            let extends = superclass.map(|s| format!(" < {}", s)).unwrap_or_default();
            let include_lines = includes
                .iter()
                .map(|m| format!("  include {}", m))
                .collect::<Vec<_>>()
                .join("\n");
            let method_lines = methods.join("\n\n");
            format!(
                "class {}{}\n{}\n\n{}\nend",
                name, extends, include_lines, method_lines
            )
        })
}

/// Generate a module and classes that use it (for mixin testing)
pub fn ruby_mixin_hierarchy() -> impl Strategy<Value = String> {
    (
        ruby_class_name(),
        prop::collection::vec(ruby_identifier(), 1..3),
        prop::collection::vec(ruby_class_name(), 1..3),
    )
        .prop_map(|(module_name, method_names, class_names)| {
            // Generate the module with methods
            let module_methods = method_names
                .iter()
                .map(|m| format!("  def {}\n    \"from {}\"\n  end", m, module_name))
                .collect::<Vec<_>>()
                .join("\n\n");

            let module_code = format!("module {}\n{}\nend\n\n", module_name, module_methods);

            // Generate classes that include the module
            let class_codes = class_names
                .iter()
                .enumerate()
                .map(|(i, class_name)| {
                    let mixin_type = match i % 3 {
                        0 => "include",
                        1 => "extend",
                        _ => "prepend",
                    };
                    format!(
                        "class {}\n  {} {}\nend",
                        class_name, mixin_type, module_name
                    )
                })
                .collect::<Vec<_>>()
                .join("\n\n");

            format!("{}{}", module_code, class_codes)
        })
}

// =============================================================================
// Main Content Generator
// =============================================================================

/// Generate random Ruby content with rich variety
/// This is the main generator used for fuzzing - it produces diverse Ruby code
pub fn ruby_content() -> impl Strategy<Value = String> {
    prop_oneof![
        // Simple structures (40%)
        10 => ruby_class(),
        10 => ruby_module(),

        // Nested structures (20%)
        5 => ruby_nested_class(),
        5 => ruby_nested_modules(),
        5 => ruby_singleton_class(),

        // Complex structures (25%)
        5 => ruby_class_with_initialize(),
        5 => ruby_module_with_mixins(),
        5 => ruby_class_with_includes(),
        5 => ruby_mixin_hierarchy(),

        // Edge cases (15%)
        3 => ruby_identifier().prop_map(|name| format!("# comment\n{} = 42", name)),
        2 => Just("".to_string()),
        2 => "[a-z ]{0,50}".prop_map(|text| format!("# {}", text)),
        3 => "[a-z{}()\\[\\]<>\\n ]{0,100}", // Invalid syntax for error recovery testing
    ]
}

/// Generate specifically valid Ruby content (no intentional syntax errors)
pub fn valid_ruby_content() -> impl Strategy<Value = String> {
    prop_oneof![
        ruby_class(),
        ruby_module(),
        ruby_nested_class(),
        ruby_nested_modules(),
        ruby_singleton_class(),
        ruby_class_with_initialize(),
        ruby_module_with_mixins(),
        ruby_class_with_includes(),
        ruby_mixin_hierarchy(),
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

