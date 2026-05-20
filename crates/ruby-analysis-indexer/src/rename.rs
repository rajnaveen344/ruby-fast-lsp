//! AST-based rename visitor for local variables.
//!
//! Uses Prism's `depth` field to reliably identify all occurrences of a local
//! variable that refer to the same definition. This is more reliable than
//! stored positions because it uses the parser's own scope resolution as the
//! source of truth.
//!
//! ## Algorithm
//!
//! Each scope-creating node (def, class, module, block, lambda) gets a unique
//! scope ID. For any local variable node with `depth=N`, its defining scope ID
//! is `scope_stack[scope_stack.len() - 1 - N]`. Two nodes refer to the same
//! variable iff they have the same name AND the same defining scope ID.
//!
//! The visitor runs in two phases:
//! 1. **FindTarget**: Traverse to find the variable at the cursor position,
//!    record its name and defining scope ID.
//! 2. **Collect**: Re-traverse, collecting all nodes with matching name and
//!    defining scope ID.

use ruby_prism::{
    visit_block_node, visit_block_parameter_node, visit_class_node, visit_def_node,
    visit_lambda_node, visit_local_variable_and_write_node,
    visit_local_variable_operator_write_node, visit_local_variable_or_write_node,
    visit_local_variable_write_node, visit_module_node, visit_optional_parameter_node,
    visit_singleton_class_node, BlockNode, BlockParameterNode, ClassNode, DefNode,
    KeywordRestParameterNode, LambdaNode, LocalVariableAndWriteNode,
    LocalVariableOperatorWriteNode, LocalVariableOrWriteNode, LocalVariableReadNode,
    LocalVariableTargetNode, LocalVariableWriteNode, ModuleNode, Node, OptionalParameterNode,
    RequiredParameterNode, RestParameterNode, SingletonClassNode, Visit,
};
use tower_lsp::lsp_types::Range;

use crate::RubyDocument;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    FindTarget,
    Collect,
}

pub struct RenameVisitor {
    document: RubyDocument,
    cursor_offset: usize,
    // Scope tracking — each scope-creating node gets a unique ID
    scope_stack: Vec<usize>,
    next_scope_id: usize,
    // Phase 1 output: the variable we're renaming
    target_name: Option<String>,
    target_scope_id: Option<usize>,
    // Phase 2 output: all ranges to rename
    rename_ranges: Vec<Range>,
    phase: Phase,
}

impl RenameVisitor {
    /// Find all rename target ranges for the local variable at `cursor_offset`.
    ///
    /// Returns an empty Vec if the cursor is not on a local variable.
    pub fn find_rename_targets(
        document: RubyDocument,
        cursor_offset: usize,
        root: &Node,
    ) -> Vec<Range> {
        // Phase 1: Find the target variable
        let mut visitor = Self {
            document,
            cursor_offset,
            scope_stack: vec![0], // Program-level scope
            next_scope_id: 1,
            target_name: None,
            target_scope_id: None,
            rename_ranges: Vec::new(),
            phase: Phase::FindTarget,
        };
        visitor.visit(root);

        if visitor.target_name.is_none() || visitor.target_scope_id.is_none() {
            return Vec::new();
        }

        // Phase 2: Collect all matching occurrences
        visitor.phase = Phase::Collect;
        visitor.scope_stack = vec![0];
        visitor.next_scope_id = 1;
        visitor.visit(root);

        visitor.rename_ranges
    }

    fn push_scope(&mut self) {
        let id = self.next_scope_id;
        self.next_scope_id += 1;
        self.scope_stack.push(id);
    }

    fn pop_scope(&mut self) {
        self.scope_stack.pop();
    }

    /// Resolve the defining scope ID for a variable with the given depth.
    /// depth=0 means defined in the current scope, depth=1 means one scope up, etc.
    fn defining_scope_id(&self, depth: u32) -> Option<usize> {
        let idx = self.scope_stack.len().checked_sub(1 + depth as usize)?;
        self.scope_stack.get(idx).copied()
    }

    fn cursor_in_location(&self, location: &ruby_prism::Location) -> bool {
        self.cursor_offset >= location.start_offset() && self.cursor_offset < location.end_offset()
    }

    /// Process a local variable node (read, write, target, compound assignment).
    fn process_local_var(
        &mut self,
        name_bytes: &[u8],
        depth: u32,
        name_range_location: &ruby_prism::Location,
    ) {
        let defining_scope = match self.defining_scope_id(depth) {
            Some(id) => id,
            None => return,
        };

        match self.phase {
            Phase::FindTarget => {
                if self.target_name.is_none() && self.cursor_in_location(name_range_location) {
                    self.target_name = Some(String::from_utf8_lossy(name_bytes).to_string());
                    self.target_scope_id = Some(defining_scope);
                }
            }
            Phase::Collect => {
                if let (Some(ref target_name), Some(target_scope)) =
                    (&self.target_name, self.target_scope_id)
                {
                    let name = String::from_utf8_lossy(name_bytes);
                    if name.as_ref() == target_name.as_str() && defining_scope == target_scope {
                        let range = self
                            .document
                            .prism_location_to_lsp_range(name_range_location);
                        self.rename_ranges.push(range);
                    }
                }
            }
        }
    }

    /// Process a parameter node (always depth=0 in its enclosing scope).
    fn process_parameter(&mut self, name_bytes: &[u8], location: &ruby_prism::Location) {
        self.process_local_var(name_bytes, 0, location);
    }
}

impl Visit<'_> for RenameVisitor {
    // ── Scope-creating nodes ────────────────────────────────────────────

    fn visit_def_node(&mut self, node: &DefNode) {
        self.push_scope();
        visit_def_node(self, node);
        self.pop_scope();
    }

    fn visit_class_node(&mut self, node: &ClassNode) {
        self.push_scope();
        visit_class_node(self, node);
        self.pop_scope();
    }

    fn visit_module_node(&mut self, node: &ModuleNode) {
        self.push_scope();
        visit_module_node(self, node);
        self.pop_scope();
    }

    fn visit_block_node(&mut self, node: &BlockNode) {
        self.push_scope();
        visit_block_node(self, node);
        self.pop_scope();
    }

    fn visit_lambda_node(&mut self, node: &LambdaNode) {
        self.push_scope();
        visit_lambda_node(self, node);
        self.pop_scope();
    }

    fn visit_singleton_class_node(&mut self, node: &SingletonClassNode) {
        self.push_scope();
        visit_singleton_class_node(self, node);
        self.pop_scope();
    }

    // NOTE: ForNode does NOT create a scope in Ruby — variables leak out.
    // We intentionally do not override visit_for_node.

    // ── Local variable nodes ────────────────────────────────────────────

    fn visit_local_variable_read_node(&mut self, node: &LocalVariableReadNode) {
        self.process_local_var(node.name().as_slice(), node.depth(), &node.location());
        // Leaf node — no children to visit
    }

    fn visit_local_variable_write_node(&mut self, node: &LocalVariableWriteNode) {
        // Use name_loc() to only rename the name, not the entire assignment
        self.process_local_var(node.name().as_slice(), node.depth(), &node.name_loc());
        visit_local_variable_write_node(self, node);
    }

    fn visit_local_variable_target_node(&mut self, node: &LocalVariableTargetNode) {
        self.process_local_var(node.name().as_slice(), node.depth(), &node.location());
        // Leaf node — no children to visit
    }

    fn visit_local_variable_and_write_node(&mut self, node: &LocalVariableAndWriteNode) {
        self.process_local_var(node.name().as_slice(), node.depth(), &node.name_loc());
        visit_local_variable_and_write_node(self, node);
    }

    fn visit_local_variable_or_write_node(&mut self, node: &LocalVariableOrWriteNode) {
        self.process_local_var(node.name().as_slice(), node.depth(), &node.name_loc());
        visit_local_variable_or_write_node(self, node);
    }

    fn visit_local_variable_operator_write_node(&mut self, node: &LocalVariableOperatorWriteNode) {
        self.process_local_var(node.name().as_slice(), node.depth(), &node.name_loc());
        visit_local_variable_operator_write_node(self, node);
    }

    // ── Parameter nodes (always depth=0) ────────────────────────────────

    fn visit_required_parameter_node(&mut self, node: &RequiredParameterNode) {
        self.process_parameter(node.name().as_slice(), &node.location());
        // Leaf node
    }

    fn visit_optional_parameter_node(&mut self, node: &OptionalParameterNode) {
        self.process_parameter(node.name().as_slice(), &node.name_loc());
        visit_optional_parameter_node(self, node);
    }

    fn visit_rest_parameter_node(&mut self, node: &RestParameterNode) {
        if let (Some(name), Some(name_loc)) = (node.name(), node.name_loc()) {
            self.process_parameter(name.as_slice(), &name_loc);
        }
        // Leaf node
    }

    fn visit_block_parameter_node(&mut self, node: &BlockParameterNode) {
        if let (Some(name), Some(name_loc)) = (node.name(), node.name_loc()) {
            self.process_parameter(name.as_slice(), &name_loc);
        }
        visit_block_parameter_node(self, node);
    }

    fn visit_keyword_rest_parameter_node(&mut self, node: &KeywordRestParameterNode) {
        if let (Some(name), Some(name_loc)) = (node.name(), node.name_loc()) {
            self.process_parameter(name.as_slice(), &name_loc);
        }
        // Leaf node
    }
}
