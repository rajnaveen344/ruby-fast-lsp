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

        // Determine method kind based on receiver and scope. Only support:
        //   * `def self.foo`            (receiver: self)
        //   * `def Foo.foo` inside `class Foo`  (constant read matching current class/module)
        // Otherwise skip indexing.
        let mut method_kind = MethodKind::Instance;
        let mut skip_method = false;

        if let Some(receiver) = node.receiver() {
            if receiver.as_self_node().is_some() {
                method_kind = MethodKind::Class;
            } else if let Some(read_node) = receiver.as_constant_read_node() {
                let recv_name = String::from_utf8_lossy(read_node.name().as_slice()).to_string();
                // Current namespace last element (if any) should match receiver constant
                let ns_stack = self.scope_tracker.get_ns_stack();
                let last_ns = ns_stack.last();
                if let Some(last) = last_ns {
                    if last.to_string() == recv_name {
                        method_kind = MethodKind::Class;
                    } else {
                        skip_method = true;
                    }
                } else {
                    // No enclosing namespace -> unsupported
                    skip_method = true;
                }
            } else {
                // ConstantPathNode or other receiver types not supported
                skip_method = true;
            }
        } else if self.scope_tracker.in_singleton() {
            method_kind = MethodKind::Class;
        }

        if skip_method {
            warn!("Skipping method with unsupported receiver");
            return;
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

        let namespace_parts = self.scope_tracker.get_ns_stack();

        let fqn = FullyQualifiedName::method(namespace_parts.clone(), method.clone());

        debug!("Visiting method definition: {}", fqn);

        let owner_fqn = FullyQualifiedName::Constant(namespace_parts.clone());

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
