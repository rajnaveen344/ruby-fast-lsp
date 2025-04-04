use std::sync::{Arc, Mutex};

use lsp_types::{Location as LspLocation, Position, Range, Url};
use ruby_prism::{
    visit_call_node, visit_class_node, visit_def_node, visit_module_node, CallNode, ClassNode,
    DefNode, ModuleNode, Visit,
};

use super::{
    entry::{Entry, MethodVisibility},
    index::RubyIndex,
    types::ruby_namespace::RubyNamespace,
};

mod call_node;
mod class_node;
mod def_node;
mod module_node;
mod singleton_class_node;
mod utils;

pub struct Visitor {
    pub index: Arc<Mutex<RubyIndex>>,
    pub uri: Url,
    pub content: String,
    pub namespace_stack: Vec<RubyNamespace>,
    pub _visibility_stack: Vec<MethodVisibility>,
    pub _current_method: Option<String>,
    pub _owner_stack: Vec<Entry>,
}

impl Visitor {
    pub fn new(index: Arc<Mutex<RubyIndex>>, uri: Url, content: String) -> Self {
        Self {
            index,
            uri,
            content,
            namespace_stack: vec![],
            _visibility_stack: vec![MethodVisibility::Public],
            _current_method: None,
            _owner_stack: vec![],
        }
    }

    pub fn prism_loc_to_lsp_loc(&self, loc: ruby_prism::Location) -> LspLocation {
        let start_offset = loc.start_offset();
        let end_offset = loc.end_offset();
        let uri = self.uri.clone();

        // Calculate correct line and character positions by counting newlines
        let (start_line, start_character) = self.offset_to_position(&self.content, start_offset);
        let (end_line, end_character) = self.offset_to_position(&self.content, end_offset);

        let range = Range::new(
            Position::new(start_line, start_character),
            Position::new(end_line, end_character),
        );
        LspLocation::new(uri, range)
    }

    // Helper function to convert byte offset to (line, character) position
    fn offset_to_position(&self, content: &str, offset: usize) -> (u32, u32) {
        let mut line = 0;
        let mut line_start_offset = 0;

        // Find the line containing the offset by counting newlines
        for (i, c) in content.chars().take(offset).enumerate() {
            if c == '\n' {
                line += 1;
                line_start_offset = i + 1; // +1 to skip the newline character
            }
        }

        // Character offset within the line
        let character = (offset - line_start_offset) as u32;

        (line, character)
    }
}

impl Visit<'_> for Visitor {
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

    fn visit_call_node(&mut self, node: &CallNode) {
        self.process_call_node_entry(node);
        visit_call_node(self, node);
        self.process_call_node_exit(node);
    }
}
