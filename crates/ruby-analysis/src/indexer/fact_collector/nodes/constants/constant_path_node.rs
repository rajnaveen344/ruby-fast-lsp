use crate::core::ReferenceCandidate;
use crate::indexer::collect_namespaces;
use ruby_prism::ConstantPathNode;

use crate::indexer::fact_collector::FactCollector;

impl FactCollector {
    pub(in crate::indexer::fact_collector) fn process_constant_path_node_entry(
        &mut self,
        node: &ConstantPathNode,
    ) {
        let mut namespaces = Vec::new();
        collect_namespaces(node, &mut namespaces);

        if namespaces.is_empty() {
            return;
        }

        let range =
            self.text_range_from_prism_location(&node.location(), "constant path reference");
        self.facts.references.push(ReferenceCandidate::constant(
            range,
            namespaces,
            self.scope_tracker.get_ns_stack(),
        ));
    }

    pub(in crate::indexer::fact_collector) fn process_constant_path_node_exit(
        &mut self,
        _node: &ConstantPathNode,
    ) {
    }
}
