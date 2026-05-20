use ruby_analysis_core::ReferenceCandidate;
use ruby_analysis_indexer::collect_namespaces;
use ruby_prism::ConstantPathNode;

use super::FactCollector;

impl FactCollector {
    pub fn process_constant_path_node_entry(&mut self, node: &ConstantPathNode) {
        let mut namespaces = Vec::new();
        collect_namespaces(node, &mut namespaces);

        if namespaces.is_empty() {
            return;
        }

        let name = namespaces
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join("::");
        let range =
            self.text_range_from_prism_location(&node.location(), "constant path reference");
        self.reference_candidates.push(ReferenceCandidate::constant(
            range,
            namespaces,
            self.scope_tracker.get_ns_stack(),
            name,
        ));
    }

    pub fn process_constant_path_node_exit(&mut self, _node: &ConstantPathNode) {}
}
