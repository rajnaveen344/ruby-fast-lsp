pub mod position;

use lsp_types::Position;
use ruby_prism::{
    visit_class_node, visit_module_node, BlockNode, ClassNode, ConstantPathNode, ConstantReadNode,
    DefNode, LambdaNode, ModuleNode, ProgramNode, SingletonClassNode, Visit,
};

use crate::indexer::types::{
    fully_qualified_name::FullyQualifiedName, ruby_namespace::RubyNamespace,
};
pub struct RubyPrismAnalyzer {
    content: String,
    namespace_stack: Vec<RubyNamespace>,
}

impl RubyPrismAnalyzer {
    pub fn new(content: String) -> Self {
        Self {
            content,
            namespace_stack: vec![],
        }
    }

    pub fn get_namespace_stack(&self) -> Vec<RubyNamespace> {
        self.namespace_stack.clone()
    }

    /// Based on a constant node target, a constant path node parent and a position, this method will find the exact
    /// portion of the constant path that matches the requested position, for higher precision in hover and
    /// definition. For example:
    ///
    /// ```ruby
    /// Foo::Bar::Baz
    ///           ^ Going to definition here should go to Foo::Bar::Baz
    ///      ^ Going to definition here should go to Foo::Bar
    ///  ^ Going to definition here should go to Foo
    /// ```
    pub fn get_identifier(&self, position: Position) -> FullyQualifiedName {
        todo!()
    }
}

/// We visit all nodes where we want to find an identifier at a specific position.
/// If the position is with the location range of a node, we walk it, else skip the node.
/// Only the following identifiers are supported for now:
/// - ConstantPathNode
/// - ConstantReadNode
/// All other node implementations are provided only for namespace tracking and traversal optimization.
impl Visit<'_> for RubyPrismAnalyzer {
    fn visit_class_node(&mut self, node: &ClassNode) {
        let namespace = RubyNamespace::new(&String::from_utf8_lossy(node.name().as_slice()));
        if let Ok(namespace) = namespace {
            self.namespace_stack.push(namespace);
        }
        visit_class_node(self, node);
        self.namespace_stack.pop();
    }

    fn visit_module_node(&mut self, node: &ModuleNode) {
        let namespace = RubyNamespace::new(&String::from_utf8_lossy(node.name().as_slice()));
        if let Ok(namespace) = namespace {
            self.namespace_stack.push(namespace);
        }
        visit_module_node(self, node);
        self.namespace_stack.pop();
    }

    fn visit_singleton_class_node(&mut self, node: &SingletonClassNode) {
        todo!()
    }

    fn visit_def_node(&mut self, node: &DefNode) {
        todo!()
    }

    fn visit_block_node(&mut self, node: &BlockNode) {
        todo!()
    }

    fn visit_lambda_node(&mut self, node: &LambdaNode) {
        todo!()
    }

    fn visit_program_node(&mut self, node: &ProgramNode) {
        todo!()
    }

    fn visit_constant_path_node(&mut self, node: &ConstantPathNode) {
        todo!()
    }

    fn visit_constant_read_node(&mut self, node: &ConstantReadNode) {
        todo!()
    }
}
