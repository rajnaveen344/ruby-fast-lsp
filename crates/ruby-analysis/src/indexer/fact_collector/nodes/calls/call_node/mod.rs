//! Call node entry/exit, direct macro dispatch, and call-family submodules.

mod delegation;
mod diagnostics;
mod dynamic_definitions;
mod macros;
mod names;
mod receivers;
mod reference;
mod reflection;

use crate::core::GraphEdgeKind;
use crate::core::MethodVisibility;
use ruby_prism::CallNode;

use crate::indexer::fact_collector::FactCollector;

impl FactCollector {
    pub(in crate::indexer::fact_collector) fn process_call_node_entry(&mut self, node: &CallNode) {
        let extension_host = self.extensions.host.clone();
        let extension_handled = extension_host.process_call_node(self, node);
        let track_call = extension_host.should_track_enclosing_call(self, node);
        let resolved_call = track_call.then(|| extension_host.resolved_call_for_stack(self, node));
        self.extensions.enter_call(extension_handled, resolved_call);

        let direct_call_handled = self.process_direct_call_facts(node);
        if !direct_call_handled
            && !extension_handled
            && !node
                .receiver()
                .is_some_and(|receiver| receiver.as_call_node().is_some())
        {
            self.process_call_reference_candidate(node);
        }
    }

    /// Collect a call-on-call reference after its receiver subtree has been
    /// visited. Inner candidates then precede outer candidates during engine
    /// resolution, and an inner call that already ended at terminal Unknown
    /// does not force a retained candidate for every remaining chain segment.
    pub(in crate::indexer::fact_collector) fn process_nested_receiver_call_reference_candidate(
        &mut self,
        node: &CallNode,
    ) {
        if node
            .receiver()
            .is_some_and(|receiver| receiver.as_call_node().is_some())
            && !self.extensions.current_call_handled()
        {
            self.process_call_reference_candidate(node);
        }
    }

    fn process_direct_call_facts(&mut self, node: &CallNode) -> bool {
        // A receiverless declaration call in an ordinary block acts on the
        // block's definition owner, as a `def` there does, whether or not the
        // block's `self` is proven; the seed walk applies the same rule.
        self.push_direct_included_hook_mixin_edges(node);

        if node.receiver().is_some() && node.name().as_slice() == b"class_attribute" {
            self.push_direct_class_attribute_method_facts(node);
        }

        if node.receiver().is_some() {
            self.push_direct_send_dynamic_method_fact(node);
            self.push_direct_receiver_define_singleton_method_fact(node);
            return false;
        }

        match node.name().as_slice() {
            b"attr_reader" => {
                self.push_direct_attr_method_facts(node, true, false);
                true
            }
            b"attr_writer" => {
                self.push_direct_attr_method_facts(node, false, true);
                true
            }
            b"attr_accessor" => {
                self.push_direct_attr_method_facts(node, true, true);
                true
            }
            b"class_attribute" => {
                self.push_direct_class_attribute_method_facts(node);
                true
            }
            b"private" => {
                self.push_direct_visibility_modifier(node, MethodVisibility::Private);
                true
            }
            b"protected" => {
                self.push_direct_visibility_modifier(node, MethodVisibility::Protected);
                true
            }
            b"public" => {
                self.push_direct_visibility_modifier(node, MethodVisibility::Public);
                true
            }
            b"module_function" => {
                self.push_direct_module_function_facts(node);
                true
            }
            b"alias_method" => {
                self.push_direct_alias_method_fact(node);
                true
            }
            b"define_method" => {
                self.push_direct_define_method_fact(node);
                true
            }
            b"define_singleton_method" => {
                self.push_direct_define_singleton_method_fact(node);
                true
            }
            b"delegate" => {
                self.push_direct_delegate_method_facts(node);
                true
            }
            b"def_delegator" | b"def_delegators" => {
                self.push_direct_forwardable_delegate_method_facts(node);
                true
            }
            b"include" => {
                self.push_direct_mixin_edges(node, GraphEdgeKind::Include);
                true
            }
            b"prepend" => {
                self.push_direct_mixin_edges(node, GraphEdgeKind::Prepend);
                true
            }
            b"extend" => {
                self.push_direct_mixin_edges(node, GraphEdgeKind::Extend);
                true
            }
            _ => false,
        }
    }

    pub(in crate::indexer::fact_collector) fn process_call_node_exit(&mut self, _node: &CallNode) {
        self.extensions.exit_call();
    }
}
