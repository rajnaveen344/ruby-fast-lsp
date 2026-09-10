use ruby_prism::SingletonClassNode;

use crate::indexer::fact_collector::FactCollector;

impl FactCollector {
    pub(in crate::indexer::fact_collector) fn process_singleton_class_node_entry(
        &mut self,
        _node: &SingletonClassNode,
    ) {
        self.scope_tracker.enter_singleton();
    }

    pub(in crate::indexer::fact_collector) fn process_singleton_class_node_exit(
        &mut self,
        _node: &SingletonClassNode,
    ) {
        self.scope_tracker.exit_singleton();
    }
}
