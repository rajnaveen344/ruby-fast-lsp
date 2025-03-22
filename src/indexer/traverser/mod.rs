use std::sync::{Arc, Mutex};

use log::info;
use lsp_types::{Location as LspLocation, Position, Range, Url};
use ruby_prism::{
    visit_call_node, visit_class_node, visit_def_node, visit_module_node,
    visit_singleton_class_node, CallNode, ClassNode, DefNode, Location as PrismLocation,
    ModuleNode, SingletonClassNode, Visit,
};

use crate::indexer::entry::{EntryBuilder, EntryType};

use super::{
    entry::{Entry, Visibility},
    index::RubyIndex,
    types::{constant::Constant, fully_qualified_constant::FullyQualifiedName, method::Method},
};

pub struct Visitor {
    pub index: Arc<Mutex<RubyIndex>>,
    pub uri: Url,
    pub content: String,
    pub visibility_stack: Vec<Visibility>,
    pub current_method: Option<String>,
    pub namespace_stack: Vec<Constant>,
    pub owner_stack: Vec<Entry>,
}

impl Visitor {
    pub fn new(index: Arc<Mutex<RubyIndex>>, uri: Url, content: String) -> Self {
        Self {
            index,
            uri,
            content,
            visibility_stack: vec![Visibility::Public],
            current_method: None,
            namespace_stack: vec![],
            owner_stack: vec![],
        }
    }

    fn prism_loc_to_lsp_loc(&self, loc: PrismLocation) -> LspLocation {
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

    fn push_namespace(&mut self, constant_name: Constant, entry: Entry) {
        self.namespace_stack.push(constant_name);
        self.visibility_stack.push(entry.visibility);
        self.owner_stack.push(entry.clone());
        self.index.lock().unwrap().add_entry(entry.clone());
    }

    fn pop_namespace(&mut self) {
        self.namespace_stack.pop();
        self.visibility_stack.pop();
        self.owner_stack.pop();
    }

    fn build_fully_qualified_name(
        &self,
        name: Constant,
        method: Option<Method>,
    ) -> FullyQualifiedName {
        if self.namespace_stack.is_empty() {
            FullyQualifiedName::new(vec![name], method)
        } else {
            let mut namespace = self.namespace_stack.clone();
            namespace.push(name);
            FullyQualifiedName::new(namespace, method)
        }
    }
}

impl Visit<'_> for Visitor {
    fn visit_module_node(&mut self, node: &ModuleNode) {
        info!(
            "Visiting module node: {}",
            String::from_utf8_lossy(node.name().as_slice())
        );
        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let const_path = node.constant_path();
        let full_loc = node.location();
        let name_loc = const_path.location();
        let fqn = self.build_fully_qualified_name(Constant::from(name.clone()), None);

        let entry = EntryBuilder::new(Constant::from(name.clone()))
            .fully_qualified_name(fqn.into())
            .location(self.prism_loc_to_lsp_loc(full_loc))
            .entry_type(EntryType::Module)
            .build()
            .unwrap();

        self.push_namespace(Constant::from(name), entry);

        visit_module_node(self, node);

        self.pop_namespace();
    }

    fn visit_class_node(&mut self, node: &ClassNode) {
        info!(
            "Visiting class node: {}",
            String::from_utf8_lossy(node.name().as_slice())
        );

        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let const_path = node.constant_path();
        let full_loc = node.location();
        let name_loc = const_path.location();
        let fqn = self.build_fully_qualified_name(Constant::from(name.clone()), None);

        // Extract parent class information if available
        let parent_class = if let Some(superclass) = node.superclass() {
            if let Some(cread) = superclass.as_constant_read_node() {
                Some(String::from_utf8_lossy(cread.name().as_slice()).to_string())
            } else if let Some(_) = superclass.as_constant_path_node() {
                // For constant path nodes, we can't easily access the name
                // Just record a marker for now
                Some("ParentClass".to_string())
            } else {
                None
            }
        } else {
            // Default parent is Object unless this is already Object
            if name != "Object" {
                Some("Object".to_string())
            } else {
                None
            }
        };

        let entry = EntryBuilder::new(Constant::from(name.clone()))
            .fully_qualified_name(fqn.into())
            .location(self.prism_loc_to_lsp_loc(full_loc))
            .entry_type(EntryType::Class)
            .build()
            .unwrap();

        self.push_namespace(Constant::from(name), entry);

        visit_class_node(self, node);

        self.pop_namespace();
    }

    fn visit_singleton_class_node(&mut self, node: &SingletonClassNode) {
        info!("Visiting singleton class node");

        // Get the current namespace
        let current_owner = self.owner_stack.last();

        if let Some(_owner) = current_owner {
            // Create a singleton class name for the current namespace
            let expression = node.expression();
            let is_self_node = expression.as_self_node().is_some();

            let current_name = if let Some(last_name) = self.namespace_stack.last() {
                last_name.to_string()
            } else {
                "Anonymous".to_string()
            };

            let singleton_name = if is_self_node {
                format!("<Class:{}>", current_name)
            } else {
                let expr_name = if let Some(cread) = expression.as_constant_read_node() {
                    String::from_utf8_lossy(cread.name().as_slice()).to_string()
                } else if let Some(_) = expression.as_constant_path_node() {
                    // For constant path nodes, we can't easily access the name
                    "Class".to_string()
                } else {
                    "Unknown".to_string()
                };
                format!("<Class:{}>", expr_name)
            };

            let fqn = self.build_fully_qualified_name(Constant::from(singleton_name.clone()), None);
            let location = self.prism_loc_to_lsp_loc(node.location());

            // Create a singleton class entry
            let entry = EntryBuilder::new(Constant::from(singleton_name.clone()))
                .fully_qualified_name(fqn.into())
                .location(location)
                .entry_type(EntryType::SingletonClass)
                .build()
                .unwrap();

            self.push_namespace(Constant::from(singleton_name), entry);

            visit_singleton_class_node(self, node);

            self.pop_namespace();
        } else {
            visit_singleton_class_node(self, node);
        }
    }

    fn visit_def_node(&mut self, node: &DefNode) {
        info!(
            "Visiting def node: {}",
            String::from_utf8_lossy(node.name().as_slice())
        );

        // Get the current owner namespace
        let owner = self.owner_stack.last();
        if owner.is_none() {
            visit_def_node(self, node);
            return;
        }

        // Extract the method name
        let method_name = String::from_utf8_lossy(node.name().as_slice()).to_string();

        // Store the current method name for param processing
        self.current_method = Some(method_name.clone());

        // Determine method visibility
        let visibility = self
            .visibility_stack
            .last()
            .cloned()
            .unwrap_or(Visibility::Public);

        // Get receiver information to determine if it's a singleton method
        let is_singleton_method = node.receiver().is_some();

        if is_singleton_method {
            // Handle singleton methods (class methods)
            if let Some(receiver) = node.receiver() {
                if receiver.as_self_node().is_some() {
                    // This is a class method (defined with self.)
                    if let Some(owner) = owner.cloned() {
                        // Create singleton class entry to use as the owner
                        let owner_name = owner.constant_name.to_string();
                        let singleton_name = format!("<Class:{}>", owner_name);
                        let singleton_fqn = self.build_fully_qualified_name(
                            Constant::from(singleton_name.clone()),
                            None,
                        );

                        // Create method entry and add to index
                        let fqn = FullyQualifiedName::new(
                            vec![],
                            Some(Method::from(method_name.clone())),
                        );
                        let method_location = self.prism_loc_to_lsp_loc(node.location());
                        let _method_name_location = self.prism_loc_to_lsp_loc(node.name_loc());

                        let method_entry = EntryBuilder::new(Constant::from(method_name))
                            .fully_qualified_name(fqn)
                            .location(method_location)
                            .entry_type(EntryType::Method)
                            .visibility(visibility)
                            .build()
                            .unwrap();

                        self.index.lock().unwrap().add_entry(method_entry);
                    }
                }
            }
        } else {
            // Regular instance method
            if let Some(owner) = owner.cloned() {
                let method_location = self.prism_loc_to_lsp_loc(node.location());
                let _method_name_location = self.prism_loc_to_lsp_loc(node.name_loc());
                let fqn = FullyQualifiedName::new(vec![], Some(Method::from(method_name.clone())));

                let method_entry = EntryBuilder::new(Constant::from(method_name))
                    .fully_qualified_name(fqn)
                    .location(method_location)
                    .entry_type(EntryType::Method)
                    .visibility(visibility)
                    .build()
                    .unwrap();

                self.index.lock().unwrap().add_entry(method_entry);
            }
        }

        visit_def_node(self, node);

        // Clear the current method
        self.current_method = None;
    }

    fn visit_call_node(&mut self, node: &CallNode) {
        info!(
            "Visiting call node: {}",
            String::from_utf8_lossy(node.name().as_slice())
        );

        let message = String::from_utf8_lossy(node.name().as_slice()).to_string();

        // Handle special method calls
        match message.as_str() {
            "private" => {
                self.visibility_stack.push(Visibility::Private);
            }
            "protected" => {
                self.visibility_stack.push(Visibility::Protected);
            }
            "public" => {
                self.visibility_stack.push(Visibility::Public);
            }
            "attr_reader" | "attr_writer" | "attr_accessor" => {
                // Handle attribute methods
                if let Some(_owner) = self.owner_stack.last().cloned() {
                    // Process attribute declarations
                    if let Some(_args) = node.arguments() {
                        // Implement attr_* handling
                        // This is simplified; a complete implementation would traverse all arguments
                        // and handle string/symbol arguments correctly
                    }
                }
            }
            "include" | "prepend" | "extend" => {
                // Handle module operations
                // Simplified implementation; would need to extract included module names
            }
            _ => {
                // Regular method call
            }
        }

        visit_call_node(self, node);

        // Clean up visibility stack on leaving the special method call
        match message.as_str() {
            "private" | "protected" | "public" => {
                // Only pop if we're not leaving a method def with this visibility
                if !node.arguments().map_or(false, |args| {
                    args.arguments()
                        .iter()
                        .any(|arg| arg.as_def_node().is_some())
                }) {
                    self.visibility_stack.pop();
                }
            }
            _ => {}
        }
    }
}
