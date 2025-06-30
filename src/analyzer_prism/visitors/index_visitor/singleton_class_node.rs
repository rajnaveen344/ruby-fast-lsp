use ruby_prism::SingletonClassNode;

use super::IndexVisitor;

impl IndexVisitor {
    pub fn process_singleton_class_node_entry(&mut self, _node: &SingletonClassNode) {
        self.scope_tracker.enter_singleton();
    }

    pub fn process_singleton_class_node_exit(&mut self, _node: &SingletonClassNode) {
        self.scope_tracker.exit_singleton();
    }
}
