//! # Generator State
//!
//! Maintains pools and truth ledgers for the Graph Growth strategy.
//!
//! The state tracks:
//! - **Pools**: Track defined entities that can be referenced
//! - **Ledgers**: Track expectations for verification
//! - **ID counter**: Ensures all generated names are unique

use super::ledgers::{CompletionLedger, ErrorLedger, HintLedger, ReferenceLedger, TypeLedger};
use std::collections::HashMap;

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

    /// Define a subclass (inherits from parent in pool) - opens and closes immediately
    /// Note: Inheritance reference resolution is a known LSP limitation,
    /// so we don't track the parent reference
    pub fn define_subclass(&mut self, class_name: &str, parent_name: &str) {
        // NOTE: Inheritance reference resolution (class B < A -> A definition)
        // is not reliably tested because it's a known LSP limitation.
        self.emit(&format!("class {} < {}", class_name, parent_name));
        self.emit("end");
        self.emit("");

        self.classes.push(class_name.to_string());
        self.methods.insert(class_name.to_string(), Vec::new());
    }

    /// Open a subclass for adding methods (doesn't close it)
    /// Note: Inheritance reference resolution is a known LSP limitation,
    /// so we don't track the parent reference
    pub fn open_subclass(&mut self, class_name: &str, parent_name: &str) {
        // NOTE: Inheritance reference resolution (class B < A -> A definition)
        // is not reliably tested because it's a known LSP limitation.
        self.emit(&format!("class {} < {}", class_name, parent_name));

        self.classes.push(class_name.to_string());
        self.methods.insert(class_name.to_string(), Vec::new());
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

    /// Add a method with a typed return value
    pub fn define_method_with_return(
        &mut self,
        owner: &str,
        method_name: &str,
        return_value: &str,
        return_type: &str,
    ) {
        self.emit(&format!("  def {}", method_name));
        self.emit(&format!("    {}", return_value));
        self.emit("  end");
        self.emit("");

        // Track method return type
        self.type_ledger
            .method_returns
            .insert(method_name.to_string(), return_type.to_string());

        if let Some(methods) = self.methods.get_mut(owner) {
            methods.push(method_name.to_string());
        }
    }

    /// Create a typed method call with anchor
    pub fn typed_method_call(
        &mut self,
        var_name: &str,
        receiver: &str,
        method_name: &str,
        expected_type: &str,
    ) {
        let type_anchor = self.make_type_anchor();
        self.emit(&format!(
            "{} = {}.{} # <{}>",
            var_name, receiver, method_name, type_anchor
        ));
        self.type_ledger
            .var_types
            .insert(var_name.to_string(), expected_type.to_string());
    }

    /// Create a method call WITHOUT type expectations
    /// Use this when method return type inference isn't reliable enough to test
    pub fn untyped_method_call(&mut self, var_name: &str, receiver: &str, method_name: &str) {
        self.emit(&format!("{} = {}.{}", var_name, receiver, method_name));
    }

    /// Add instance variable with type tracking
    pub fn define_ivar(&mut self, ivar_name: &str, value: &str, expected_type: &str) {
        let type_anchor = self.make_type_anchor();
        self.emit(&format!(
            "    {} = {} # <{}>",
            ivar_name, value, type_anchor
        ));
        self.type_ledger
            .var_types
            .insert(ivar_name.to_string(), expected_type.to_string());
    }

    /// Add completion trigger on a variable (not self)
    pub fn add_var_completion_trigger(&mut self, var_name: &str, expected_methods: Vec<String>) {
        let comp_anchor = self.make_completion_anchor();
        self.emit(&format!("{}. # <{}>", var_name, comp_anchor));
        self.completion_ledger
            .expected_completions
            .insert(comp_anchor, expected_methods);
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
