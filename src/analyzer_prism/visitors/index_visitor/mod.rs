use std::sync::{Arc, Mutex};

use lsp_types::{Location as LspLocation, Range, Url};
use ruby_prism::*;

use crate::indexer::index::RubyIndex;
use crate::server::RubyLanguageServer;
use crate::types::scope::{LVScope, LVScopeId, LVScopeKind, LVScopeStack};
use crate::types::{
    ruby_document::RubyDocument, ruby_method::RubyMethod, ruby_namespace::RubyConstant,
};

mod block_node;
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
    pub uri: Url,
    pub document: RubyDocument,
    pub namespace_stack: Vec<Vec<RubyConstant>>,
    pub in_singleton_node: bool,
    pub current_method: Option<RubyMethod>,
    pub scope_stack: LVScopeStack,
}

impl IndexVisitor {
    pub fn new(server: &RubyLanguageServer, uri: Url) -> Self {
        let document = server.docs.lock().unwrap().get(&uri).unwrap().clone();
        let lv_scope = LVScope::new(
            0,
            LspLocation {
                uri: uri.clone(),
                range: Range::new(
                    document.offset_to_position(0),
                    document.offset_to_position(document.content.len()),
                ),
            },
            LVScopeKind::TopLevel,
        );
        Self {
            index: server.index(),
            uri,
            document,
            namespace_stack: vec![],
            in_singleton_node: false,
            scope_stack: vec![lv_scope],
            current_method: None,
        }
    }

    pub fn prism_loc_to_lsp_loc(&self, loc: ruby_prism::Location) -> LspLocation {
        let uri = self.uri.clone();
        let range = self.document.prism_location_to_lsp_range(&loc);
        LspLocation::new(uri, range)
    }

    pub fn push_ns_scope(&mut self, namespace: RubyConstant) {
        self.namespace_stack.push(vec![namespace]);
    }

    fn push_ns_scopes(&mut self, namespaces: Vec<RubyConstant>) {
        self.namespace_stack.push(namespaces);
    }

    pub fn pop_ns_scope(&mut self) -> Option<Vec<RubyConstant>> {
        self.namespace_stack.pop()
    }

    pub fn current_namespace(&self) -> Vec<RubyConstant> {
        self.namespace_stack.iter().flatten().cloned().collect()
    }

    fn push_lv_scope(
        &mut self,
        scope_id: LVScopeId,
        location: lsp_types::Location,
        kind: LVScopeKind,
    ) {
        self.scope_stack
            .push(LVScope::new(scope_id, location, kind));
    }

    fn pop_lv_scope(&mut self) -> Option<LVScope> {
        self.scope_stack.pop()
    }
}

impl Visit<'_> for IndexVisitor {
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
