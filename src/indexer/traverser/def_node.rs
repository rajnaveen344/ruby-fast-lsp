use std::collections::HashMap;

use log::debug;
use ruby_prism::DefNode;

use crate::indexer::{
    entry::{entry_kind::EntryKind, Entry, MethodKind, MethodOrigin, MethodVisibility},
    types::{fully_qualified_name::FullyQualifiedName, ruby_method::RubyMethod},
};

use super::Visitor;

impl Visitor {
    pub fn process_def_node_entry(&mut self, node: &DefNode) {
        let method_name_id = node.name();
        let method_name_bytes = method_name_id.as_slice();
        let method_name_str = String::from_utf8_lossy(method_name_bytes);

        if let Ok(method_name) = RubyMethod::try_from(method_name_str.as_ref()) {
            let name_location = node.name_loc();
            let location = self.prism_loc_to_lsp_loc(name_location);
            let current_namespace_path = self.namespace_stack.clone();
            let fqn =
                FullyQualifiedName::instance_method(current_namespace_path.clone(), method_name);

            debug!("Visiting method definition: {}", fqn);

            let owner_fqn = if current_namespace_path.is_empty() {
                FullyQualifiedName::Namespace(vec![])
            } else {
                FullyQualifiedName::Namespace(current_namespace_path)
            };

            let entry = Entry {
                fqn: fqn.clone(),
                location,
                kind: EntryKind::Method {
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

            self._current_method = Some(fqn.to_string());
        } else {
            debug!("Skipping invalid method name: {}", method_name_str);
        }
    }

    pub fn process_def_node_exit(&mut self, _node: &DefNode) {
        self._current_method = None;
    }
}
