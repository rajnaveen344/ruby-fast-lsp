use log::warn;
use ruby_prism::DefNode;

use crate::indexer::entry::MethodKind;
use crate::types::{ruby_method::RubyMethod, scope::{LVScope, LVScopeKind}};

use super::ReferenceVisitor;

impl ReferenceVisitor {
    pub fn process_def_node_entry(&mut self, node: &DefNode) {
        let body_loc = if let Some(body) = node.body() {
            self.document
                .prism_location_to_lsp_location(&body.location())
        } else {
            self.document
                .prism_location_to_lsp_location(&node.location())
        };
        let scope_id = self.document.position_to_offset(body_loc.range.start);
        self.scope_tracker
            .push_lv_scope(LVScope::new(scope_id, body_loc, LVScopeKind::Method));

        let mut method_kind = MethodKind::Instance;

        if let Some(receiver) = node.receiver() {
            if let Some(_) = receiver.as_self_node() {
                method_kind = MethodKind::Class;
            } else if let Some(_) = receiver.as_constant_path_node() {
                method_kind = MethodKind::Class;
            } else if let Some(_) = receiver.as_constant_read_node() {
                method_kind = MethodKind::Class;
            }
        }

        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let method = RubyMethod::new(name.as_str(), method_kind);

        if let Err(_) = method {
            warn!("Skipping invalid method name: {}", name);
            return;
        }
    }

    pub fn process_def_node_exit(&mut self, _node: &DefNode) {
        self.scope_tracker.pop_lv_scope();
    }
}