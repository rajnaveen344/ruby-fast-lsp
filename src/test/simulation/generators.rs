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

// =============================================================================
// TRACKED GENERATORS - Code with Known Definition/Reference Positions
// =============================================================================

/// A marker representing a known symbol location in generated code
#[derive(Debug, Clone)]
pub struct SymbolMarker {
    /// Name of the symbol
    pub name: String,
    /// Position where this symbol is defined or referenced
    pub position: Position,
    /// What kind of marker this is
    pub kind: MarkerKind,
    /// Expected definition position (for references)
    pub definition_position: Option<Position>,
}

/// The kind of marker
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkerKind {
    /// A definition (class, module, method, variable)
    Definition,
    /// A reference to a definition
    Reference,
    /// A method call
    MethodCall,
    /// A type assignment - variable with known type
    TypeAssignment {
        /// The expected type name (e.g., "String", "Integer")
        expected_type: String,
    },
    /// A completion trigger point (after a dot)
    CompletionTrigger {
        /// Expected method names that should appear in completion
        expected_methods: Vec<String>,
    },
}

/// Generated code with tracked symbol positions
#[derive(Debug, Clone)]
pub struct TrackedCode {
    /// The generated Ruby code
    pub code: String,
    /// All markers in the code
    pub markers: Vec<SymbolMarker>,
    /// Suggested filename
    pub filename: String,
}

impl TrackedCode {
    /// Get all definition markers
    pub fn definitions(&self) -> Vec<&SymbolMarker> {
        self.markers
            .iter()
            .filter(|m| m.kind == MarkerKind::Definition)
            .collect()
    }

    /// Get all reference markers
    pub fn references(&self) -> Vec<&SymbolMarker> {
        self.markers
            .iter()
            .filter(|m| m.kind == MarkerKind::Reference)
            .collect()
    }

    /// Get all method call markers
    pub fn method_calls(&self) -> Vec<&SymbolMarker> {
        self.markers
            .iter()
            .filter(|m| m.kind == MarkerKind::MethodCall)
            .collect()
    }

    /// Get markers for a specific symbol name
    pub fn markers_for(&self, name: &str) -> Vec<&SymbolMarker> {
        self.markers.iter().filter(|m| m.name == name).collect()
    }
}

// =============================================================================
// Simple Tracked Generators
// =============================================================================

/// Generate a class with a method definition and a call to that method
/// Returns tracked positions for both definition and call site
pub fn tracked_class_with_method_call() -> impl Strategy<Value = TrackedCode> {
    (ruby_class_name(), ruby_identifier()).prop_map(|(class_name, method_name)| {
        // Line 0: class ClassName
        // Line 1:   def method_name
        // Line 2:     nil
        // Line 3:   end
        // Line 4:
        // Line 5:   def caller
        // Line 6:     method_name  <- method call here
        // Line 7:   end
        // Line 8: end

        let code = format!(
            "class {}\n  def {}\n    nil\n  end\n\n  def caller\n    {}\n  end\nend",
            class_name, method_name, method_name
        );

        let markers = vec![
            SymbolMarker {
                name: class_name.clone(),
                position: Position {
                    line: 0,
                    character: 6,
                },
                kind: MarkerKind::Definition,
                definition_position: None,
            },
            SymbolMarker {
                name: method_name.clone(),
                position: Position {
                    line: 1,
                    character: 6,
                },
                kind: MarkerKind::Definition,
                definition_position: None,
            },
            SymbolMarker {
                name: method_name.clone(),
                position: Position {
                    line: 6,
                    character: 4,
                },
                kind: MarkerKind::MethodCall,
                definition_position: Some(Position {
                    line: 1,
                    character: 6,
                }),
            },
        ];

        TrackedCode {
            code,
            markers,
            filename: format!("{}.rb", class_name.to_lowercase()),
        }
    })
}

/// Generate a module with a method, included in a class that calls the method
pub fn tracked_mixin_method_call() -> impl Strategy<Value = TrackedCode> {
    (ruby_class_name(), ruby_class_name(), ruby_identifier()).prop_map(
        |(module_name, class_name, method_name)| {
            // Line 0: module ModuleName
            // Line 1:   def method_name
            // Line 2:     "from module"
            // Line 3:   end
            // Line 4: end
            // Line 5:
            // Line 6: class ClassName
            // Line 7:   include ModuleName  <- reference to module
            // Line 8:
            // Line 9:   def use_mixin
            // Line 10:    method_name  <- call to mixin method
            // Line 11:  end
            // Line 12: end

            let code = format!(
                "module {}\n  def {}\n    \"from module\"\n  end\nend\n\nclass {}\n  include {}\n\n  def use_mixin\n    {}\n  end\nend",
                module_name, method_name, class_name, module_name, method_name
            );

            let markers = vec![
                // Module definition
                SymbolMarker {
                    name: module_name.clone(),
                    position: Position { line: 0, character: 7 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Method definition in module
                SymbolMarker {
                    name: method_name.clone(),
                    position: Position { line: 1, character: 6 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Class definition
                SymbolMarker {
                    name: class_name.clone(),
                    position: Position { line: 6, character: 6 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Include statement - reference to module
                SymbolMarker {
                    name: module_name.clone(),
                    position: Position { line: 7, character: 10 },
                    kind: MarkerKind::Reference,
                    definition_position: Some(Position { line: 0, character: 7 }),
                },
                // Method call - should resolve to mixin method
                SymbolMarker {
                    name: method_name.clone(),
                    position: Position { line: 10, character: 4 },
                    kind: MarkerKind::MethodCall,
                    definition_position: Some(Position { line: 1, character: 6 }),
                },
            ];

            TrackedCode {
                code,
                markers,
                filename: format!("{}.rb", class_name.to_lowercase()),
            }
        },
    )
}

/// Generate a class hierarchy with inheritance and method override
pub fn tracked_inheritance() -> impl Strategy<Value = TrackedCode> {
    (ruby_class_name(), ruby_class_name(), ruby_identifier()).prop_map(
        |(parent_name, child_name, method_name)| {
            // Line 0: class ParentName
            // Line 1:   def method_name
            // Line 2:     "parent"
            // Line 3:   end
            // Line 4: end
            // Line 5:
            // Line 6: class ChildName < ParentName  <- reference to parent
            // Line 7:   def method_name  <- override
            // Line 8:     super  <- call to parent method
            // Line 9:   end
            // Line 10: end

            let code = format!(
                "class {}\n  def {}\n    \"parent\"\n  end\nend\n\nclass {} < {}\n  def {}\n    super\n  end\nend",
                parent_name, method_name, child_name, parent_name, method_name
            );

            let markers = vec![
                // Parent class definition
                SymbolMarker {
                    name: parent_name.clone(),
                    position: Position { line: 0, character: 6 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Parent method definition
                SymbolMarker {
                    name: method_name.clone(),
                    position: Position { line: 1, character: 6 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Child class definition
                SymbolMarker {
                    name: child_name.clone(),
                    position: Position {
                        line: 6,
                        character: 6,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Reference to parent in inheritance
                SymbolMarker {
                    name: parent_name.clone(),
                    position: Position {
                        line: 6,
                        character: (9 + child_name.len()) as u32,
                    },
                    kind: MarkerKind::Reference,
                    definition_position: Some(Position { line: 0, character: 6 }),
                },
                // Override method definition
                SymbolMarker {
                    name: method_name.clone(),
                    position: Position { line: 7, character: 6 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
            ];

            TrackedCode {
                code,
                markers,
                filename: format!("{}.rb", child_name.to_lowercase()),
            }
        },
    )
}

/// Generate instance variable definition and usage
pub fn tracked_instance_variable() -> impl Strategy<Value = TrackedCode> {
    (ruby_class_name(), ruby_identifier()).prop_map(|(class_name, var_name)| {
        // Line 0: class ClassName
        // Line 1:   def initialize
        // Line 2:     @var_name = 42  <- definition
        // Line 3:   end
        // Line 4:
        // Line 5:   def reader
        // Line 6:     @var_name  <- reference
        // Line 7:   end
        // Line 8:
        // Line 9:   def writer
        // Line 10:    @var_name = 100  <- another reference
        // Line 11:  end
        // Line 12: end

        let code = format!(
            "class {}\n  def initialize\n    @{} = 42\n  end\n\n  def reader\n    @{}\n  end\n\n  def writer\n    @{} = 100\n  end\nend",
            class_name, var_name, var_name, var_name
        );

        let ivar_name = format!("@{}", var_name);
        let markers = vec![
            // Class definition
            SymbolMarker {
                name: class_name.clone(),
                position: Position { line: 0, character: 6 },
                kind: MarkerKind::Definition,
                definition_position: None,
            },
            // Instance variable definition in initialize
            SymbolMarker {
                name: ivar_name.clone(),
                position: Position { line: 2, character: 4 },
                kind: MarkerKind::Definition,
                definition_position: None,
            },
            // Instance variable reference in reader
            SymbolMarker {
                name: ivar_name.clone(),
                position: Position { line: 6, character: 4 },
                kind: MarkerKind::Reference,
                definition_position: Some(Position { line: 2, character: 4 }),
            },
            // Instance variable reference in writer
            SymbolMarker {
                name: ivar_name.clone(),
                position: Position { line: 10, character: 4 },
                kind: MarkerKind::Reference,
                definition_position: Some(Position { line: 2, character: 4 }),
            },
        ];

        TrackedCode {
            code,
            markers,
            filename: format!("{}.rb", class_name.to_lowercase()),
        }
    })
}

/// Generate nested modules with namespaced constant access
pub fn tracked_nested_constant() -> impl Strategy<Value = TrackedCode> {
    (ruby_class_name(), ruby_class_name(), ruby_class_name()).prop_map(
        |(outer_name, inner_name, const_name)| {
            // Line 0: module OuterName
            // Line 1:   module InnerName
            // Line 2:     CONST_NAME = 42  <- definition
            // Line 3:   end
            // Line 4: end
            // Line 5:
            // Line 6: # Access the constant
            // Line 7: OuterName::InnerName::CONST_NAME  <- reference

            let code = format!(
                "module {}\n  module {}\n    {} = 42\n  end\nend\n\n# Access the constant\n{}::{}::{}",
                outer_name, inner_name, const_name, outer_name, inner_name, const_name
            );

            let markers = vec![
                // Outer module definition
                SymbolMarker {
                    name: outer_name.clone(),
                    position: Position { line: 0, character: 7 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Inner module definition
                SymbolMarker {
                    name: inner_name.clone(),
                    position: Position { line: 1, character: 9 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Constant definition
                SymbolMarker {
                    name: const_name.clone(),
                    position: Position { line: 2, character: 4 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Outer reference in namespaced access
                SymbolMarker {
                    name: outer_name.clone(),
                    position: Position { line: 7, character: 0 },
                    kind: MarkerKind::Reference,
                    definition_position: Some(Position { line: 0, character: 7 }),
                },
                // Inner reference in namespaced access
                SymbolMarker {
                    name: inner_name.clone(),
                    position: Position {
                        line: 7,
                        character: (outer_name.len() + 2) as u32,
                    },
                    kind: MarkerKind::Reference,
                    definition_position: Some(Position { line: 1, character: 9 }),
                },
                // Constant reference
                SymbolMarker {
                    name: const_name.clone(),
                    position: Position {
                        line: 7,
                        character: (outer_name.len() + inner_name.len() + 4) as u32,
                    },
                    kind: MarkerKind::Reference,
                    definition_position: Some(Position { line: 2, character: 4 }),
                },
            ];

            TrackedCode {
                code,
                markers,
                filename: format!("{}.rb", outer_name.to_lowercase()),
            }
        },
    )
}

/// Generate multiple classes in one file with cross-references
pub fn tracked_multi_class() -> impl Strategy<Value = TrackedCode> {
    (
        ruby_class_name(),
        ruby_class_name(),
        ruby_identifier(),
    )
        .prop_map(|(class_a, class_b, method_name)| {
            // Line 0: class ClassA
            // Line 1:   def method_name
            // Line 2:     ClassB.new  <- reference to ClassB
            // Line 3:   end
            // Line 4: end
            // Line 5:
            // Line 6: class ClassB
            // Line 7:   def method_name
            // Line 8:     ClassA.new  <- reference to ClassA
            // Line 9:   end
            // Line 10: end

            let code = format!(
                "class {}\n  def {}\n    {}.new\n  end\nend\n\nclass {}\n  def {}\n    {}.new\n  end\nend",
                class_a, method_name, class_b, class_b, method_name, class_a
            );

            let markers = vec![
                // ClassA definition
                SymbolMarker {
                    name: class_a.clone(),
                    position: Position { line: 0, character: 6 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // ClassA method definition
                SymbolMarker {
                    name: method_name.clone(),
                    position: Position { line: 1, character: 6 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Reference to ClassB in ClassA
                SymbolMarker {
                    name: class_b.clone(),
                    position: Position { line: 2, character: 4 },
                    kind: MarkerKind::Reference,
                    definition_position: Some(Position { line: 6, character: 6 }),
                },
                // ClassB definition
                SymbolMarker {
                    name: class_b.clone(),
                    position: Position { line: 6, character: 6 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // ClassB method definition
                SymbolMarker {
                    name: method_name.clone(),
                    position: Position { line: 7, character: 6 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Reference to ClassA in ClassB
                SymbolMarker {
                    name: class_a.clone(),
                    position: Position { line: 8, character: 4 },
                    kind: MarkerKind::Reference,
                    definition_position: Some(Position { line: 0, character: 6 }),
                },
            ];

            TrackedCode {
                code,
                markers,
                filename: "multi_class.rb".to_string(),
            }
        })
}

/// Generate prepend scenario (method resolution order test)
pub fn tracked_prepend_override() -> impl Strategy<Value = TrackedCode> {
    (ruby_class_name(), ruby_class_name(), ruby_identifier()).prop_map(
        |(module_name, class_name, method_name)| {
            // Line 0: module ModuleName
            // Line 1:   def method_name
            // Line 2:     "from prepended module"
            // Line 3:   end
            // Line 4: end
            // Line 5:
            // Line 6: class ClassName
            // Line 7:   prepend ModuleName  <- prepend (overrides class methods)
            // Line 8:
            // Line 9:   def method_name  <- this is shadowed by prepend
            // Line 10:    "from class"
            // Line 11:  end
            // Line 12:
            // Line 13:  def call_method
            // Line 14:    method_name  <- should resolve to prepended module's method
            // Line 15:  end
            // Line 16: end

            let code = format!(
                "module {}\n  def {}\n    \"from prepended module\"\n  end\nend\n\nclass {}\n  prepend {}\n\n  def {}\n    \"from class\"\n  end\n\n  def call_method\n    {}\n  end\nend",
                module_name, method_name, class_name, module_name, method_name, method_name
            );

            let markers = vec![
                // Module definition
                SymbolMarker {
                    name: module_name.clone(),
                    position: Position { line: 0, character: 7 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Module method definition (the one that wins with prepend)
                SymbolMarker {
                    name: method_name.clone(),
                    position: Position { line: 1, character: 6 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Class definition
                SymbolMarker {
                    name: class_name.clone(),
                    position: Position { line: 6, character: 6 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Prepend reference to module
                SymbolMarker {
                    name: module_name.clone(),
                    position: Position { line: 7, character: 10 },
                    kind: MarkerKind::Reference,
                    definition_position: Some(Position { line: 0, character: 7 }),
                },
                // Class method definition (shadowed by prepend)
                SymbolMarker {
                    name: method_name.clone(),
                    position: Position { line: 9, character: 6 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Method call - should resolve to prepended module's method (line 1)
                // This is the key test for prepend semantics!
                SymbolMarker {
                    name: method_name.clone(),
                    position: Position { line: 14, character: 4 },
                    kind: MarkerKind::MethodCall,
                    // NOTE: With prepend, this should resolve to the module's method, not the class's
                    definition_position: Some(Position { line: 1, character: 6 }),
                },
            ];

            TrackedCode {
                code,
                markers,
                filename: format!("{}.rb", class_name.to_lowercase()),
            }
        },
    )
}

/// Generate extend scenario (class methods from module)
/// NOTE: Method call resolution for extended methods is a KNOWN LIMITATION.
/// We only test structural resolution (module/class refs), not method call resolution.
pub fn tracked_extend() -> impl Strategy<Value = TrackedCode> {
    (ruby_class_name(), ruby_class_name(), ruby_identifier()).prop_map(
        |(module_name, class_name, method_name)| {
            // Line 0: module ModuleName
            // Line 1:   def method_name
            // Line 2:     "class method"
            // Line 3:   end
            // Line 4: end
            // Line 5:
            // Line 6: class ClassName
            // Line 7:   extend ModuleName  <- extend adds as class methods
            // Line 8: end
            // Line 9:
            // Line 10: # Call as class method
            // Line 11: ClassName.method_name  <- class method call (NOT TESTED - known limitation)

            let code = format!(
                "module {}\n  def {}\n    \"class method\"\n  end\nend\n\nclass {}\n  extend {}\nend\n\n# Call as class method\n{}.{}",
                module_name, method_name, class_name, module_name, class_name, method_name
            );

            let markers = vec![
                // Module definition
                SymbolMarker {
                    name: module_name.clone(),
                    position: Position { line: 0, character: 7 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Module method definition
                SymbolMarker {
                    name: method_name.clone(),
                    position: Position { line: 1, character: 6 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Class definition
                SymbolMarker {
                    name: class_name.clone(),
                    position: Position { line: 6, character: 6 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Extend reference to module
                SymbolMarker {
                    name: module_name.clone(),
                    position: Position { line: 7, character: 9 },
                    kind: MarkerKind::Reference,
                    definition_position: Some(Position { line: 0, character: 7 }),
                },
                // Class reference in method call
                SymbolMarker {
                    name: class_name.clone(),
                    position: Position { line: 11, character: 0 },
                    kind: MarkerKind::Reference,
                    definition_position: Some(Position { line: 6, character: 6 }),
                },
                // NOTE: Method call resolution for ClassName.method_name is NOT tested
                // because extend-based class method resolution is a known LSP limitation.
                // When this is fixed, add:
                // SymbolMarker { name: method_name, position: line 11, kind: MethodCall,
                //                definition_position: Some(line 1) }
            ];

            TrackedCode {
                code,
                markers,
                filename: format!("{}.rb", class_name.to_lowercase()),
            }
        },
    )
}

// =============================================================================
// COMPLEX MIXIN GENERATORS (from Section 9 of the documentation)
// =============================================================================

/// Generate diamond inheritance pattern:
///       M_base
///      /      \
///   M_left   M_right
///      \      /
///       C_final
///
/// Tests Ruby's C3 linearization algorithm
pub fn tracked_diamond_mixin() -> impl Strategy<Value = TrackedCode> {
    (
        ruby_class_name(),
        ruby_class_name(),
        ruby_class_name(),
        ruby_class_name(),
        ruby_identifier(),
    )
        .prop_map(|(base, left, right, final_class, method)| {
            // Line 0: module Base
            // Line 1:   def method
            // Line 2:     "from base"
            // Line 3:   end
            // Line 4: end
            // Line 5:
            // Line 6: module Left
            // Line 7:   include Base
            // Line 8: end
            // Line 9:
            // Line 10: module Right
            // Line 11:   include Base
            // Line 12: end
            // Line 13:
            // Line 14: class Final
            // Line 15:   include Left
            // Line 16:   include Right  # Right included LAST, comes first in chain
            // Line 17:
            // Line 18:   def call
            // Line 19:     method  # Should resolve to Base's method
            // Line 20:   end
            // Line 21: end

            let code = format!(
                r#"module {}
  def {}
    "from base"
  end
end

module {}
  include {}
end

module {}
  include {}
end

class {}
  include {}
  include {}

  def call
    {}
  end
end"#,
                base, method, left, base, right, base, final_class, left, right, method
            );

            let markers = vec![
                // Base module definition
                SymbolMarker {
                    name: base.clone(),
                    position: Position {
                        line: 0,
                        character: 7,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Base method definition
                SymbolMarker {
                    name: method.clone(),
                    position: Position {
                        line: 1,
                        character: 6,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Left module definition
                SymbolMarker {
                    name: left.clone(),
                    position: Position {
                        line: 6,
                        character: 7,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Left includes Base
                SymbolMarker {
                    name: base.clone(),
                    position: Position {
                        line: 7,
                        character: 10,
                    },
                    kind: MarkerKind::Reference,
                    definition_position: Some(Position {
                        line: 0,
                        character: 7,
                    }),
                },
                // Right module definition
                SymbolMarker {
                    name: right.clone(),
                    position: Position {
                        line: 10,
                        character: 7,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Right includes Base
                SymbolMarker {
                    name: base.clone(),
                    position: Position {
                        line: 11,
                        character: 10,
                    },
                    kind: MarkerKind::Reference,
                    definition_position: Some(Position {
                        line: 0,
                        character: 7,
                    }),
                },
                // Final class definition
                SymbolMarker {
                    name: final_class.clone(),
                    position: Position {
                        line: 14,
                        character: 6,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Final includes Left
                SymbolMarker {
                    name: left.clone(),
                    position: Position {
                        line: 15,
                        character: 10,
                    },
                    kind: MarkerKind::Reference,
                    definition_position: Some(Position {
                        line: 6,
                        character: 7,
                    }),
                },
                // Final includes Right
                SymbolMarker {
                    name: right.clone(),
                    position: Position {
                        line: 16,
                        character: 10,
                    },
                    kind: MarkerKind::Reference,
                    definition_position: Some(Position {
                        line: 10,
                        character: 7,
                    }),
                },
                // Method call in Final - should resolve to Base's method
                SymbolMarker {
                    name: method.clone(),
                    position: Position {
                        line: 19,
                        character: 4,
                    },
                    kind: MarkerKind::MethodCall,
                    definition_position: Some(Position {
                        line: 1,
                        character: 6,
                    }),
                },
            ];

            TrackedCode {
                code,
                markers,
                filename: format!("{}.rb", final_class.to_lowercase()),
            }
        })
}

/// Generate deep include chain (N levels)
/// M0 (has method) <- M1 <- M2 <- ... <- C_final
pub fn tracked_deep_include_chain() -> impl Strategy<Value = TrackedCode> {
    (
        prop::collection::vec(ruby_class_name(), 3..6), // 3-5 modules deep
        ruby_class_name(),                              // final class
        ruby_identifier(),                              // method name
    )
        .prop_map(|(module_names, final_class, method_name)| {
            let mut code = String::new();
            let mut markers = Vec::new();
            let mut current_line = 0u32;

            // First module has the method
            code.push_str(&format!(
                "module {}\n  def {}\n    \"from deepest\"\n  end\nend\n\n",
                module_names[0], method_name
            ));

            markers.push(SymbolMarker {
                name: module_names[0].clone(),
                position: Position {
                    line: current_line,
                    character: 7,
                },
                kind: MarkerKind::Definition,
                definition_position: None,
            });

            markers.push(SymbolMarker {
                name: method_name.clone(),
                position: Position {
                    line: current_line + 1,
                    character: 6,
                },
                kind: MarkerKind::Definition,
                definition_position: None,
            });

            current_line += 6; // module + method + end + blank

            // Each subsequent module includes the previous
            for i in 1..module_names.len() {
                code.push_str(&format!(
                    "module {}\n  include {}\nend\n\n",
                    module_names[i],
                    module_names[i - 1]
                ));

                markers.push(SymbolMarker {
                    name: module_names[i].clone(),
                    position: Position {
                        line: current_line,
                        character: 7,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                });

                // Include reference to previous module
                let prev_def_line = if i == 1 {
                    0
                } else {
                    (6 + (i - 1) * 4) as u32 - 4
                };
                markers.push(SymbolMarker {
                    name: module_names[i - 1].clone(),
                    position: Position {
                        line: current_line + 1,
                        character: 10,
                    },
                    kind: MarkerKind::Reference,
                    definition_position: Some(Position {
                        line: prev_def_line,
                        character: 7,
                    }),
                });

                current_line += 4; // module + include + end + blank
            }

            // Final class includes the last module
            code.push_str(&format!(
                "class {}\n  include {}\n\n  def call\n    {}\n  end\nend",
                final_class,
                module_names.last().unwrap(),
                method_name
            ));

            markers.push(SymbolMarker {
                name: final_class.clone(),
                position: Position {
                    line: current_line,
                    character: 6,
                },
                kind: MarkerKind::Definition,
                definition_position: None,
            });

            // Include reference to last module
            let last_module_line = current_line - 4;
            markers.push(SymbolMarker {
                name: module_names.last().unwrap().clone(),
                position: Position {
                    line: current_line + 1,
                    character: 10,
                },
                kind: MarkerKind::Reference,
                definition_position: Some(Position {
                    line: last_module_line,
                    character: 7,
                }),
            });

            // Method call - should resolve to deepest module's method
            markers.push(SymbolMarker {
                name: method_name.clone(),
                position: Position {
                    line: current_line + 4,
                    character: 4,
                },
                kind: MarkerKind::MethodCall,
                definition_position: Some(Position {
                    line: 1,
                    character: 6,
                }),
            });

            TrackedCode {
                code,
                markers,
                filename: format!("{}.rb", final_class.to_lowercase()),
            }
        })
}

/// Generate module with known include/extend/prepend counts for CodeLens testing
pub fn tracked_mixin_counts() -> impl Strategy<Value = TrackedCode> {
    (
        ruby_class_name(),                              // module name
        prop::collection::vec(ruby_class_name(), 1..4), // classes that include
        prop::collection::vec(ruby_class_name(), 0..3), // classes that extend
        prop::collection::vec(ruby_class_name(), 0..2), // classes that prepend
    )
        .prop_map(|(module_name, includers, extenders, prependers)| {
            let mut code = String::new();
            let mut markers = Vec::new();
            let mut current_line = 0u32;

            // Module definition
            code.push_str(&format!(
                "module {}\n  def shared_method\n    \"shared\"\n  end\nend\n\n",
                module_name
            ));

            markers.push(SymbolMarker {
                name: module_name.clone(),
                position: Position {
                    line: current_line,
                    character: 7,
                },
                kind: MarkerKind::Definition,
                definition_position: None,
            });

            current_line += 6;

            // Classes that include
            for class_name in &includers {
                code.push_str(&format!(
                    "class {}\n  include {}\nend\n\n",
                    class_name, module_name
                ));

                markers.push(SymbolMarker {
                    name: class_name.clone(),
                    position: Position {
                        line: current_line,
                        character: 6,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                });

                markers.push(SymbolMarker {
                    name: module_name.clone(),
                    position: Position {
                        line: current_line + 1,
                        character: 10,
                    },
                    kind: MarkerKind::Reference,
                    definition_position: Some(Position {
                        line: 0,
                        character: 7,
                    }),
                });

                current_line += 4;
            }

            // Classes that extend
            for class_name in &extenders {
                code.push_str(&format!(
                    "class {}\n  extend {}\nend\n\n",
                    class_name, module_name
                ));

                markers.push(SymbolMarker {
                    name: class_name.clone(),
                    position: Position {
                        line: current_line,
                        character: 6,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                });

                markers.push(SymbolMarker {
                    name: module_name.clone(),
                    position: Position {
                        line: current_line + 1,
                        character: 9,
                    },
                    kind: MarkerKind::Reference,
                    definition_position: Some(Position {
                        line: 0,
                        character: 7,
                    }),
                });

                current_line += 4;
            }

            // Classes that prepend
            for class_name in &prependers {
                code.push_str(&format!(
                    "class {}\n  prepend {}\nend\n\n",
                    class_name, module_name
                ));

                markers.push(SymbolMarker {
                    name: class_name.clone(),
                    position: Position {
                        line: current_line,
                        character: 6,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                });

                markers.push(SymbolMarker {
                    name: module_name.clone(),
                    position: Position {
                        line: current_line + 1,
                        character: 10,
                    },
                    kind: MarkerKind::Reference,
                    definition_position: Some(Position {
                        line: 0,
                        character: 7,
                    }),
                });

                current_line += 4;
            }

            TrackedCode {
                code,
                markers,
                filename: format!("{}.rb", module_name.to_lowercase()),
            }
        })
}

/// Generate code for completion testing through ancestor chain
pub fn tracked_completion_through_mixins() -> impl Strategy<Value = TrackedCode> {
    (
        ruby_class_name(),
        ruby_class_name(),
        ruby_class_name(),
        ruby_identifier(),
        ruby_identifier(),
        ruby_identifier(),
    )
        .prop_map(|(mod1, mod2, class_name, method1, method2, method3)| {
            // Line 0: module Mod1
            // Line 1:   def method1
            // Line 2:     "from mod1"
            // Line 3:   end
            // Line 4: end
            // Line 5:
            // Line 6: module Mod2
            // Line 7:   def method2
            // Line 8:     "from mod2"
            // Line 9:   end
            // Line 10: end
            // Line 11:
            // Line 12: class ClassName
            // Line 13:   include Mod1
            // Line 14:   include Mod2
            // Line 15:
            // Line 16:   def method3
            // Line 17:     self.  # Completion position - should include method1, method2, method3
            // Line 18:   end
            // Line 19: end

            let code = format!(
                r#"module {}
  def {}
    "from mod1"
  end
end

module {}
  def {}
    "from mod2"
  end
end

class {}
  include {}
  include {}

  def {}
    self.
  end
end"#,
                mod1, method1, mod2, method2, class_name, mod1, mod2, method3
            );

            let markers = vec![
                // Mod1 definition
                SymbolMarker {
                    name: mod1.clone(),
                    position: Position {
                        line: 0,
                        character: 7,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // method1 definition
                SymbolMarker {
                    name: method1.clone(),
                    position: Position {
                        line: 1,
                        character: 6,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Mod2 definition
                SymbolMarker {
                    name: mod2.clone(),
                    position: Position {
                        line: 6,
                        character: 7,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // method2 definition
                SymbolMarker {
                    name: method2.clone(),
                    position: Position {
                        line: 7,
                        character: 6,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Class definition
                SymbolMarker {
                    name: class_name.clone(),
                    position: Position {
                        line: 12,
                        character: 6,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // NOTE: Include references are NOT tested for definition resolution
                // because `include ModuleName` → ModuleName definition is a known LSP limitation.
                // When fixed, add:
                // SymbolMarker { name: mod1, position: line 13 char 10, kind: Reference,
                //                definition_position: Some(line 0 char 7) }
                // SymbolMarker { name: mod2, position: line 14 char 10, kind: Reference,
                //                definition_position: Some(line 6 char 7) }
                // method3 definition
                SymbolMarker {
                    name: method3.clone(),
                    position: Position {
                        line: 16,
                        character: 6,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
            ];

            TrackedCode {
                code,
                markers,
                filename: format!("{}.rb", class_name.to_lowercase()),
            }
        })
}

/// Generate edge cases that should not crash the LSP
pub fn tracked_mixin_edge_cases() -> impl Strategy<Value = TrackedCode> {
    prop_oneof![
        // Self-include (should not infinite loop)
        Just(TrackedCode {
            code: "module SelfInclude\n  include SelfInclude\n  def foo; end\nend".to_string(),
            markers: vec![
                SymbolMarker {
                    name: "SelfInclude".to_string(),
                    position: Position { line: 0, character: 7 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                SymbolMarker {
                    name: "SelfInclude".to_string(),
                    position: Position {
                        line: 1,
                        character: 10
                    },
                    kind: MarkerKind::Reference,
                    definition_position: Some(Position { line: 0, character: 7 }),
                },
            ],
            filename: "self_include.rb".to_string(),
        }),
        // Missing module (should not crash)
        Just(TrackedCode {
            code: "class WithMissing\n  include NonExistentModule\n  def foo; end\nend".to_string(),
            markers: vec![
                SymbolMarker {
                    name: "WithMissing".to_string(),
                    position: Position { line: 0, character: 6 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
            ],
            filename: "with_missing.rb".to_string(),
        }),
        // Circular include (should not infinite loop)
        Just(TrackedCode {
            code: "module CircA\n  include CircB\nend\n\nmodule CircB\n  include CircA\nend"
                .to_string(),
            markers: vec![
                SymbolMarker {
                    name: "CircA".to_string(),
                    position: Position { line: 0, character: 7 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                SymbolMarker {
                    name: "CircB".to_string(),
                    position: Position { line: 1, character: 10 },
                    kind: MarkerKind::Reference,
                    definition_position: Some(Position { line: 4, character: 7 }),
                },
                SymbolMarker {
                    name: "CircB".to_string(),
                    position: Position { line: 4, character: 7 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                SymbolMarker {
                    name: "CircA".to_string(),
                    position: Position { line: 5, character: 10 },
                    kind: MarkerKind::Reference,
                    definition_position: Some(Position { line: 0, character: 7 }),
                },
            ],
            filename: "circular.rb".to_string(),
        }),
        // Deeply nested namespace
        Just(TrackedCode {
            code: "module A\n  module B\n    module C\n      def deep_method; end\n    end\n  end\nend\n\nclass UseDeep\n  include A::B::C\nend"
                .to_string(),
            markers: vec![
                SymbolMarker {
                    name: "A".to_string(),
                    position: Position { line: 0, character: 7 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                SymbolMarker {
                    name: "B".to_string(),
                    position: Position { line: 1, character: 9 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                SymbolMarker {
                    name: "C".to_string(),
                    position: Position { line: 2, character: 11 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                SymbolMarker {
                    name: "UseDeep".to_string(),
                    position: Position { line: 8, character: 6 },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
            ],
            filename: "deep_namespace.rb".to_string(),
        }),
    ]
}

// =============================================================================
// TYPE INFERENCE GENERATORS (for testing type stability across edits)
// =============================================================================

/// Generate code with known type assignments for type inference testing
/// Tests that type information survives document edits
pub fn tracked_type_assignments() -> impl Strategy<Value = TrackedCode> {
    (ruby_class_name(), ruby_identifier(), ruby_identifier()).prop_map(
        |(class_name, var1, var2)| {
            // Line 0: class ClassName
            // Line 1:   def example
            // Line 2:     str_var = "hello"      # String
            // Line 3:     int_var = 42           # Integer
            // Line 4:     arr_var = [1, 2, 3]    # Array
            // Line 5:     hash_var = {a: 1}      # Hash
            // Line 6:     propagated = str_var   # String (propagated)
            // Line 7:     str_var.               # Completion trigger
            // Line 8:   end
            // Line 9: end

            let code = format!(
                r#"class {}
  def example
    {} = "hello"
    {} = 42
    arr_var = [1, 2, 3]
    hash_var = {{a: 1}}
    propagated = {}
    {}.
  end
end"#,
                class_name, var1, var2, var1, var1
            );

            let markers = vec![
                // Class definition
                SymbolMarker {
                    name: class_name.clone(),
                    position: Position {
                        line: 0,
                        character: 6,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // String variable
                SymbolMarker {
                    name: var1.clone(),
                    position: Position {
                        line: 2,
                        character: 4,
                    },
                    kind: MarkerKind::TypeAssignment {
                        expected_type: "String".to_string(),
                    },
                    definition_position: None,
                },
                // Integer variable
                SymbolMarker {
                    name: var2.clone(),
                    position: Position {
                        line: 3,
                        character: 4,
                    },
                    kind: MarkerKind::TypeAssignment {
                        expected_type: "Integer".to_string(),
                    },
                    definition_position: None,
                },
                // Array variable
                SymbolMarker {
                    name: "arr_var".to_string(),
                    position: Position {
                        line: 4,
                        character: 4,
                    },
                    kind: MarkerKind::TypeAssignment {
                        expected_type: "Array".to_string(),
                    },
                    definition_position: None,
                },
                // Hash variable
                SymbolMarker {
                    name: "hash_var".to_string(),
                    position: Position {
                        line: 5,
                        character: 4,
                    },
                    kind: MarkerKind::TypeAssignment {
                        expected_type: "Hash".to_string(),
                    },
                    definition_position: None,
                },
                // Propagated type
                SymbolMarker {
                    name: "propagated".to_string(),
                    position: Position {
                        line: 6,
                        character: 4,
                    },
                    kind: MarkerKind::TypeAssignment {
                        expected_type: "String".to_string(),
                    },
                    definition_position: None,
                },
                // Completion trigger after string variable
                SymbolMarker {
                    name: var1.clone(),
                    position: Position {
                        line: 7,
                        character: 4 + var1.len() as u32 + 1,
                    },
                    kind: MarkerKind::CompletionTrigger {
                        expected_methods: vec![
                            "upcase".to_string(),
                            "downcase".to_string(),
                            "length".to_string(),
                            "to_s".to_string(),
                        ],
                    },
                    definition_position: None,
                },
            ];

            TrackedCode {
                code,
                markers,
                filename: format!("{}.rb", class_name.to_lowercase()),
            }
        },
    )
}

/// Generate code testing type narrowing after conditionals
pub fn tracked_type_narrowing() -> impl Strategy<Value = TrackedCode> {
    (ruby_class_name(), ruby_identifier()).prop_map(|(class_name, var_name)| {
        // Line 0: class ClassName
        // Line 1:   def check(value)
        // Line 2:     if value.is_a?(String)
        // Line 3:       value.upcase  # value is String here
        // Line 4:     elsif value.is_a?(Integer)
        // Line 5:       value + 1     # value is Integer here
        // Line 6:     end
        // Line 7:   end
        // Line 8:
        // Line 9:   def nilcheck
        // Line 10:    x = nil
        // Line 11:    x ||= "default"
        // Line 12:    x.             # x is String here
        // Line 13:  end
        // Line 14: end

        let code = format!(
            r#"class {}
  def check({})
    if {}.is_a?(String)
      {}.upcase
    elsif {}.is_a?(Integer)
      {} + 1
    end
  end

  def nilcheck
    x = nil
    x ||= "default"
    x.
  end
end"#,
            class_name, var_name, var_name, var_name, var_name, var_name
        );

        let markers = vec![
            // Class definition
            SymbolMarker {
                name: class_name.clone(),
                position: Position {
                    line: 0,
                    character: 6,
                },
                kind: MarkerKind::Definition,
                definition_position: None,
            },
            // After ||= assignment, x should be String
            SymbolMarker {
                name: "x".to_string(),
                position: Position {
                    line: 12,
                    character: 5,
                },
                kind: MarkerKind::CompletionTrigger {
                    expected_methods: vec![
                        "upcase".to_string(),
                        "downcase".to_string(),
                        "length".to_string(),
                    ],
                },
                definition_position: None,
            },
        ];

        TrackedCode {
            code,
            markers,
            filename: format!("{}.rb", class_name.to_lowercase()),
        }
    })
}

/// Generate code testing type stability across unrelated edits
/// The key test: edit line X, verify type at line Y is unchanged
pub fn tracked_type_stability() -> impl Strategy<Value = TrackedCode> {
    (ruby_class_name(), ruby_identifier(), ruby_identifier()).prop_map(
        |(class_name, method1, method2)| {
            // Line 0: class ClassName
            // Line 1:   def method1
            // Line 2:     str = "hello"
            // Line 3:     str.upcase      # Edit here should NOT affect line 8
            // Line 4:   end
            // Line 5:
            // Line 6:   def method2
            // Line 7:     num = 42
            // Line 8:     num.            # Type should remain Integer even after editing line 3
            // Line 9:   end
            // Line 10: end

            let code = format!(
                r#"class {}
  def {}
    str = "hello"
    str.upcase
  end

  def {}
    num = 42
    num.
  end
end"#,
                class_name, method1, method2
            );

            let markers = vec![
                // Class definition
                SymbolMarker {
                    name: class_name.clone(),
                    position: Position {
                        line: 0,
                        character: 6,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // String variable in method1
                SymbolMarker {
                    name: "str".to_string(),
                    position: Position {
                        line: 2,
                        character: 4,
                    },
                    kind: MarkerKind::TypeAssignment {
                        expected_type: "String".to_string(),
                    },
                    definition_position: None,
                },
                // Integer variable in method2
                SymbolMarker {
                    name: "num".to_string(),
                    position: Position {
                        line: 7,
                        character: 4,
                    },
                    kind: MarkerKind::TypeAssignment {
                        expected_type: "Integer".to_string(),
                    },
                    definition_position: None,
                },
                // Completion trigger for num - should survive edits to method1
                SymbolMarker {
                    name: "num".to_string(),
                    position: Position {
                        line: 8,
                        character: 8,
                    },
                    kind: MarkerKind::CompletionTrigger {
                        expected_methods: vec![
                            "to_s".to_string(),
                            "abs".to_string(),
                            "times".to_string(),
                        ],
                    },
                    definition_position: None,
                },
            ];

            TrackedCode {
                code,
                markers,
                filename: format!("{}.rb", class_name.to_lowercase()),
            }
        },
    )
}

/// Generate code with method chaining for type flow testing
pub fn tracked_method_chain_types() -> impl Strategy<Value = TrackedCode> {
    ruby_class_name().prop_map(|class_name| {
        // Line 0: class ClassName
        // Line 1:   def chain_example
        // Line 2:     result = "hello".upcase.reverse.chars
        // Line 3:     # result should be Array
        // Line 4:     result.
        // Line 5:   end
        // Line 6: end

        let code = format!(
            r#"class {}
  def chain_example
    result = "hello".upcase.reverse.chars
    result.
  end
end"#,
            class_name
        );

        let markers = vec![
            SymbolMarker {
                name: class_name.clone(),
                position: Position {
                    line: 0,
                    character: 6,
                },
                kind: MarkerKind::Definition,
                definition_position: None,
            },
            // result should be Array (from .chars)
            SymbolMarker {
                name: "result".to_string(),
                position: Position {
                    line: 2,
                    character: 4,
                },
                kind: MarkerKind::TypeAssignment {
                    expected_type: "Array".to_string(),
                },
                definition_position: None,
            },
            // Completion for result (Array methods)
            SymbolMarker {
                name: "result".to_string(),
                position: Position {
                    line: 3,
                    character: 11,
                },
                kind: MarkerKind::CompletionTrigger {
                    expected_methods: vec![
                        "each".to_string(),
                        "map".to_string(),
                        "first".to_string(),
                        "length".to_string(),
                    ],
                },
                definition_position: None,
            },
        ];

        TrackedCode {
            code,
            markers,
            filename: format!("{}.rb", class_name.to_lowercase()),
        }
    })
}

/// Generate code testing inlay hint positions
pub fn tracked_inlay_hints() -> impl Strategy<Value = TrackedCode> {
    (ruby_class_name(), ruby_identifier()).prop_map(|(class_name, method_name)| {
        // Inlay hints should appear for local variable assignments

        let code = format!(
            r#"class {}
  def {}
    a = "string"
    b = 123
    c = [1, 2, 3]
    d = {{key: "value"}}
    e = a
  end
end"#,
            class_name, method_name
        );

        let markers = vec![
            SymbolMarker {
                name: class_name.clone(),
                position: Position {
                    line: 0,
                    character: 6,
                },
                kind: MarkerKind::Definition,
                definition_position: None,
            },
            // Each variable should have an inlay hint with its type
            SymbolMarker {
                name: "a".to_string(),
                position: Position {
                    line: 2,
                    character: 4,
                },
                kind: MarkerKind::TypeAssignment {
                    expected_type: "String".to_string(),
                },
                definition_position: None,
            },
            SymbolMarker {
                name: "b".to_string(),
                position: Position {
                    line: 3,
                    character: 4,
                },
                kind: MarkerKind::TypeAssignment {
                    expected_type: "Integer".to_string(),
                },
                definition_position: None,
            },
            SymbolMarker {
                name: "c".to_string(),
                position: Position {
                    line: 4,
                    character: 4,
                },
                kind: MarkerKind::TypeAssignment {
                    expected_type: "Array".to_string(),
                },
                definition_position: None,
            },
            SymbolMarker {
                name: "d".to_string(),
                position: Position {
                    line: 5,
                    character: 4,
                },
                kind: MarkerKind::TypeAssignment {
                    expected_type: "Hash".to_string(),
                },
                definition_position: None,
            },
            SymbolMarker {
                name: "e".to_string(),
                position: Position {
                    line: 6,
                    character: 4,
                },
                kind: MarkerKind::TypeAssignment {
                    expected_type: "String".to_string(),
                },
                definition_position: None,
            },
        ];

        TrackedCode {
            code,
            markers,
            filename: format!("{}.rb", class_name.to_lowercase()),
        }
    })
}

/// Generate any tracked code scenario (including new complex mixin scenarios)
pub fn tracked_code() -> impl Strategy<Value = TrackedCode> {
    prop_oneof![
        // Basic scenarios (higher weight)
        3 => tracked_class_with_method_call(),
        3 => tracked_mixin_method_call(),
        3 => tracked_inheritance(),
        2 => tracked_instance_variable(),
        2 => tracked_nested_constant(),
        2 => tracked_multi_class(),
        2 => tracked_prepend_override(),
        2 => tracked_extend(),
        // Complex mixin scenarios (lower weight - more expensive)
        1 => tracked_diamond_mixin(),
        1 => tracked_deep_include_chain(),
        1 => tracked_mixin_counts(),
        1 => tracked_completion_through_mixins(),
        1 => tracked_mixin_edge_cases(),
        // Type inference scenarios
        2 => tracked_type_assignments(),
        1 => tracked_type_narrowing(),
        2 => tracked_type_stability(),
        1 => tracked_method_chain_types(),
        2 => tracked_inlay_hints(),
    ]
}
