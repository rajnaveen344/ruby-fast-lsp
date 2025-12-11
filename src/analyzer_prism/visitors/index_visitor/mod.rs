use std::sync::Arc;

use parking_lot::Mutex;
use ruby_prism::*;
use tower_lsp::lsp_types::Url;

use crate::analyzer_prism::scope_tracker::ScopeTracker;
use crate::indexer::index::RubyIndex;
use crate::server::RubyLanguageServer;
use crate::type_inference::literal_analyzer::LiteralAnalyzer;

use crate::type_inference::ruby_type::RubyType;
use crate::types::ruby_document::RubyDocument;

mod block_node;
mod call_node;
mod class_node;
mod class_variable_write_node;
mod constant_path_write_node;
mod constant_write_node;
mod def_node;
mod global_variable_write_node;
mod instance_variable_write_node;
mod local_variable_write_node;
mod module_node;
mod parameters_node;
mod singleton_class_node;

pub struct IndexVisitor {
    pub index: Arc<Mutex<RubyIndex>>,
    pub document: RubyDocument,
    pub scope_tracker: ScopeTracker,
    pub literal_analyzer: LiteralAnalyzer,
}

impl IndexVisitor {
    pub fn new(server: &RubyLanguageServer, uri: Url) -> Self {
        let index = server.index();
        let document = server.get_doc(&uri).unwrap();
        let scope_tracker = ScopeTracker::new(&document);
        Self {
            index,
            document,
            scope_tracker,
            literal_analyzer: LiteralAnalyzer::new(),
        }
    }

    /// Infer type from a value node during indexing.
    /// This is the shared type inference logic used by all variable write nodes.
    pub fn infer_type_from_value(&self, value_node: &Node) -> RubyType {
        // Try literal analysis first
        if let Some(literal_type) = self.literal_analyzer.analyze_literal(value_node) {
            return literal_type;
        }

        // Try method call resolution
        // Optimization: Disabled for now to improve indexing performance on file open.
        // Deep resolution traversing ancestor chains for every local variable assignment is too slow.
        /*
        if let Some(call_node) = value_node.as_call_node() {
            let ns_stack = self.scope_tracker.get_ns_stack();
            log::debug!(
                "infer_type_from_value: method call detected, namespace stack: {:?}",
                ns_stack
            );
            // Pass current namespace context so 'self' can be resolved properly
            let resolver = MethodResolver::with_namespace(self.index.clone(), ns_stack);
            if let Some(return_type) = resolver.resolve_call_type(&call_node) {
                log::debug!("infer_type_from_value: resolved type: {:?}", return_type);
                return return_type;
            }
            log::debug!("infer_type_from_value: could not resolve type");
        }
        */

        // LIGHTWEIGHT OPTIMIZATION: Handle `Constant.new` without full resolution
        // This restores basic type inference for object instantiation which is critical for many tests
        if let Some(call_node) = value_node.as_call_node() {
            // LIGHTWEIGHT OPTIMIZATION: Handle `Constant.new` without full resolution
            if call_node.name().as_slice() == b"new" {
                if let Some(receiver) = call_node.receiver() {
                    // Check for simple constant receiver: `MyClass.new`
                    if let Some(const_node) = receiver.as_constant_read_node() {
                        let name =
                            String::from_utf8_lossy(const_node.name().as_slice()).to_string();
                        // We assume it's a class in the current scope or top level.
                        use crate::types::fully_qualified_name::FullyQualifiedName;
                        if let Ok(fqn) = FullyQualifiedName::try_from(name.as_str()) {
                            return RubyType::Class(fqn);
                        }
                    }
                }
            }
        }

        // Default to unknown type
        RubyType::Unknown
    }
}

impl Visit<'_> for IndexVisitor {
    fn visit_call_node(&mut self, node: &CallNode) {
        self.process_call_node_entry(node);
        visit_call_node(self, node);
        self.process_call_node_exit(node);
    }

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

    fn visit_singleton_class_node(&mut self, node: &SingletonClassNode) {
        self.process_singleton_class_node_entry(node);
        visit_singleton_class_node(self, node);
        self.process_singleton_class_node_exit(node);
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

    fn visit_constant_write_node(&mut self, node: &ConstantWriteNode) {
        self.process_constant_write_node_entry(node);
        visit_constant_write_node(self, node);
        self.process_constant_write_node_exit(node);
    }

    fn visit_constant_path_write_node(&mut self, node: &ConstantPathWriteNode) {
        self.process_constant_path_write_node_entry(node);
        visit_constant_path_write_node(self, node);
        self.process_constant_path_write_node_exit(node);
    }

    fn visit_local_variable_write_node(&mut self, node: &LocalVariableWriteNode) {
        self.process_local_variable_write_node_entry(node);
        visit_local_variable_write_node(self, node);
        self.process_local_variable_write_node_exit(node);
    }

    fn visit_local_variable_target_node(&mut self, node: &LocalVariableTargetNode) {
        self.process_local_variable_target_node_entry(node);
        visit_local_variable_target_node(self, node);
        self.process_local_variable_target_node_exit(node);
    }

    fn visit_local_variable_or_write_node(&mut self, node: &LocalVariableOrWriteNode) {
        self.process_local_variable_or_write_node_entry(node);
        visit_local_variable_or_write_node(self, node);
        self.process_local_variable_or_write_node_exit(node);
    }

    fn visit_local_variable_and_write_node(&mut self, node: &LocalVariableAndWriteNode) {
        self.process_local_variable_and_write_node_entry(node);
        visit_local_variable_and_write_node(self, node);
        self.process_local_variable_and_write_node_exit(node);
    }

    fn visit_local_variable_operator_write_node(&mut self, node: &LocalVariableOperatorWriteNode) {
        self.process_local_variable_operator_write_node_entry(node);
        visit_local_variable_operator_write_node(self, node);
        self.process_local_variable_operator_write_node_exit(node);
    }

    fn visit_parameters_node(&mut self, node: &ruby_prism::ParametersNode<'_>) {
        self.process_parameters_node_entry(node);
        visit_parameters_node(self, node);
        self.process_parameters_node_exit(node);
    }

    fn visit_class_variable_write_node(&mut self, node: &ClassVariableWriteNode) {
        self.process_class_variable_write_node_entry(node);
        visit_class_variable_write_node(self, node);
        self.process_class_variable_write_node_exit(node);
    }

    fn visit_class_variable_target_node(&mut self, node: &ClassVariableTargetNode) {
        self.process_class_variable_target_node_entry(node);
        visit_class_variable_target_node(self, node);
        self.process_class_variable_target_node_exit(node);
    }

    fn visit_class_variable_or_write_node(&mut self, node: &ClassVariableOrWriteNode) {
        self.process_class_variable_or_write_node_entry(node);
        visit_class_variable_or_write_node(self, node);
        self.process_class_variable_or_write_node_exit(node);
    }

    fn visit_class_variable_and_write_node(&mut self, node: &ClassVariableAndWriteNode) {
        self.process_class_variable_and_write_node_entry(node);
        visit_class_variable_and_write_node(self, node);
        self.process_class_variable_and_write_node_exit(node);
    }

    fn visit_class_variable_operator_write_node(&mut self, node: &ClassVariableOperatorWriteNode) {
        self.process_class_variable_operator_write_node_entry(node);
        visit_class_variable_operator_write_node(self, node);
        self.process_class_variable_operator_write_node_exit(node);
    }

    fn visit_instance_variable_write_node(&mut self, node: &InstanceVariableWriteNode) {
        self.process_instance_variable_write_node_entry(node);
        visit_instance_variable_write_node(self, node);
        self.process_instance_variable_write_node_exit(node);
    }

    fn visit_instance_variable_target_node(&mut self, node: &InstanceVariableTargetNode) {
        self.process_instance_variable_target_node_entry(node);
        visit_instance_variable_target_node(self, node);
        self.process_instance_variable_target_node_exit(node);
    }

    fn visit_instance_variable_or_write_node(&mut self, node: &InstanceVariableOrWriteNode) {
        self.process_instance_variable_or_write_node_entry(node);
        visit_instance_variable_or_write_node(self, node);
        self.process_instance_variable_or_write_node_exit(node);
    }

    fn visit_instance_variable_and_write_node(&mut self, node: &InstanceVariableAndWriteNode) {
        self.process_instance_variable_and_write_node_entry(node);
        visit_instance_variable_and_write_node(self, node);
        self.process_instance_variable_and_write_node_exit(node);
    }

    fn visit_instance_variable_operator_write_node(
        &mut self,
        node: &InstanceVariableOperatorWriteNode,
    ) {
        self.process_instance_variable_operator_write_node_entry(node);
        visit_instance_variable_operator_write_node(self, node);
        self.process_instance_variable_operator_write_node_exit(node);
    }

    fn visit_global_variable_write_node(&mut self, node: &GlobalVariableWriteNode) {
        self.process_global_variable_write_node_entry(node);
        visit_global_variable_write_node(self, node);
        self.process_global_variable_write_node_exit(node);
    }

    fn visit_global_variable_target_node(&mut self, node: &GlobalVariableTargetNode) {
        self.process_global_variable_target_node_entry(node);
        visit_global_variable_target_node(self, node);
        self.process_global_variable_target_node_exit(node);
    }

    fn visit_global_variable_or_write_node(&mut self, node: &GlobalVariableOrWriteNode) {
        self.process_global_variable_or_write_node_entry(node);
        visit_global_variable_or_write_node(self, node);
        self.process_global_variable_or_write_node_exit(node);
    }

    fn visit_global_variable_and_write_node(&mut self, node: &GlobalVariableAndWriteNode) {
        self.process_global_variable_and_write_node_entry(node);
        visit_global_variable_and_write_node(self, node);
        self.process_global_variable_and_write_node_exit(node);
    }

    fn visit_global_variable_operator_write_node(
        &mut self,
        node: &GlobalVariableOperatorWriteNode,
    ) {
        self.process_global_variable_operator_write_node_entry(node);
        visit_global_variable_operator_write_node(self, node);
        self.process_global_variable_operator_write_node_exit(node);
    }
}
