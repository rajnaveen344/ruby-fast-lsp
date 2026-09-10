use ruby_prism::SingletonClassNode;

use super::FactCollector;

impl FactCollector {
    pub(super) fn process_singleton_class_node_entry(&mut self, _node: &SingletonClassNode) {
        self.scope_tracker.enter_singleton();
    }

    pub(super) fn process_singleton_class_node_exit(&mut self, _node: &SingletonClassNode) {
        self.scope_tracker.exit_singleton();
    }
}
