use crate::analyzer_prism::position::lsp_pos_to_prism_loc;
use crate::indexer::types::fully_qualified_name::FullyQualifiedName;
use crate::indexer::types::ruby_namespace::RubyNamespace;
use lsp_types::Position;
use ruby_prism::{
    visit_class_node, visit_module_node, ClassNode, ConstantPathNode, ConstantReadNode, Location,
    ModuleNode, Visit,
};

/// Visitor for finding identifiers at a specific position
pub struct IdentifierVisitor {
    code: String,
    position: Position,
    pub identifier: Option<FullyQualifiedName>,
    pub namespace_stack: Vec<RubyNamespace>,
}

impl IdentifierVisitor {
    pub fn new(code: String, position: Position) -> Self {
        Self {
            code,
            position,
            identifier: None,
            namespace_stack: Vec::new(),
        }
    }

    pub fn is_position_in_location(&self, location: &Location) -> bool {
        let position_offset = lsp_pos_to_prism_loc(self.position, &self.code);

        let start_offset = location.start_offset();
        let end_offset = location.end_offset();

        position_offset >= start_offset && position_offset < end_offset
    }

    /// Collect namespace parts from a constant path node in the correct order
    /// For example, for "Outer::Inner::Klass", it returns [Outer, Inner, Klass]
    pub fn collect_constant_path_names(&self, node: &ConstantPathNode) -> Vec<RubyNamespace> {
        let mut namespaces = Vec::new();
        self.collect_constant_path_names_recursive(node, &mut namespaces);
        namespaces
    }

    /// Helper method to recursively collect namespace parts from a constant path node
    fn collect_constant_path_names_recursive(
        &self,
        node: &ConstantPathNode,
        namespaces: &mut Vec<RubyNamespace>,
    ) {
        // The AST for Outer::Inner::Klass looks like:
        // ConstantPathNode (Klass)
        //   - parent: ConstantPathNode (Inner)
        //     - parent: ConstantReadNode (Outer)

        // First, handle the parent part if it exists
        if let Some(parent) = node.parent() {
            // We need to check the type of the parent node
            if let Some(parent_path) = parent.as_constant_path_node() {
                // Recursively collect namespaces for nested paths
                self.collect_constant_path_names_recursive(&parent_path, namespaces);
            } else if let Some(parent_const) = parent.as_constant_read_node() {
                // Add a simple parent namespace
                let name = String::from_utf8_lossy(parent_const.name().as_slice()).to_string();
                if let Ok(ns) = RubyNamespace::new(&name) {
                    namespaces.push(ns);
                }
            }
        }

        if let Some(name_bytes) = node.name() {
            let name = String::from_utf8_lossy(name_bytes.as_slice()).to_string();
            if let Ok(ns) = RubyNamespace::new(&name) {
                namespaces.push(ns);
            }
        }
    }
}

impl Visit<'_> for IdentifierVisitor {
    fn visit_class_node(&mut self, node: &ClassNode) {
        if self.is_position_in_location(&node.location()) {
            let name = String::from_utf8_lossy(&node.name().as_slice());
            self.namespace_stack
                .push(RubyNamespace::new(&name.to_string()).unwrap());
            visit_class_node(self, &node);
            self.namespace_stack.pop();
        }
    }

    fn visit_module_node(&mut self, node: &ModuleNode) {
        if self.is_position_in_location(&node.location()) {
            let name = String::from_utf8_lossy(&node.name().as_slice());
            self.namespace_stack
                .push(RubyNamespace::new(&name.to_string()).unwrap());
            visit_module_node(self, &node);
            self.namespace_stack.pop();
        }
    }

    fn visit_constant_path_node(&mut self, node: &ConstantPathNode) {
        if self.is_position_in_location(&node.location()) {
            let namespaces = self.collect_constant_path_names(node);
            self.identifier = Some(FullyQualifiedName::namespace(namespaces));
        }
    }

    fn visit_constant_read_node(&mut self, node: &ConstantReadNode) {
        if self.is_position_in_location(&node.location()) {
            let constant_name = String::from_utf8_lossy(node.name().as_slice()).to_string();

            self.identifier = Some(FullyQualifiedName::namespace(vec![RubyNamespace::new(
                constant_name.as_str(),
            )
            .unwrap()]));
        }
    }
}
