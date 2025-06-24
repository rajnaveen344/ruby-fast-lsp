use ruby_prism::SingletonClassNode;

use super::IndexVisitor;

impl IndexVisitor {
    pub fn process_singleton_class_node_entry(&mut self, _node: &SingletonClassNode) {
        self.in_singleton_node = true;
    }

    pub fn process_singleton_class_node_exit(&mut self, _node: &SingletonClassNode) {
        self.in_singleton_node = false;
    }
}
