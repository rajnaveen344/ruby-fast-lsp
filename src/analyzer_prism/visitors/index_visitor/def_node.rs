use std::collections::HashMap;

use log::{debug, warn};
use ruby_prism::DefNode;

use crate::indexer::entry::{
    entry_kind::EntryKind, Entry, MethodKind, MethodOrigin, MethodVisibility,
};
use crate::types::scope_kind::LVScopeKind;
use crate::types::{fully_qualified_name::FullyQualifiedName, ruby_method::RubyMethod};

use super::IndexVisitor;

impl IndexVisitor {
    pub fn process_def_node_entry(&mut self, node: &DefNode) {
        let method_name_id = node.name();
        let method_name_bytes = method_name_id.as_slice();
        let method_name_str = String::from_utf8_lossy(method_name_bytes);

        let method_name = RubyMethod::try_from(method_name_str.as_ref());

        if let Err(_) = method_name {
            warn!("Skipping invalid method name: {}", method_name_str);
            return;
        }

        let method_name = method_name.unwrap();
        let name_location = node.name_loc();
        let location = self.prism_loc_to_lsp_loc(name_location);
        let current_namespace = self.current_namespace();
        let fqn =
            FullyQualifiedName::instance_method(current_namespace.clone(), method_name.clone());

        debug!("Visiting method definition: {}", fqn);

        let owner_fqn = if current_namespace.is_empty() {
            FullyQualifiedName::Constant(vec![])
        } else {
            FullyQualifiedName::Constant(current_namespace)
        };

        let entry = Entry {
            fqn: fqn.clone(),
            location,
            kind: EntryKind::Method {
                name: method_name.clone().into(),
                kind: MethodKind::Instance,
                parameters: vec![],
                owner: owner_fqn,
                visibility: MethodVisibility::Public,
                origin: MethodOrigin::Direct,
                origin_visibility: None,
            },
            metadata: HashMap::new(),
        };

        let mut index = self.index.lock().unwrap();
        index.add_entry(entry);
        debug!("Added method entry: {}", fqn);

        self.current_method = Some(method_name.clone());

        drop(index);

        self.push_lv_scope(LVScopeKind::Method);
    }

    pub fn process_def_node_exit(&mut self, _node: &DefNode) {
        self.current_method = None;
        self.pop_lv_scope();
    }
}
