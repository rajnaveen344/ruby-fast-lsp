use std::sync::{Arc, Mutex};

use lsp_types::{Location as LspLocation, Url};
use ruby_prism::{
    visit_class_node, visit_constant_path_write_node, visit_constant_write_node, visit_def_node,
    visit_local_variable_write_node, visit_module_node, visit_parameters_node, ClassNode,
    ConstantPathWriteNode, ConstantWriteNode, DefNode, LocalVariableWriteNode, ModuleNode, Visit,
};

use crate::indexer::index::RubyIndex;
use crate::server::RubyLanguageServer;
use crate::types::{
    ruby_document::RubyDocument, ruby_method::RubyMethod, ruby_namespace::RubyConstant,
    scope_kind::LVScopeDepth,
};

mod class_node;
mod constant_path_write_node;
mod constant_write_node;
mod def_node;
mod local_variable_write_node;
mod module_node;
mod parameters_node;
mod singleton_class_node;

pub struct IndexVisitor {
    pub index: Arc<Mutex<RubyIndex>>,
    pub uri: Url,
    pub document: RubyDocument,
    pub namespace_stack: Vec<RubyConstant>,
    pub scope_stack: Vec<()>,
    pub scope_depth: LVScopeDepth,
    pub current_method: Option<RubyMethod>,
}

impl IndexVisitor {
    pub fn new(server: &RubyLanguageServer, uri: Url) -> Self {
        let document = server.docs.lock().unwrap().get(&uri).unwrap().clone();
        Self {
            index: server.index(),
            uri,
            document,
            namespace_stack: vec![],
            scope_stack: vec![],
            scope_depth: 0,
            current_method: None,
        }
    }

    pub fn prism_loc_to_lsp_loc(&self, loc: ruby_prism::Location) -> LspLocation {
        let uri = self.uri.clone();
        let range = self.document.prism_location_to_lsp_range(&loc);
        LspLocation::new(uri, range)
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

    fn visit_def_node(&mut self, node: &DefNode) {
        self.process_def_node_entry(node);
        visit_def_node(self, node);
        self.process_def_node_exit(node);
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

    fn visit_parameters_node(&mut self, node: &ruby_prism::ParametersNode<'_>) {
        self.process_parameters_node_entry(node);
        visit_parameters_node(self, node);
        self.process_parameters_node_exit(node);
    }
}
