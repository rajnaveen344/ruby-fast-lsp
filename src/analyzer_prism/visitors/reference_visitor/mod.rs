use ruby_prism::*;

use crate::analyzer_prism::scope_tracker::ScopeTracker;
use crate::indexer::index_ref::{Index, Unlocked};
use crate::types::ruby_document::RubyDocument;

mod block_node;
mod call_node;
mod class_node;
mod constant_path_node;
mod constant_read_node;
mod def_node;
mod local_variable_read_node;
mod module_node;

#[cfg(test)]
mod tests;

pub struct ReferenceVisitor {
    pub index: Index<Unlocked>,
    pub document: RubyDocument,
    pub scope_tracker: ScopeTracker,
    pub include_local_vars: bool,
    /// When true, track unresolved constants in the index for diagnostics
    pub track_unresolved: bool,
}

impl ReferenceVisitor {
    pub fn new(index: Index<Unlocked>, document: RubyDocument) -> Self {
        Self::with_options(index, document, true)
    }

    pub fn with_options(
        index: Index<Unlocked>,
        document: RubyDocument,
        include_local_vars: bool,
    ) -> Self {
        let scope_tracker = ScopeTracker::new(&document);
        Self {
            index,
            document,
            scope_tracker,
            include_local_vars,
            track_unresolved: false,
        }
    }

    /// Create a visitor that tracks unresolved constants
    pub fn with_unresolved_tracking(
        index: Index<Unlocked>,
        document: RubyDocument,
        include_local_vars: bool,
    ) -> Self {
        let scope_tracker = ScopeTracker::new(&document);
        Self {
            index,
            document,
            scope_tracker,
            include_local_vars,
            track_unresolved: true,
        }
    }
}

impl Visit<'_> for ReferenceVisitor {
    fn visit_module_node(&mut self, node: &ModuleNode) {
        self.process_module_node_entry(node);
        visit_module_node(self, node);
        self.process_module_node_exit(node);
    }

    fn visit_class_node(&mut self, node: &ClassNode) {
        self.process_class_node_entry(node);
        visit_class_node(self, node);
        self.process_class_node_exit(node);
    }

    fn visit_def_node(&mut self, node: &DefNode) {
        self.process_def_node_entry(node);
        visit_def_node(self, node);
        self.process_def_node_exit(node);
    }

    fn visit_block_node(&mut self, node: &BlockNode) {
        self.process_block_node_entry(node);
        visit_block_node(self, node);
        self.process_block_node_exit(node);
    }

    fn visit_constant_path_node(&mut self, node: &ConstantPathNode) {
        self.process_constant_path_node_entry(node);
        visit_constant_path_node(self, node);
        self.process_constant_path_node_exit(node);
    }

    fn visit_constant_read_node(&mut self, node: &ConstantReadNode) {
        self.process_constant_read_node_entry(node);
        visit_constant_read_node(self, node);
        self.process_constant_read_node_exit(node);
    }

    fn visit_local_variable_read_node(&mut self, node: &LocalVariableReadNode) {
        self.process_local_variable_read_node_entry(node);
        visit_local_variable_read_node(self, node);
        self.process_local_variable_read_node_exit(node);
    }

    fn visit_call_node(&mut self, node: &CallNode) {
        self.process_call_node_entry(node);
        visit_call_node(self, node);
        self.process_call_node_exit(node);
    }
}
