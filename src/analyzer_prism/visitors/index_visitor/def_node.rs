use log::{debug, warn};
use ruby_prism::DefNode;

use crate::indexer::entry::{
    entry_kind::EntryKind, Entry, MethodKind, MethodOrigin, MethodVisibility,
};
use crate::types::scope::{LVScope, LVScopeKind};
use crate::types::{fully_qualified_name::FullyQualifiedName, ruby_method::RubyMethod};

use super::IndexVisitor;

impl IndexVisitor {
    pub fn process_def_node_entry(&mut self, node: &DefNode) {
        let method_name_id = node.name();
        let method_name_bytes = method_name_id.as_slice();
        let method_name_str = String::from_utf8_lossy(method_name_bytes);

        let mut method_kind = MethodKind::Instance;

        if let Some(receiver) = node.receiver() {
            if receiver.as_self_node().is_some()
                || receiver.as_constant_path_node().is_some()
                || receiver.as_constant_read_node().is_some()
            {
                method_kind = MethodKind::Class;
            }
        } else if self.scope_tracker.in_singleton() {
            method_kind = MethodKind::Class;
        }

        let method = RubyMethod::new(method_name_str.as_ref(), method_kind);

        if let Err(_) = method {
            warn!("Skipping invalid method name: {}", method_name_str);
            return;
        }

        let mut method = method.unwrap();

        if method.get_name() == "initialize" {
            method = RubyMethod::new("new", MethodKind::Class).unwrap();
        }

        let name_location = node.name_loc();
        let location = self.document.prism_location_to_lsp_location(&name_location);
        let current_namespace = self.scope_tracker.get_ns_stack();
        let fqn = FullyQualifiedName::instance_method(current_namespace.clone(), method.clone());

        debug!("Visiting method definition: {}", fqn);

        let owner_fqn = FullyQualifiedName::Constant(current_namespace);

        let entry = Entry {
            fqn: fqn.clone(),
            location,
            kind: EntryKind::Method {
                name: method.clone().into(),
                parameters: vec![],
                owner: owner_fqn,
                visibility: MethodVisibility::Public,
                origin: MethodOrigin::Direct,
                origin_visibility: None,
            },
        };

        let mut index = self.index.lock().unwrap();
        index.add_entry(entry);
        debug!("Added method entry: {}", fqn);

        drop(index);

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
    }

    pub fn process_def_node_exit(&mut self, _node: &DefNode) {
        self.scope_tracker.pop_lv_scope();
    }
}
