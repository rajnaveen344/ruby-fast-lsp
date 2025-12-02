//! # Generators
//!
//! Proptest strategies for generating Ruby content, valid edits,
//! positions, and transition sequences.
//!
//! ## Graph Growth Strategy
//!
//! This module implements the "Graph Growth" pattern for generating complex Ruby code:
//!
//! 1. **Grow, Don't Write**: Build a DAG of dependencies. New code only references old code.
//! 2. **Find, Don't Remember**: Store unique names, find positions dynamically.
//! 3. **Construction-Based Typing**: Decide types before generating code.
//! 4. **Anchor Everything**: Use anchor comments (`# <REF:42>`) for non-unique items.
//!
//! ## Key Components
//!
//! - `SourceLocator`: Finds positions dynamically by searching for unique names/anchors
//! - `GeneratorState`: Maintains pools of definitions and truth ledgers
//! - `TruthLedger`: Tracks expectations for verification (types, references, hints, errors)

use super::{LspModel, Transition};
use proptest::prelude::*;
use std::collections::HashMap;
use tower_lsp::lsp_types::{Position, Range};

// =============================================================================
// SOURCE LOCATOR - Dynamic Position Finding (Graph Growth Strategy)
// =============================================================================

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

// =============================================================================
// TRUTH LEDGERS - Expectations Tracking (Graph Growth Strategy)
// =============================================================================

/// Tracks expected variable/method types for completion and inlay hint verification.
///
/// ## Example
///
/// When generating `var_42 = "hello"`, we record:
/// `type_ledger.insert("var_42", "String")`
///
/// During verification, we ask the LSP for inlay hints and check if
/// the hint at `var_42` contains "String".
#[derive(Debug, Clone, Default)]
pub struct TypeLedger {
    /// Variable name -> expected type (e.g., "var_42" -> "String")
    pub var_types: HashMap<String, String>,
    /// Method name -> expected return type
    pub method_returns: HashMap<String, String>,
}

/// Tracks reference anchors for go-to-definition verification.
///
/// ## Example
///
/// When generating `_ = Class_0.new # <REF:42>`, we record:
/// `ref_ledger.insert("REF:42", "Class_0")`
///
/// During verification:
/// 1. Find position of `# <REF:42>` anchor
/// 2. Ask LSP for "Definition" at that position
/// 3. Verify it jumps to where `Class_0` is defined
#[derive(Debug, Clone, Default)]
pub struct ReferenceLedger {
    /// Anchor ID -> target definition name (e.g., "REF:42" -> "Class_0")
    pub anchors: HashMap<String, String>,
}

/// Tracks expected inlay hints.
///
/// ## Example
///
/// When generating `x = Class_0.new`, we record:
/// `hint_ledger.insert("x", "Class_0")`
#[derive(Debug, Clone, Default)]
pub struct HintLedger {
    /// Variable/expression name -> expected hint text
    pub hints: HashMap<String, String>,
}

/// Tracks expected diagnostic errors (the "Saboteur" pattern).
///
/// ## Example
///
/// When generating `1 + "string" # <ERR:99>`, we record:
/// `error_ledger.insert("ERR:99", "TypeError")`
#[derive(Debug, Clone, Default)]
pub struct ErrorLedger {
    /// Anchor ID -> expected error type
    pub errors: HashMap<String, String>,
}

/// Tracks expected completion items for a given position.
///
/// ## Example
///
/// When generating a class with methods, we track what completions should appear:
/// `completion_ledger.insert("COMP:1", vec!["method_0", "method_1"])`
#[derive(Debug, Clone, Default)]
pub struct CompletionLedger {
    /// Anchor ID -> expected completion items
    pub expected_completions: HashMap<String, Vec<String>>,
}

// =============================================================================
// GENERATOR STATE - Pools and Ledgers (Graph Growth Strategy)
// =============================================================================

/// The state of the code generator, maintaining pools and truth ledgers.
///
/// This implements the "Graph Growth" strategy:
/// - **Pools**: Track defined entities that can be referenced
/// - **Ledgers**: Track expectations for verification
/// - **ID counter**: Ensures all generated names are unique
#[derive(Debug, Clone, Default)]
pub struct GeneratorState {
    // === Structural Pools (The DAG) ===
    /// Pool of defined class names (e.g., ["Class_0", "Class_1"])
    pub classes: Vec<String>,
    /// Pool of defined module names
    pub modules: Vec<String>,
    /// Pool of defined method names per class/module
    pub methods: HashMap<String, Vec<String>>,
    /// Pool of defined instance variables per class
    pub instance_vars: HashMap<String, Vec<String>>,
    /// Pool of defined constants per namespace
    pub constants: HashMap<String, Vec<String>>,

    // === Truth Ledgers ===
    pub type_ledger: TypeLedger,
    pub ref_ledger: ReferenceLedger,
    pub hint_ledger: HintLedger,
    pub error_ledger: ErrorLedger,
    pub completion_ledger: CompletionLedger,

    // === Source Buffer ===
    pub lines: Vec<String>,

    // === ID Tracking ===
    next_id: u32,
}

impl GeneratorState {
    /// Create a new empty generator state
    pub fn new() -> Self {
        Self::default()
    }

    /// Get the next unique ID and increment the counter
    pub fn next_id(&mut self) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    /// Generate a unique class name
    pub fn make_class_name(&mut self) -> String {
        let id = self.next_id();
        format!("Class_{}", id)
    }

    /// Generate a unique module name
    pub fn make_module_name(&mut self) -> String {
        let id = self.next_id();
        format!("Mod_{}", id)
    }

    /// Generate a unique method name
    pub fn make_method_name(&mut self) -> String {
        let id = self.next_id();
        format!("method_{}", id)
    }

    /// Generate a unique variable name
    pub fn make_var_name(&mut self) -> String {
        let id = self.next_id();
        format!("var_{}", id)
    }

    /// Generate a unique instance variable name
    pub fn make_ivar_name(&mut self) -> String {
        let id = self.next_id();
        format!("@ivar_{}", id)
    }

    /// Generate a unique constant name
    pub fn make_const_name(&mut self) -> String {
        let id = self.next_id();
        format!("CONST_{}", id)
    }

    /// Generate a unique reference anchor ID
    pub fn make_ref_anchor(&mut self) -> String {
        let id = self.next_id();
        format!("REF:{}", id)
    }

    /// Generate a unique type anchor ID
    pub fn make_type_anchor(&mut self) -> String {
        let id = self.next_id();
        format!("TYPE:{}", id)
    }

    /// Generate a unique completion anchor ID
    pub fn make_completion_anchor(&mut self) -> String {
        let id = self.next_id();
        format!("COMP:{}", id)
    }

    /// Generate a unique error anchor ID
    pub fn make_error_anchor(&mut self) -> String {
        let id = self.next_id();
        format!("ERR:{}", id)
    }

    /// Emit a line of code to the buffer
    pub fn emit(&mut self, line: &str) {
        self.lines.push(line.to_string());
    }

    /// Emit multiple lines
    pub fn emit_lines(&mut self, lines: &[&str]) {
        for line in lines {
            self.emit(line);
        }
    }

    /// Get the current line number (0-indexed)
    pub fn current_line(&self) -> u32 {
        self.lines.len() as u32
    }

    /// Build the final source code from the buffer
    pub fn build_source(&self) -> String {
        self.lines.join("\n")
    }

    // === Structural Actions ===

    /// Define a base class (no parent)
    pub fn define_base_class(&mut self, class_name: &str) {
        self.emit(&format!("class {}", class_name));
        self.emit("end");
        self.emit("");
        self.classes.push(class_name.to_string());
        self.methods.insert(class_name.to_string(), Vec::new());
    }

    /// Define a subclass (inherits from parent in pool)
    pub fn define_subclass(&mut self, class_name: &str, parent_name: &str) -> String {
        let ref_anchor = self.make_ref_anchor();
        self.emit(&format!(
            "class {} < {} # <{}>",
            class_name, parent_name, ref_anchor
        ));
        self.emit("end");
        self.emit("");

        // Track the reference
        self.ref_ledger
            .anchors
            .insert(ref_anchor.clone(), parent_name.to_string());

        self.classes.push(class_name.to_string());
        self.methods.insert(class_name.to_string(), Vec::new());

        ref_anchor
    }

    /// Define a module
    pub fn define_module(&mut self, module_name: &str) {
        self.emit(&format!("module {}", module_name));
        self.emit("end");
        self.emit("");
        self.modules.push(module_name.to_string());
        self.methods.insert(module_name.to_string(), Vec::new());
    }

    /// Open a class for modification (re-open)
    pub fn open_class(&mut self, class_name: &str) {
        self.emit(&format!("class {}", class_name));
    }

    /// Close the current class/module
    pub fn close_class(&mut self) {
        self.emit("end");
        self.emit("");
    }

    /// Add an include statement to the current class
    pub fn add_include(&mut self, module_name: &str) -> String {
        let ref_anchor = self.make_ref_anchor();
        self.emit(&format!("  include {} # <{}>", module_name, ref_anchor));
        self.ref_ledger
            .anchors
            .insert(ref_anchor.clone(), module_name.to_string());
        ref_anchor
    }

    /// Add an extend statement
    pub fn add_extend(&mut self, module_name: &str) -> String {
        let ref_anchor = self.make_ref_anchor();
        self.emit(&format!("  extend {} # <{}>", module_name, ref_anchor));
        self.ref_ledger
            .anchors
            .insert(ref_anchor.clone(), module_name.to_string());
        ref_anchor
    }

    /// Add a prepend statement
    pub fn add_prepend(&mut self, module_name: &str) -> String {
        let ref_anchor = self.make_ref_anchor();
        self.emit(&format!("  prepend {} # <{}>", module_name, ref_anchor));
        self.ref_ledger
            .anchors
            .insert(ref_anchor.clone(), module_name.to_string());
        ref_anchor
    }

    /// Add a method definition to the current class/module
    pub fn define_method(&mut self, owner: &str, method_name: &str) {
        self.emit(&format!("  def {}", method_name));
        self.emit("    nil");
        self.emit("  end");
        self.emit("");

        if let Some(methods) = self.methods.get_mut(owner) {
            methods.push(method_name.to_string());
        }
    }

    /// Add a variable assignment with type tracking
    pub fn assign_variable(
        &mut self,
        var_name: &str,
        value: &str,
        expected_type: &str,
        indent: usize,
    ) {
        let indent_str = "  ".repeat(indent);
        self.emit(&format!("{}{} = {}", indent_str, var_name, value));
        self.type_ledger
            .var_types
            .insert(var_name.to_string(), expected_type.to_string());
    }

    /// Create a reference to a class with anchor tracking
    pub fn make_class_reference(&mut self, class_name: &str) -> (String, String) {
        let ref_anchor = self.make_ref_anchor();
        let code = format!("{}.new # <{}>", class_name, ref_anchor);
        self.ref_ledger
            .anchors
            .insert(ref_anchor.clone(), class_name.to_string());
        (code, ref_anchor)
    }

    /// Add a completion trigger point
    pub fn add_completion_trigger(&mut self, owner: &str) -> String {
        let comp_anchor = self.make_completion_anchor();

        // Get expected methods for this owner
        let expected_methods = self.methods.get(owner).cloned().unwrap_or_default();

        self.emit(&format!("    self. # <{}>", comp_anchor));
        self.completion_ledger
            .expected_completions
            .insert(comp_anchor.clone(), expected_methods);

        comp_anchor
    }
}

// =============================================================================
// TRACKED CODE V2 - Using SourceLocator and Ledgers
// =============================================================================

/// Generated code with tracked expectations using the Graph Growth strategy.
///
/// Unlike the legacy `TrackedCode` which stored exact positions, this version
/// stores unique names and anchors. Positions are resolved dynamically using
/// `SourceLocator` during verification.
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
        let _locator = self.locator(); // May be used in future for more precise detection
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

// =============================================================================
// CONVERSION: TrackedCodeV2 -> Legacy TrackedCode
// =============================================================================

impl TrackedCodeV2 {
    /// Convert to the legacy TrackedCode format for backwards compatibility.
    ///
    /// This creates SymbolMarker entries by finding positions dynamically
    /// using the SourceLocator. This allows existing tests to work with
    /// the new Graph Growth generators.
    pub fn to_legacy(&self) -> TrackedCode {
        let locator = self.locator();
        let mut markers = Vec::new();

        // Convert class definitions to markers
        for class_name in &self.state.classes {
            if let Some(pos) = locator.find_token(class_name) {
                markers.push(SymbolMarker {
                    name: class_name.clone(),
                    position: pos,
                    kind: MarkerKind::Definition,
                    definition_position: None,
                });
            }
        }

        // Convert module definitions to markers
        for module_name in &self.state.modules {
            if let Some(pos) = locator.find_token(module_name) {
                markers.push(SymbolMarker {
                    name: module_name.clone(),
                    position: pos,
                    kind: MarkerKind::Definition,
                    definition_position: None,
                });
            }
        }

        // Convert method definitions to markers
        for (_owner, methods) in &self.state.methods {
            for method_name in methods {
                if let Some(pos) = locator.find_token(method_name) {
                    markers.push(SymbolMarker {
                        name: method_name.clone(),
                        position: pos,
                        kind: MarkerKind::Definition,
                        definition_position: None,
                    });
                }
            }
        }

        // Convert reference anchors to markers
        // The anchor marks the line, but we need to find the actual reference position
        // on that line (where the target_name appears)
        for (anchor_id, target_name) in &self.state.ref_ledger.anchors {
            if let Some(anchor_line) = locator.find_anchor_line(anchor_id) {
                // Find the target name on the anchor's line
                if let Some(usage_pos) = locator.find_token_on_line(target_name, anchor_line) {
                    let def_pos = locator.find_token(target_name);
                    markers.push(SymbolMarker {
                        name: target_name.clone(),
                        position: usage_pos,
                        kind: MarkerKind::Reference,
                        definition_position: def_pos,
                    });
                }
            }
        }

        // Convert type ledger entries to markers
        for (var_name, expected_type) in &self.state.type_ledger.var_types {
            if let Some(pos) = locator.find_token(var_name) {
                markers.push(SymbolMarker {
                    name: var_name.clone(),
                    position: pos,
                    kind: MarkerKind::TypeInference {
                        expected_type: expected_type.clone(),
                    },
                    definition_position: None,
                });
            }
        }

        // Convert completion anchors to markers
        // For completion, we need the position right after the dot (.)
        for (anchor_id, expected_methods) in &self.state.completion_ledger.expected_completions {
            if let Some(pos) = locator.find_completion_position(anchor_id) {
                markers.push(SymbolMarker {
                    name: "completion".to_string(),
                    position: pos,
                    kind: MarkerKind::CompletionTrigger {
                        expected_methods: expected_methods.clone(),
                    },
                    definition_position: None,
                });
            }
        }

        TrackedCode {
            code: self.code.clone(),
            markers,
            filename: self.filename.clone(),
            edit_count: self.edit_count,
        }
    }
}

/// Wrapper to make TrackedCodeV2 usable in existing test infrastructure
impl From<TrackedCodeV2> for TrackedCode {
    fn from(v2: TrackedCodeV2) -> Self {
        v2.to_legacy()
    }
}

// =============================================================================
// GRAPH GROWTH GENERATORS - Pool-Based Code Generation
// =============================================================================

/// Generate a TrackedCodeV2 with a class hierarchy using Graph Growth
pub fn graph_class_hierarchy() -> impl Strategy<Value = TrackedCodeV2> {
    // Parameters: number of classes (1-4), whether to use inheritance
    (1..5usize, prop::bool::ANY).prop_map(|(num_classes, use_inheritance)| {
        let mut state = GeneratorState::new();

        // Generate the first class (always a base class)
        let first_class = state.make_class_name();
        state.emit(&format!("class {}", first_class));
        let method1 = state.make_method_name();
        state.define_method(&first_class, &method1);
        state.close_class();
        state.classes.push(first_class.clone());
        state.methods.insert(first_class.clone(), vec![method1]);

        // Generate additional classes
        for _ in 1..num_classes {
            let class_name = state.make_class_name();

            if use_inheritance && !state.classes.is_empty() {
                // Pick a random parent from existing classes
                let parent_idx = (state.next_id() as usize) % state.classes.len();
                let parent = state.classes[parent_idx].clone();
                state.define_subclass(&class_name, &parent);
            } else {
                state.define_base_class(&class_name);
            }
        }

        TrackedCodeV2::from_state(state, "hierarchy.rb".to_string())
    })
}

/// Generate a TrackedCodeV2 with mixin relationships
pub fn graph_mixin_relationships() -> impl Strategy<Value = TrackedCodeV2> {
    (1..4usize, 1..4usize).prop_map(|(num_modules, num_classes)| {
        let mut state = GeneratorState::new();

        // Generate modules first
        for _ in 0..num_modules {
            let module_name = state.make_module_name();
            state.emit(&format!("module {}", module_name));

            // Add a method to each module
            let method = state.make_method_name();
            state.define_method(&module_name, &method);

            state.close_class();
            state.modules.push(module_name.clone());
            state.methods.insert(module_name, vec![method]);
        }

        // Generate classes that include modules
        for _ in 0..num_classes {
            let class_name = state.make_class_name();
            state.open_class(&class_name);

            // Include a random module
            if !state.modules.is_empty() {
                let mod_idx = (state.next_id() as usize) % state.modules.len();
                let module_name = state.modules[mod_idx].clone();
                state.add_include(&module_name);
            }

            // Add a method to the class
            let method = state.make_method_name();
            state.define_method(&class_name, &method);

            state.close_class();
            state.classes.push(class_name.clone());
        }

        TrackedCodeV2::from_state(state, "mixins.rb".to_string())
    })
}

/// Generate a TrackedCodeV2 with type-inferred variables
pub fn graph_type_inference() -> impl Strategy<Value = TrackedCodeV2> {
    prop_oneof![Just("String"), Just("Integer"), Just("Array"), Just("Hash"),].prop_map(
        |var_type| {
            let mut state = GeneratorState::new();

            let var_name = state.make_var_name();
            let value = match var_type {
                "String" => "\"hello\"",
                "Integer" => "42",
                "Array" => "[1, 2, 3]",
                "Hash" => "{ a: 1 }",
                _ => "nil",
            };

            state.assign_variable(&var_name, value, var_type, 0);
            state.emit("");

            // Add a usage of the variable
            state.emit(&format!("puts {}", var_name));

            TrackedCodeV2::from_state(state, "types.rb".to_string())
        },
    )
}

/// Generate a TrackedCodeV2 with class references (for go-to-definition testing)
pub fn graph_class_references() -> impl Strategy<Value = TrackedCodeV2> {
    (2..5usize).prop_map(|num_classes| {
        let mut state = GeneratorState::new();

        // Generate classes
        for _ in 0..num_classes {
            let class_name = state.make_class_name();
            state.define_base_class(&class_name);
        }

        // Generate references to each class
        for class_name in state.classes.clone() {
            let (ref_code, _anchor) = state.make_class_reference(&class_name);
            state.emit(&format!("_ = {}", ref_code));
        }

        TrackedCodeV2::from_state(state, "references.rb".to_string())
    })
}

/// Generate a TrackedCodeV2 with completion test points
pub fn graph_completion_test() -> impl Strategy<Value = TrackedCodeV2> {
    (2..5usize).prop_map(|num_methods| {
        let mut state = GeneratorState::new();

        let class_name = state.make_class_name();
        state.emit(&format!("class {}", class_name));

        // Generate methods
        let mut methods = Vec::new();
        for _ in 0..num_methods {
            let method = state.make_method_name();
            state.define_method(&class_name, &method);
            methods.push(method);
        }

        // Add a method with completion trigger
        state.emit("  def test_completion");
        let _comp_anchor = state.add_completion_trigger(&class_name);
        state.emit("  end");

        state.close_class();
        state.classes.push(class_name.clone());
        state.methods.insert(class_name, methods);

        TrackedCodeV2::from_state(state, "completion.rb".to_string())
    })
}

/// Generate any TrackedCodeV2 using Graph Growth strategy
pub fn graph_tracked_code() -> impl Strategy<Value = TrackedCodeV2> {
    prop_oneof![
        3 => graph_class_hierarchy(),
        3 => graph_mixin_relationships(),
        2 => graph_type_inference(),
        2 => graph_class_references(),
        2 => graph_completion_test(),
    ]
}

// =============================================================================
// STRICT TYPE INFERENCE GENERATORS - Tests that SHOULD expose bugs
// =============================================================================
//
// These generators create scenarios known to stress type inference:
// - Method chains on literals
// - Array/Hash element access
// - Instance variable types across methods
// - Type propagation through assignments

/// A strict type expectation that MUST match (not lenient)
#[derive(Debug, Clone)]
pub struct StrictTypeExpectation {
    /// Variable or expression name
    pub var_name: String,
    /// Expected type (e.g., "String", "Integer", "Array<String>")
    pub expected_type: String,
    /// Anchor ID for locating in source
    pub anchor_id: String,
    /// Description of what we're testing
    pub description: String,
}

/// Extended ledger for strict type verification
#[derive(Debug, Clone, Default)]
pub struct StrictTypeLedger {
    pub expectations: Vec<StrictTypeExpectation>,
}

impl GeneratorState {
    /// Add a strict type expectation that MUST be verified
    pub fn expect_type(&mut self, var_name: &str, expected_type: &str, _description: &str) {
        let _anchor_id = self.make_type_anchor();
        // Note: The anchor is added to the emitted code separately
        self.type_ledger
            .var_types
            .insert(var_name.to_string(), expected_type.to_string());
    }

    /// Emit a variable assignment with strict type checking anchor
    pub fn emit_typed_assignment(&mut self, var_name: &str, value: &str, expected_type: &str) {
        let anchor = self.make_type_anchor();
        self.emit(&format!("{} = {} # <{}>", var_name, value, anchor));
        self.type_ledger
            .var_types
            .insert(var_name.to_string(), expected_type.to_string());
    }
}

/// Generate method chain scenarios that test type propagation
///
/// Tests cases like:
/// - `"hello".upcase` -> String
/// - `[1,2,3].first` -> Integer (or nil)
/// - `{a: 1}.keys` -> Array
pub fn graph_method_chain_types() -> impl Strategy<Value = TrackedCodeV2> {
    prop_oneof![
        // String method chains
        Just(("\"hello\".upcase", "String", "string_upcase")),
        Just(("\"hello\".downcase", "String", "string_downcase")),
        Just(("\"hello\".length", "Integer", "string_length")),
        Just(("\"hello\".chars", "Array", "string_chars")),
        Just(("\"hello\".split", "Array", "string_split")),
        // Array method chains
        Just(("[1, 2, 3].length", "Integer", "array_length")),
        Just(("[1, 2, 3].first", "Integer", "array_first")), // Known issue: might be nil
        Just(("[1, 2, 3].last", "Integer", "array_last")),
        Just(("[1, 2, 3].reverse", "Array", "array_reverse")),
        Just(("[[1], [2]].flatten", "Array", "array_flatten")),
        // Hash method chains
        Just(("{ a: 1 }.keys", "Array", "hash_keys")),
        Just(("{ a: 1 }.values", "Array", "hash_values")),
        Just(("{ a: 1 }.length", "Integer", "hash_length")),
        // Chained methods (more complex)
        Just(("\"hello\".upcase.downcase", "String", "chained_string")),
        Just(("[1, 2, 3].first.to_s", "String", "array_first_to_s")), // This likely fails
        Just(("{ a: 1 }[:a].to_s", "String", "hash_access_to_s")),    // This likely fails
    ]
    .prop_map(|(expr, expected_type, test_name)| {
        let mut state = GeneratorState::new();
        let var_name = state.make_var_name();

        state.emit(&format!(
            "# Test: {} should be {}",
            test_name, expected_type
        ));
        state.emit_typed_assignment(&var_name, expr, expected_type);
        state.emit("");
        state.emit(&format!("puts {}", var_name));

        TrackedCodeV2::from_state(state, format!("{}.rb", test_name))
    })
}

/// Generate array element access scenarios
///
/// Tests:
/// - `["a", "b"][0]` -> String (but inference might say nil or unknown)
/// - `[1, 2, 3][0]` -> Integer
pub fn graph_array_access_types() -> impl Strategy<Value = TrackedCodeV2> {
    prop_oneof![
        // String arrays
        Just((
            "[\"a\", \"b\", \"c\"]",
            "0",
            "String",
            "string_array_access"
        )),
        Just((
            "[\"hello\", \"world\"]",
            "1",
            "String",
            "string_array_second"
        )),
        // Integer arrays
        Just(("[1, 2, 3]", "0", "Integer", "int_array_access")),
        Just(("[10, 20, 30]", "-1", "Integer", "int_array_negative")),
        // Mixed - these are harder to type
        Just(("[:a, :b]", "0", "Symbol", "symbol_array_access")),
    ]
    .prop_map(|(array_literal, index, expected_type, test_name)| {
        let mut state = GeneratorState::new();
        let var_name = state.make_var_name();

        state.emit(&format!(
            "# Test: {}[{}] should be {}",
            array_literal, index, expected_type
        ));
        let expr = format!("{}[{}]", array_literal, index);
        state.emit_typed_assignment(&var_name, &expr, expected_type);
        state.emit("");

        // Try to call a method that requires the expected type
        match expected_type {
            "String" => {
                let result_var = state.make_var_name();
                state.emit(&format!(
                    "{} = {}.upcase # Should work if {} is String",
                    result_var, var_name, var_name
                ));
            }
            "Integer" => {
                let result_var = state.make_var_name();
                state.emit(&format!(
                    "{} = {} + 1 # Should work if {} is Integer",
                    result_var, var_name, var_name
                ));
            }
            _ => {}
        }

        TrackedCodeV2::from_state(state, format!("{}.rb", test_name))
    })
}

/// Generate class instance method type scenarios
///
/// Tests that method calls on class instances return correct types
pub fn graph_class_method_types() -> impl Strategy<Value = TrackedCodeV2> {
    Just(()).prop_map(|_| {
        let mut state = GeneratorState::new();

        // Define a class with typed methods
        let class_name = state.make_class_name();
        state.emit(&format!("class {}", class_name));
        state.emit("  def get_string");
        state.emit("    \"hello\"");
        state.emit("  end");
        state.emit("");
        state.emit("  def get_number");
        state.emit("    42");
        state.emit("  end");
        state.emit("");
        state.emit("  def get_array");
        state.emit("    [1, 2, 3]");
        state.emit("  end");
        state.emit("end");
        state.emit("");

        // Create instance and call methods
        let instance_var = state.make_var_name();
        state.emit(&format!("{} = {}.new", instance_var, class_name));
        state.emit("");

        // These should have known return types based on method body analysis
        let str_var = state.make_var_name();
        state.emit_typed_assignment(&str_var, &format!("{}.get_string", instance_var), "String");

        let num_var = state.make_var_name();
        state.emit_typed_assignment(&num_var, &format!("{}.get_number", instance_var), "Integer");

        let arr_var = state.make_var_name();
        state.emit_typed_assignment(&arr_var, &format!("{}.get_array", instance_var), "Array");

        state.emit("");
        state.emit("# Chain method calls - these test type propagation");
        let chain_var = state.make_var_name();
        state.emit_typed_assignment(
            &chain_var,
            &format!("{}.get_string.upcase", instance_var),
            "String",
        );

        state.classes.push(class_name);
        TrackedCodeV2::from_state(state, "class_method_types.rb".to_string())
    })
}

/// Generate scenarios with instance variables across methods
///
/// Tests that @ivar types are tracked correctly between methods
pub fn graph_ivar_type_propagation() -> impl Strategy<Value = TrackedCodeV2> {
    Just(()).prop_map(|_| {
        let mut state = GeneratorState::new();

        let class_name = state.make_class_name();
        state.emit(&format!("class {}", class_name));

        // Initialize with a known type
        state.emit("  def initialize");
        state.emit("    @data = \"initial string\""); // String
        state.emit("    @numbers = [1, 2, 3]"); // Array<Integer>
        state.emit("  end");
        state.emit("");

        // Method that uses the ivars - type should propagate
        state.emit("  def process");
        let result_var = state.make_var_name();
        // @data.upcase should be String if @data is String
        state.emit(&format!(
            "    {} = @data.upcase # <TYPE:ivar_string>",
            result_var
        ));
        state
            .type_ledger
            .var_types
            .insert(result_var.clone(), "String".to_string());

        let sum_var = state.make_var_name();
        // @numbers.first should be Integer if @numbers is Array<Integer>
        state.emit(&format!(
            "    {} = @numbers.first # <TYPE:ivar_array>",
            sum_var
        ));
        state
            .type_ledger
            .var_types
            .insert(sum_var.clone(), "Integer".to_string());

        state.emit(&format!("    {}", result_var));
        state.emit("  end");
        state.emit("end");
        state.emit("");

        // Test usage
        let obj_var = state.make_var_name();
        state.emit(&format!("{} = {}.new", obj_var, class_name));
        let call_result = state.make_var_name();
        state.emit_typed_assignment(&call_result, &format!("{}.process", obj_var), "String");

        state.classes.push(class_name);
        TrackedCodeV2::from_state(state, "ivar_propagation.rb".to_string())
    })
}

/// Generate completion test for method chains
///
/// After `str = "hello"`, typing `str.` should suggest String methods
pub fn graph_completion_after_type() -> impl Strategy<Value = TrackedCodeV2> {
    prop_oneof![
        Just((
            "\"hello\"",
            "String",
            vec!["upcase", "downcase", "length", "chars"]
        )),
        Just((
            "[1, 2, 3]",
            "Array",
            vec!["first", "last", "length", "each"]
        )),
        Just(("{ a: 1 }", "Hash", vec!["keys", "values", "each"])),
        Just(("42", "Integer", vec!["to_s", "times", "abs"])),
    ]
    .prop_map(|(literal, type_name, expected_methods)| {
        let mut state = GeneratorState::new();
        let var_name = state.make_var_name();

        state.emit(&format!("# {} should have {} methods", var_name, type_name));
        state.assign_variable(&var_name, literal, type_name, 0);
        state.emit("");

        // Completion trigger
        let comp_anchor = state.make_completion_anchor();
        state.emit(&format!("{}. # <{}>", var_name, comp_anchor));

        // Record expected completions
        state.completion_ledger.expected_completions.insert(
            comp_anchor,
            expected_methods.iter().map(|s| s.to_string()).collect(),
        );

        TrackedCodeV2::from_state(state, format!("{}_completion.rb", type_name.to_lowercase()))
    })
}

/// Generate edge cases known to break type inference
///
/// These are scenarios where type inference is known to fail
pub fn graph_type_edge_cases() -> impl Strategy<Value = TrackedCodeV2> {
    prop_oneof![
        // Conditional assignment - type might be union
        Just((
            "x = rand > 0.5 ? \"string\" : 42",
            "union",
            "conditional_type"
        )),
        // Nil coalescing
        Just(("x = nil || \"default\"", "String", "nil_coalesce")),
        // Array of mixed types
        Just((
            "arr = [1, \"two\", :three]; x = arr[0]",
            "unknown",
            "mixed_array"
        )),
        // Method with no obvious return type
        Just((
            "def mystery; if rand > 0.5; 1; else; \"a\"; end; end; x = mystery",
            "unknown",
            "mystery_method"
        )),
    ]
    .prop_map(|(code, expected_category, test_name)| {
        let mut state = GeneratorState::new();

        state.emit(&format!(
            "# Edge case: {} (expected: {})",
            test_name, expected_category
        ));
        // For edge cases, we just emit the code and don't expect precise types
        for line in code.split("; ") {
            state.emit(line);
        }

        TrackedCodeV2::from_state(state, format!("{}.rb", test_name))
    })
}

// =============================================================================
// Position Adjustment for Edit Tracking
// =============================================================================

/// Adjusts a position after an edit operation.
///
/// When text is edited at a range, all positions after the edit need to be
/// adjusted. This function computes the new position for a marker given the
/// original edit range and replacement text.
///
/// Returns `None` if the position was inside the deleted range (marker destroyed).
pub fn adjust_position_after_edit(
    position: Position,
    edit_range: &Range,
    new_text: &str,
) -> Option<Position> {
    // If position is before the edit start, it's unchanged
    if position.line < edit_range.start.line
        || (position.line == edit_range.start.line
            && position.character < edit_range.start.character)
    {
        return Some(position);
    }

    // If position is inside the deleted range, the marker is destroyed
    if is_position_in_range(&position, edit_range) {
        return None;
    }

    // Position is after the edit - compute the delta
    let deleted_lines = edit_range.end.line - edit_range.start.line;
    let new_lines = new_text.matches('\n').count() as u32;

    // Calculate the last line length of new text
    let new_text_last_line_len = new_text
        .rfind('\n')
        .map(|i| (new_text.len() - i - 1) as u32)
        .unwrap_or(new_text.len() as u32);

    let new_position = if position.line == edit_range.end.line {
        // Position is on the same line as edit end
        // Character needs adjustment based on end position
        let chars_after_edit = position.character.saturating_sub(edit_range.end.character);

        if new_lines == 0 {
            // Edit stays on same line
            let new_char = if deleted_lines == 0 {
                // Single-line edit
                edit_range.start.character + new_text_last_line_len + chars_after_edit
            } else {
                // Multi-line to single-line: position moves to start line
                edit_range.start.character + new_text_last_line_len + chars_after_edit
            };
            Position {
                line: edit_range.start.line,
                character: new_char,
            }
        } else {
            // New text has multiple lines
            Position {
                line: edit_range.start.line + new_lines,
                character: new_text_last_line_len + chars_after_edit,
            }
        }
    } else {
        // Position is on a line after the edit end line
        let line_delta = new_lines as i64 - deleted_lines as i64;
        Position {
            line: (position.line as i64 + line_delta) as u32,
            character: position.character,
        }
    };

    Some(new_position)
}

/// Check if a position is inside a range (inclusive start, exclusive end)
fn is_position_in_range(position: &Position, range: &Range) -> bool {
    // Before start
    if position.line < range.start.line
        || (position.line == range.start.line && position.character < range.start.character)
    {
        return false;
    }

    // After or at end
    if position.line > range.end.line
        || (position.line == range.end.line && position.character >= range.end.character)
    {
        return false;
    }

    true
}

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
    /// A completion trigger point (after `self.` or `obj.`)
    /// Expected methods are user-defined methods from the class/module, not builtins
    CompletionTrigger {
        /// Expected method names that should appear in completion (user-defined only)
        expected_methods: Vec<String>,
    },
    /// A variable with an expected inferred type
    /// Used to test that type inference survives edits
    TypeInference {
        /// Expected type (e.g., "String", "Integer", "Array", "Hash")
        expected_type: String,
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
    /// Count of edits applied (for debugging)
    pub edit_count: u32,
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

    /// Get type inference markers (variables with expected types)
    pub fn type_markers(&self) -> Vec<&SymbolMarker> {
        self.markers
            .iter()
            .filter(|m| matches!(m.kind, MarkerKind::TypeInference { .. }))
            .collect()
    }

    /// Get markers for a specific symbol name
    pub fn markers_for(&self, name: &str) -> Vec<&SymbolMarker> {
        self.markers.iter().filter(|m| m.name == name).collect()
    }

    /// Apply an edit to the tracked code, adjusting all marker positions.
    ///
    /// This is the critical function for deterministic edit tracking.
    /// After applying an edit:
    /// 1. The code string is updated with the new text
    /// 2. All marker positions are adjusted to reflect the edit
    /// 3. Markers that were inside the deleted range are marked as destroyed
    ///
    /// Returns true if the edit was valid, false if it would be out of bounds.
    pub fn apply_edit(&mut self, range: &Range, new_text: &str) -> bool {
        // Validate range is within bounds
        let line_count = self.code.lines().count() as u32;
        if range.start.line >= line_count && line_count > 0 {
            return false;
        }

        // Apply edit to the code string
        let new_code = apply_edit_to_code(&self.code, range, new_text);
        self.code = new_code;

        // Update all marker positions
        let mut surviving_markers = Vec::new();
        for mut marker in self.markers.drain(..) {
            // Adjust the marker position
            if let Some(new_pos) = adjust_position_after_edit(marker.position, range, new_text) {
                marker.position = new_pos;

                // Also adjust the definition position if present
                if let Some(def_pos) = marker.definition_position {
                    marker.definition_position =
                        adjust_position_after_edit(def_pos, range, new_text);
                }

                surviving_markers.push(marker);
            }
            // If adjust_position_after_edit returns None, the marker was destroyed
        }
        self.markers = surviving_markers;

        self.edit_count += 1;
        true
    }

    /// Get a "safe" edit position that won't destroy any markers.
    ///
    /// Returns a position at the end of a line that has no markers on it,
    /// or None if no safe position exists.
    pub fn find_safe_edit_line(&self) -> Option<u32> {
        let line_count = self.code.lines().count() as u32;
        if line_count == 0 {
            return None;
        }

        // Find lines with no markers
        let marker_lines: std::collections::HashSet<u32> =
            self.markers.iter().map(|m| m.position.line).collect();

        // Also avoid lines that are definition targets
        let def_lines: std::collections::HashSet<u32> = self
            .markers
            .iter()
            .filter_map(|m| m.definition_position)
            .map(|p| p.line)
            .collect();

        // Find the first line with no markers
        for line in 0..line_count {
            if !marker_lines.contains(&line) && !def_lines.contains(&line) {
                return Some(line);
            }
        }

        // If all lines have markers, return the last line
        // (we'll insert at the very end)
        Some(line_count.saturating_sub(1))
    }

    /// Get the length of a specific line
    pub fn line_length(&self, line: u32) -> usize {
        self.code
            .lines()
            .nth(line as usize)
            .map(|l| l.len())
            .unwrap_or(0)
    }
}

/// Apply an edit to a code string and return the result
fn apply_edit_to_code(code: &str, range: &Range, new_text: &str) -> String {
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
            edit_count: 0,
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
                edit_count: 0,
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
                edit_count: 0,
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
            edit_count: 0,
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
                edit_count: 0,
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
                edit_count: 0,
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
                edit_count: 0,
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
                edit_count: 0,
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
                edit_count: 0,
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
                edit_count: 0,
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
                edit_count: 0,
            }
        })
}

/// Generate code for completion testing through ancestor chain
/// Tests that completion includes methods from included modules
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
                // Completion trigger - should include methods from modules and class
                SymbolMarker {
                    name: "self".to_string(),
                    position: Position {
                        line: 17,
                        character: 9,
                    },
                    kind: MarkerKind::CompletionTrigger {
                        expected_methods: vec![method1, method2, method3],
                    },
                    definition_position: None,
                },
            ];

            TrackedCode {
                code,
                markers,
                filename: format!("{}.rb", class_name.to_lowercase()),
                edit_count: 0,
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
            edit_count: 0,
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
            edit_count: 0,
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
            edit_count: 0,
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
            edit_count: 0,
        }),
    ]
}

// =============================================================================
// COMPLETION GENERATORS (for testing completion with user-defined methods)
// =============================================================================

/// Generate a class with multiple methods and test completion for self.
/// This tests that completion includes user-defined methods, not builtins.
pub fn tracked_self_completion() -> impl Strategy<Value = TrackedCode> {
    (
        ruby_class_name(),
        ruby_identifier(),
        ruby_identifier(),
        ruby_identifier(),
    )
        .prop_map(|(class_name, method1, method2, method3)| {
            // Line 0: class ClassName
            // Line 1:   def method1
            // Line 2:     nil
            // Line 3:   end
            // Line 4:
            // Line 5:   def method2
            // Line 6:     nil
            // Line 7:   end
            // Line 8:
            // Line 9:   def method3
            // Line 10:    nil
            // Line 11:  end
            // Line 12:
            // Line 13:  def test_completion
            // Line 14:    self.  # Completion trigger - should include method1, method2, method3
            // Line 15:  end
            // Line 16: end

            let code = format!(
                r#"class {}
  def {}
    nil
  end

  def {}
    nil
  end

  def {}
    nil
  end

  def test_completion
    self.
  end
end"#,
                class_name, method1, method2, method3
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
                // Method definitions
                SymbolMarker {
                    name: method1.clone(),
                    position: Position {
                        line: 1,
                        character: 6,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                SymbolMarker {
                    name: method2.clone(),
                    position: Position {
                        line: 5,
                        character: 6,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                SymbolMarker {
                    name: method3.clone(),
                    position: Position {
                        line: 9,
                        character: 6,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Completion trigger - should include all three user-defined methods
                SymbolMarker {
                    name: "self".to_string(),
                    position: Position {
                        line: 14,
                        character: 9, // After "self."
                    },
                    kind: MarkerKind::CompletionTrigger {
                        expected_methods: vec![
                            method1,
                            method2,
                            method3,
                            "test_completion".to_string(),
                        ],
                    },
                    definition_position: None,
                },
            ];

            TrackedCode {
                code,
                markers,
                filename: format!("{}.rb", class_name.to_lowercase()),
                edit_count: 0,
            }
        })
}

/// Generate a class with attr_accessor and test completion
pub fn tracked_attr_completion() -> impl Strategy<Value = TrackedCode> {
    (ruby_class_name(), ruby_identifier(), ruby_identifier()).prop_map(
        |(class_name, attr1, attr2)| {
            // Line 0: class ClassName
            // Line 1:   attr_accessor :attr1, :attr2
            // Line 2:
            // Line 3:   def test_completion
            // Line 4:     self.  # Should include attr1, attr2, attr1=, attr2=
            // Line 5:   end
            // Line 6: end

            let code = format!(
                r#"class {}
  attr_accessor :{}, :{}

  def test_completion
    self.
  end
end"#,
                class_name, attr1, attr2
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
                // Completion trigger - should include attr readers and writers
                SymbolMarker {
                    name: "self".to_string(),
                    position: Position {
                        line: 4,
                        character: 9,
                    },
                    kind: MarkerKind::CompletionTrigger {
                        expected_methods: vec![attr1, attr2],
                    },
                    definition_position: None,
                },
            ];

            TrackedCode {
                code,
                markers,
                filename: format!("{}.rb", class_name.to_lowercase()),
                edit_count: 0,
            }
        },
    )
}

/// Generate code with module inclusion and test completion through ancestor chain
/// Note: We don't test include reference resolution as it's not reliably supported yet
pub fn tracked_mixin_completion() -> impl Strategy<Value = TrackedCode> {
    (
        ruby_class_name(),
        ruby_class_name(),
        ruby_identifier(),
        ruby_identifier(),
    )
        .prop_map(|(module_name, class_name, module_method, class_method)| {
            // Line 0: module ModuleName
            // Line 1:   def module_method
            // Line 2:     nil
            // Line 3:   end
            // Line 4: end
            // Line 5:
            // Line 6: class ClassName
            // Line 7:   include ModuleName
            // Line 8:
            // Line 9:   def class_method
            // Line 10:    nil
            // Line 11:  end
            // Line 12:
            // Line 13:  def test_completion
            // Line 14:    self.  # Should include both module_method and class_method
            // Line 15:  end
            // Line 16: end

            let code = format!(
                r#"module {}
  def {}
    nil
  end
end

class {}
  include {}

  def {}
    nil
  end

  def test_completion
    self.
  end
end"#,
                module_name, module_method, class_name, module_name, class_method
            );

            let markers = vec![
                // Module definition
                SymbolMarker {
                    name: module_name.clone(),
                    position: Position {
                        line: 0,
                        character: 7,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Module method definition
                SymbolMarker {
                    name: module_method.clone(),
                    position: Position {
                        line: 1,
                        character: 6,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Class definition
                SymbolMarker {
                    name: class_name.clone(),
                    position: Position {
                        line: 6,
                        character: 6,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // NOTE: Include reference resolution (include ModuleName -> module definition)
                // is not reliably tested because it's a known LSP limitation
                // Class method definition
                SymbolMarker {
                    name: class_method.clone(),
                    position: Position {
                        line: 9,
                        character: 6,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Completion trigger - should include methods from module and class
                SymbolMarker {
                    name: "self".to_string(),
                    position: Position {
                        line: 14,
                        character: 9,
                    },
                    kind: MarkerKind::CompletionTrigger {
                        expected_methods: vec![
                            module_method,
                            class_method,
                            "test_completion".to_string(),
                        ],
                    },
                    definition_position: None,
                },
            ];

            TrackedCode {
                code,
                markers,
                filename: format!("{}.rb", class_name.to_lowercase()),
                edit_count: 0,
            }
        })
}

/// Generate code with methods and test completion on instance
/// Note: We don't test the reference resolution for class instantiation as it's complex
pub fn tracked_instance_completion() -> impl Strategy<Value = TrackedCode> {
    (ruby_class_name(), ruby_identifier(), ruby_identifier()).prop_map(
        |(class_name, method1, method2)| {
            // Line 0: class ClassName
            // Line 1:   def method1
            // Line 2:     nil
            // Line 3:   end
            // Line 4:
            // Line 5:   def method2
            // Line 6:     nil
            // Line 7:   end
            // Line 8: end
            // Line 9:
            // Line 10: obj = ClassName.new
            // Line 11: obj.  # Completion trigger

            let code = format!(
                r#"class {}
  def {}
    nil
  end

  def {}
    nil
  end
end

obj = {}.new
obj."#,
                class_name, method1, method2, class_name
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
                // Method definitions
                SymbolMarker {
                    name: method1.clone(),
                    position: Position {
                        line: 1,
                        character: 6,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                SymbolMarker {
                    name: method2.clone(),
                    position: Position {
                        line: 5,
                        character: 6,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // NOTE: We don't test class reference in instantiation (obj = ClassName.new)
                // because goto definition from that position is not reliably supported yet.
                // Completion trigger on instance
                SymbolMarker {
                    name: "obj".to_string(),
                    position: Position {
                        line: 11,
                        character: 4,
                    },
                    kind: MarkerKind::CompletionTrigger {
                        expected_methods: vec![method1, method2],
                    },
                    definition_position: None,
                },
            ];

            TrackedCode {
                code,
                markers,
                filename: format!("{}.rb", class_name.to_lowercase()),
                edit_count: 0,
            }
        },
    )
}

// =============================================================================
// TYPE INFERENCE GENERATORS
// =============================================================================
//
// These generators test that type inference survives edits. They create code
// with variables that have known inferred types and track those positions.

/// Generate code with simple literal assignments and type markers
/// Tests: String, Integer, Float, Symbol, Array, Hash literals
pub fn tracked_type_inference_literals() -> impl Strategy<Value = TrackedCode> {
    (
        ruby_identifier(),
        ruby_identifier(),
        ruby_identifier(),
        ruby_identifier(),
    )
        .prop_map(|(str_var, int_var, arr_var, hash_var)| {
            // Line 0: str_var = "hello"
            // Line 1: int_var = 42
            // Line 2: arr_var = [1, 2, 3]
            // Line 3: hash_var = { a: 1 }

            let code = format!(
                r#"{} = "hello"
{} = 42
{} = [1, 2, 3]
{} = {{ a: 1 }}"#,
                str_var, int_var, arr_var, hash_var
            );

            let markers = vec![
                SymbolMarker {
                    name: str_var.clone(),
                    position: Position {
                        line: 0,
                        character: 0,
                    },
                    kind: MarkerKind::TypeInference {
                        expected_type: "String".to_string(),
                    },
                    definition_position: None,
                },
                SymbolMarker {
                    name: int_var.clone(),
                    position: Position {
                        line: 1,
                        character: 0,
                    },
                    kind: MarkerKind::TypeInference {
                        expected_type: "Integer".to_string(),
                    },
                    definition_position: None,
                },
                SymbolMarker {
                    name: arr_var.clone(),
                    position: Position {
                        line: 2,
                        character: 0,
                    },
                    kind: MarkerKind::TypeInference {
                        expected_type: "Array".to_string(),
                    },
                    definition_position: None,
                },
                SymbolMarker {
                    name: hash_var.clone(),
                    position: Position {
                        line: 3,
                        character: 0,
                    },
                    kind: MarkerKind::TypeInference {
                        expected_type: "Hash".to_string(),
                    },
                    definition_position: None,
                },
            ];

            TrackedCode {
                code,
                markers,
                filename: "type_literals.rb".to_string(),
                edit_count: 0,
            }
        })
}

/// Generate code with method return type inference inside a class
/// Tests type inference in method context
pub fn tracked_type_inference_in_method() -> impl Strategy<Value = TrackedCode> {
    (
        ruby_class_name(),
        ruby_identifier(),
        ruby_identifier(),
        ruby_identifier(),
    )
        .prop_map(|(class_name, method_name, local_str, local_int)| {
            // Line 0: class ClassName
            // Line 1:   def method_name
            // Line 2:     local_str = "hello"
            // Line 3:     local_int = 123
            // Line 4:     local_str
            // Line 5:   end
            // Line 6: end

            let code = format!(
                r#"class {}
  def {}
    {} = "hello"
    {} = 123
    {}
  end
end"#,
                class_name, method_name, local_str, local_int, local_str
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
                // Method definition
                SymbolMarker {
                    name: method_name.clone(),
                    position: Position {
                        line: 1,
                        character: 6,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                // Type inference for local string variable
                SymbolMarker {
                    name: local_str.clone(),
                    position: Position {
                        line: 2,
                        character: 4,
                    },
                    kind: MarkerKind::TypeInference {
                        expected_type: "String".to_string(),
                    },
                    definition_position: None,
                },
                // Type inference for local integer variable
                SymbolMarker {
                    name: local_int.clone(),
                    position: Position {
                        line: 3,
                        character: 4,
                    },
                    kind: MarkerKind::TypeInference {
                        expected_type: "Integer".to_string(),
                    },
                    definition_position: None,
                },
            ];

            TrackedCode {
                code,
                markers,
                filename: format!("{}.rb", class_name.to_lowercase()),
                edit_count: 0,
            }
        })
}

/// Generate code with instance variables and type inference
/// Tests that @ivar type inference works correctly
pub fn tracked_type_inference_ivars() -> impl Strategy<Value = TrackedCode> {
    (ruby_class_name(), ruby_identifier()).prop_map(|(class_name, var_name)| {
        // Line 0: class ClassName
        // Line 1:   def initialize
        // Line 2:     @var_name = "string value"
        // Line 3:   end
        // Line 4:
        // Line 5:   def reader
        // Line 6:     @var_name
        // Line 7:   end
        // Line 8: end

        let code = format!(
            r#"class {}
  def initialize
    @{} = "string value"
  end

  def reader
    @{}
  end
end"#,
            class_name, var_name, var_name
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
            // Instance variable assignment - type inference
            SymbolMarker {
                name: format!("@{}", var_name),
                position: Position {
                    line: 2,
                    character: 4,
                },
                kind: MarkerKind::TypeInference {
                    expected_type: "String".to_string(),
                },
                definition_position: None,
            },
            // Instance variable reference should have same type
            SymbolMarker {
                name: format!("@{}", var_name),
                position: Position {
                    line: 6,
                    character: 4,
                },
                kind: MarkerKind::TypeInference {
                    expected_type: "String".to_string(),
                },
                definition_position: None,
            },
        ];

        TrackedCode {
            code,
            markers,
            filename: format!("{}.rb", class_name.to_lowercase()),
            edit_count: 0,
        }
    })
}

/// Generate code with multiple assignments that should retain their types
/// Tests type inference under edits with multiple variables
pub fn tracked_type_inference_multi_assign() -> impl Strategy<Value = TrackedCode> {
    (ruby_identifier(), ruby_identifier(), ruby_identifier()).prop_map(|(var1, var2, var3)| {
        // Line 0: var1 = "first"
        // Line 1: var2 = 100
        // Line 2: var3 = :symbol
        // Line 3:
        // Line 4: # Use the variables
        // Line 5: puts var1
        // Line 6: puts var2
        // Line 7: puts var3

        let code = format!(
            r#"{} = "first"
{} = 100
{} = :symbol

# Use the variables
puts {}
puts {}
puts {}"#,
            var1, var2, var3, var1, var2, var3
        );

        let markers = vec![
            // Type markers for assignments
            SymbolMarker {
                name: var1.clone(),
                position: Position {
                    line: 0,
                    character: 0,
                },
                kind: MarkerKind::TypeInference {
                    expected_type: "String".to_string(),
                },
                definition_position: None,
            },
            SymbolMarker {
                name: var2.clone(),
                position: Position {
                    line: 1,
                    character: 0,
                },
                kind: MarkerKind::TypeInference {
                    expected_type: "Integer".to_string(),
                },
                definition_position: None,
            },
            SymbolMarker {
                name: var3.clone(),
                position: Position {
                    line: 2,
                    character: 0,
                },
                kind: MarkerKind::TypeInference {
                    expected_type: "Symbol".to_string(),
                },
                definition_position: None,
            },
            // References should also have types (after edits, these might break)
            SymbolMarker {
                name: var1.clone(),
                position: Position {
                    line: 5,
                    character: 5,
                },
                kind: MarkerKind::TypeInference {
                    expected_type: "String".to_string(),
                },
                definition_position: None,
            },
            SymbolMarker {
                name: var2.clone(),
                position: Position {
                    line: 6,
                    character: 5,
                },
                kind: MarkerKind::TypeInference {
                    expected_type: "Integer".to_string(),
                },
                definition_position: None,
            },
            SymbolMarker {
                name: var3.clone(),
                position: Position {
                    line: 7,
                    character: 5,
                },
                kind: MarkerKind::TypeInference {
                    expected_type: "Symbol".to_string(),
                },
                definition_position: None,
            },
        ];

        TrackedCode {
            code,
            markers,
            filename: "multi_assign.rb".to_string(),
            edit_count: 0,
        }
    })
}

/// Generate any tracked code scenario
/// Covers definitions, references, method calls, completion, and type inference
///
/// This now includes both legacy generators and new Graph Growth generators.
/// The Graph Growth generators produce cleaner, more consistent code with
/// unique identifiers that can be found dynamically.
pub fn tracked_code() -> impl Strategy<Value = TrackedCode> {
    prop_oneof![
        // === LEGACY GENERATORS (position-based) ===
        // Basic definition/reference scenarios (higher weight)
        3 => tracked_class_with_method_call(),
        3 => tracked_mixin_method_call(),
        3 => tracked_inheritance(),
        2 => tracked_instance_variable(),
        2 => tracked_nested_constant(),
        2 => tracked_multi_class(),
        2 => tracked_prepend_override(),
        2 => tracked_extend(),
        // Complex mixin scenarios
        1 => tracked_diamond_mixin(),
        1 => tracked_deep_include_chain(),
        1 => tracked_mixin_counts(),
        1 => tracked_mixin_edge_cases(),
        // Completion scenarios with user-defined methods (no builtins)
        2 => tracked_self_completion(),
        2 => tracked_attr_completion(),
        2 => tracked_mixin_completion(),
        2 => tracked_instance_completion(),
        2 => tracked_completion_through_mixins(),
        // Type inference scenarios - should survive edits
        2 => tracked_type_inference_literals(),
        2 => tracked_type_inference_in_method(),
        2 => tracked_type_inference_ivars(),
        2 => tracked_type_inference_multi_assign(),

        // === GRAPH GROWTH GENERATORS (anchor-based, dynamically located) ===
        // These use unique identifiers and anchor comments for robust position tracking
        3 => graph_class_hierarchy().prop_map(|v2| v2.to_legacy()),
        3 => graph_mixin_relationships().prop_map(|v2| v2.to_legacy()),
        2 => graph_type_inference().prop_map(|v2| v2.to_legacy()),
        2 => graph_class_references().prop_map(|v2| v2.to_legacy()),
        2 => graph_completion_test().prop_map(|v2| v2.to_legacy()),

        // === STRICT TYPE INFERENCE GENERATORS (known to expose bugs) ===
        // These test method chains, array access, and complex type scenarios
        3 => graph_method_chain_types().prop_map(|v2| v2.to_legacy()),
        3 => graph_array_access_types().prop_map(|v2| v2.to_legacy()),
        2 => graph_class_method_types().prop_map(|v2| v2.to_legacy()),
        2 => graph_ivar_type_propagation().prop_map(|v2| v2.to_legacy()),
        2 => graph_completion_after_type().prop_map(|v2| v2.to_legacy()),
        1 => graph_type_edge_cases().prop_map(|v2| v2.to_legacy()),
    ]
}

// =============================================================================
// SAFE EDIT GENERATORS
// =============================================================================
//
// These generators produce edits that can be deterministically tracked.
// They are designed to NOT destroy markers and to have predictable position
// adjustments.

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
    pub fn to_edit(&self, tracked: &TrackedCode) -> (Range, String) {
        match self {
            SafeEdit::InsertBlankLine { line } => {
                // Insert at the beginning of the line
                let insert_line =
                    (*line).min(tracked.code.lines().count().saturating_sub(1) as u32);
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
                let insert_line =
                    (*line).min(tracked.code.lines().count().saturating_sub(1) as u32);
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
                    (*before_end_line).min(tracked.code.lines().count().saturating_sub(1) as u32);
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
                let line_count = tracked.code.lines().count() as u32;
                let last_line_len =
                    tracked.code.lines().last().map(|l| l.len()).unwrap_or(0) as u32;
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
pub fn safe_edit_blank_line(tracked: &TrackedCode) -> impl Strategy<Value = SafeEdit> {
    let safe_line = tracked.find_safe_edit_line().unwrap_or(0);
    Just(SafeEdit::InsertBlankLine { line: safe_line })
}

/// Generate a safe edit that inserts a comment
pub fn safe_edit_comment(tracked: &TrackedCode) -> impl Strategy<Value = SafeEdit> {
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
            text: "# appended comment".to_string()
        }),
        ruby_identifier().prop_map(|name| SafeEdit::AppendToFile {
            text: format!("# variable: {}", name),
        }),
    ]
}

/// Generate any safe edit for a tracked code
pub fn safe_edit_for(tracked: &TrackedCode) -> BoxedStrategy<SafeEdit> {
    let safe_line = tracked.find_safe_edit_line().unwrap_or(0);

    prop_oneof![
        // Insert blank line (most common, simplest)
        5 => Just(SafeEdit::InsertBlankLine { line: safe_line }),
        // Insert comment
        3 => "[a-z ]{1,20}".prop_map(move |text| SafeEdit::InsertComment {
            line: safe_line,
            text,
        }),
        // Append to file
        2 => Just(SafeEdit::AppendToFile {
            text: "\n# appended".to_string()
        }),
    ]
    .boxed()
}

/// Calculate the expected position after applying a series of safe edits
pub fn expected_position_after_edits(
    original_position: Position,
    edits: &[(Range, String)],
) -> Position {
    let mut position = original_position;
    for (range, new_text) in edits {
        if let Some(new_pos) = adjust_position_after_edit(position, range, new_text) {
            position = new_pos;
        }
    }
    position
}

// =============================================================================
// POSITION ADJUSTMENT TESTS
// =============================================================================

#[cfg(test)]
mod position_adjustment_tests {
    use super::*;

    #[test]
    fn test_position_before_edit_unchanged() {
        let pos = Position {
            line: 0,
            character: 5,
        };
        let edit_range = Range {
            start: Position {
                line: 2,
                character: 0,
            },
            end: Position {
                line: 2,
                character: 5,
            },
        };
        let result = adjust_position_after_edit(pos, &edit_range, "replaced");
        assert_eq!(result, Some(pos));
    }

    #[test]
    fn test_position_after_edit_line_shifted() {
        let pos = Position {
            line: 5,
            character: 10,
        };
        // Insert a new line at line 2
        let edit_range = Range {
            start: Position {
                line: 2,
                character: 0,
            },
            end: Position {
                line: 2,
                character: 0,
            },
        };
        let result = adjust_position_after_edit(pos, &edit_range, "new line\n");
        assert_eq!(
            result,
            Some(Position {
                line: 6,
                character: 10
            })
        );
    }

    #[test]
    fn test_position_inside_edit_destroyed() {
        let pos = Position {
            line: 2,
            character: 3,
        };
        let edit_range = Range {
            start: Position {
                line: 2,
                character: 0,
            },
            end: Position {
                line: 2,
                character: 10,
            },
        };
        let result = adjust_position_after_edit(pos, &edit_range, "X");
        assert_eq!(result, None);
    }

    #[test]
    fn test_position_on_same_line_after_edit() {
        let pos = Position {
            line: 2,
            character: 15,
        };
        // Edit at the start of line 2
        let edit_range = Range {
            start: Position {
                line: 2,
                character: 0,
            },
            end: Position {
                line: 2,
                character: 5,
            },
        };
        // Replace 5 chars with 10 chars (net +5)
        let result = adjust_position_after_edit(pos, &edit_range, "0123456789");
        assert_eq!(
            result,
            Some(Position {
                line: 2,
                character: 20
            })
        );
    }

    #[test]
    fn test_multiline_edit_collapse() {
        let pos = Position {
            line: 5,
            character: 10,
        };
        // Delete lines 2-3 (2 lines)
        let edit_range = Range {
            start: Position {
                line: 2,
                character: 0,
            },
            end: Position {
                line: 4,
                character: 0,
            },
        };
        let result = adjust_position_after_edit(pos, &edit_range, "");
        assert_eq!(
            result,
            Some(Position {
                line: 3,
                character: 10
            })
        );
    }

    #[test]
    fn test_tracked_code_apply_edit() {
        let mut tracked = TrackedCode {
            code: "class Foo\n  def bar\n    nil\n  end\nend".to_string(),
            markers: vec![
                SymbolMarker {
                    name: "Foo".to_string(),
                    position: Position {
                        line: 0,
                        character: 6,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
                SymbolMarker {
                    name: "bar".to_string(),
                    position: Position {
                        line: 1,
                        character: 6,
                    },
                    kind: MarkerKind::Definition,
                    definition_position: None,
                },
            ],
            filename: "test.rb".to_string(),
            edit_count: 0,
        };

        // Insert a blank line at line 0
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
        tracked.apply_edit(&range, "\n");

        // Both markers should shift down by 1
        assert_eq!(tracked.markers[0].position.line, 1);
        assert_eq!(tracked.markers[1].position.line, 2);
        assert_eq!(tracked.edit_count, 1);
    }
}

// =============================================================================
// SOURCE LOCATOR TESTS (Graph Growth Strategy)
// =============================================================================

#[cfg(test)]
mod source_locator_tests {
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

    #[test]
    fn test_generator_state_unique_ids() {
        let mut state = GeneratorState::new();

        let name1 = state.make_class_name();
        let name2 = state.make_class_name();
        let name3 = state.make_module_name();

        // All names should be unique
        assert_ne!(name1, name2);
        assert_ne!(name2, name3);
        assert_ne!(name1, name3);
    }

    #[test]
    fn test_tracked_code_v2_to_legacy_conversion() {
        let mut state = GeneratorState::new();

        // Create a simple class
        let class_name = state.make_class_name(); // Class_0
        state.define_base_class(&class_name);

        // Create a reference to it
        let (ref_code, _anchor) = state.make_class_reference(&class_name);
        state.emit(&format!("_ = {}", ref_code));

        let v2 = TrackedCodeV2::from_state(state, "test.rb".to_string());
        let legacy = v2.to_legacy();

        // Should have markers for the class definition and reference
        assert!(legacy.markers.len() >= 2);

        // Find the definition marker
        let def_marker = legacy
            .markers
            .iter()
            .find(|m| m.name == "Class_0" && m.kind == MarkerKind::Definition);
        assert!(
            def_marker.is_some(),
            "Should have Class_0 definition marker"
        );

        // Find the reference marker
        let ref_marker = legacy
            .markers
            .iter()
            .find(|m| m.name == "Class_0" && m.kind == MarkerKind::Reference);
        assert!(ref_marker.is_some(), "Should have Class_0 reference marker");
    }
}
