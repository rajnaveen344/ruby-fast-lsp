use std::sync::{Arc, Mutex};

use log::{debug, warn};
use lsp_types::Url;
use ruby_prism::{
    visit_block_node, visit_class_node, visit_constant_path_node, visit_constant_read_node,
    visit_def_node, visit_local_variable_read_node, visit_module_node, BlockNode, ClassNode,
    ConstantPathNode, DefNode, LocalVariableReadNode, ModuleNode, Visit,
};

use crate::{
    analyzer_prism::{
        scope_tracker::ScopeTracker,
        utils::{self, collect_namespaces},
    },
    indexer::{entry::MethodKind, index::RubyIndex},
    server::RubyLanguageServer,
    types::{
        fully_qualified_name::FullyQualifiedName,
        ruby_document::RubyDocument,
        ruby_method::RubyMethod,
        ruby_namespace::RubyConstant,
        ruby_variable::{RubyVariable, RubyVariableType},
        scope::{LVScope, LVScopeKind},
    },
};

pub struct ReferenceVisitor {
    pub index: Arc<Mutex<RubyIndex>>,
    pub document: RubyDocument,
    pub scope_tracker: ScopeTracker,
    pub include_local_vars: bool,
}

impl ReferenceVisitor {
    pub fn new(server: &RubyLanguageServer, uri: Url) -> Self {
        Self::with_options(server, uri, true)
    }

    pub fn with_options(server: &RubyLanguageServer, uri: Url, include_local_vars: bool) -> Self {
        let index = server.index();
        let document = server.get_doc(&uri).unwrap();
        let scope_tracker = ScopeTracker::new(&document);
        Self {
            index,
            document,
            scope_tracker,
            include_local_vars,
        }
    }
}

impl Visit<'_> for ReferenceVisitor {
    fn visit_module_node(&mut self, node: &ModuleNode) {
        let const_path = node.constant_path();

        let body_loc = if let Some(body) = node.body() {
            self.document
                .prism_location_to_lsp_location(&body.location())
        } else {
            self.document
                .prism_location_to_lsp_location(&node.location())
        };

        if let Some(path_node) = const_path.as_constant_path_node() {
            let mut namespace_parts = Vec::new();
            utils::collect_namespaces(&path_node, &mut namespace_parts);
            self.scope_tracker.push_ns_scopes(namespace_parts);
            let scope_id = self.document.position_to_offset(body_loc.range.start);
            self.scope_tracker.push_lv_scope(LVScope::new(
                scope_id,
                body_loc,
                LVScopeKind::Constant,
            ));
            visit_module_node(self, node);
            self.scope_tracker.pop_ns_scope();
            self.scope_tracker.pop_lv_scope();
        } else {
            let name = String::from_utf8_lossy(node.name().as_slice());
            self.scope_tracker
                .push_ns_scope(RubyConstant::new(&name).unwrap());
            let scope_id = self.document.position_to_offset(body_loc.range.start);
            self.scope_tracker.push_lv_scope(LVScope::new(
                scope_id,
                body_loc,
                LVScopeKind::Constant,
            ));
            visit_module_node(self, node);
            self.scope_tracker.pop_ns_scope();
            self.scope_tracker.pop_lv_scope();
        }
    }

    fn visit_class_node(&mut self, node: &ClassNode) {
        let const_path = node.constant_path();

        let body_loc = if let Some(body) = node.body() {
            self.document
                .prism_location_to_lsp_location(&body.location())
        } else {
            self.document
                .prism_location_to_lsp_location(&node.location())
        };

        if let Some(path_node) = const_path.as_constant_path_node() {
            let mut namespace_parts = Vec::new();
            utils::collect_namespaces(&path_node, &mut namespace_parts);
            self.scope_tracker.push_ns_scopes(namespace_parts);
            let scope_id = self.document.position_to_offset(body_loc.range.start);
            self.scope_tracker.push_lv_scope(LVScope::new(
                scope_id,
                body_loc,
                LVScopeKind::Constant,
            ));
            visit_class_node(self, node);
            self.scope_tracker.pop_ns_scope();
            self.scope_tracker.pop_lv_scope();
        } else {
            let name = String::from_utf8_lossy(node.name().as_slice());
            self.scope_tracker
                .push_ns_scope(RubyConstant::new(&name).unwrap());
            let scope_id = self.document.position_to_offset(body_loc.range.start);
            self.scope_tracker.push_lv_scope(LVScope::new(
                scope_id,
                body_loc,
                LVScopeKind::Constant,
            ));
            visit_class_node(self, node);
            self.scope_tracker.pop_ns_scope();
            self.scope_tracker.pop_lv_scope();
        }
    }

    fn visit_def_node(&mut self, node: &DefNode) {
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

        visit_def_node(self, node);
        self.scope_tracker.pop_lv_scope();
    }

    fn visit_block_node(&mut self, node: &BlockNode) {
        let body_loc = if let Some(body) = node.body() {
            self.document
                .prism_location_to_lsp_location(&body.location())
        } else {
            self.document
                .prism_location_to_lsp_location(&node.location())
        };
        let scope_id = self.document.position_to_offset(body_loc.range.start);
        self.scope_tracker
            .push_lv_scope(LVScope::new(scope_id, body_loc, LVScopeKind::Block));
        visit_block_node(self, node);
        self.scope_tracker.pop_lv_scope();
    }

    fn visit_constant_path_node(&mut self, node: &ConstantPathNode) {
        // Collect namespaces from constant path node
        // Find in namespace stack
        // If found, add reference for each namespace
        // Eg. Example: Given this source
        //
        // ```ruby
        // module Core
        //   module Platform
        //     module API
        //       module Users; end
        //     end
        //
        //     module Something
        //       include API::Users
        //     end
        //   end
        // end
        // ```
        // `include API::Users` under `Core::Platform::Something` should add references to:
        // Core::Platform::API
        // Core::Platform::API::Users
        //
        // If not found, ignore
        let current_namespace = self.scope_tracker.get_ns_stack();
        let mut namespaces = Vec::new();
        collect_namespaces(node, &mut namespaces);

        // Check from current namespace to root namespace
        let mut ancestors = current_namespace;
        while !ancestors.is_empty() {
            let mut combined_ns = ancestors.clone();
            combined_ns.extend(namespaces.clone());

            let fqn = FullyQualifiedName::namespace(combined_ns);
            let mut index = self.index.lock().unwrap();
            let entries = index.definitions.get(&fqn);
            if let Some(_) = entries {
                let location = self
                    .document
                    .prism_location_to_lsp_location(&node.location());
                index.add_reference(fqn.clone(), location);
            }

            ancestors.pop();
        }

        // Check from root namespace
        let fqn = FullyQualifiedName::namespace(namespaces);
        let mut index = self.index.lock().unwrap();
        let entries = index.definitions.get(&fqn);
        if let Some(_) = entries {
            let location = self
                .document
                .prism_location_to_lsp_location(&node.location());
            index.add_reference(fqn.clone(), location);
        }

        drop(index);

        visit_constant_path_node(self, node);
    }

    fn visit_constant_read_node(&mut self, node: &ruby_prism::ConstantReadNode<'_>) {
        let current_namespace = self.scope_tracker.get_ns_stack();
        let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let constant = RubyConstant::new(&name).unwrap();

        // Check from current namespace to root namespace
        let mut ancestors = current_namespace;
        while !ancestors.is_empty() {
            let mut combined_ns = ancestors.clone();
            combined_ns.push(constant.clone());

            let fqn = FullyQualifiedName::namespace(combined_ns);
            let mut index = self.index.lock().unwrap();
            if index.definitions.contains_key(&fqn) {
                let location = self
                    .document
                    .prism_location_to_lsp_location(&node.location());
                debug!("Adding reference: {}", fqn);
                index.add_reference(fqn, location);
                drop(index);
                break;
            }
            drop(index);
            ancestors.pop();
        }

        // Check in root namespace
        let fqn = FullyQualifiedName::namespace(vec![constant]);
        let mut index = self.index.lock().unwrap();
        if index.definitions.contains_key(&fqn) {
            let location = self
                .document
                .prism_location_to_lsp_location(&node.location());
            debug!("Adding reference: {}", fqn);
            index.add_reference(fqn, location);
        }
        drop(index);

        visit_constant_read_node(self, node);
    }

    fn visit_local_variable_read_node(&mut self, node: &LocalVariableReadNode) {
        if !self.include_local_vars {
            visit_local_variable_read_node(self, node);
            return;
        }

        let variable_name = String::from_utf8_lossy(node.name().as_slice()).to_string();
        let location = self
            .document
            .prism_location_to_lsp_location(&node.location());
        let mut index = self.index.lock().unwrap();

        // Search through scope stack from innermost to outermost scope
        let lv_stack = self.scope_tracker.get_lv_stack();
        for i in (0..lv_stack.len()).rev() {
            // Take all scopes up to the current level
            let scopes = lv_stack[0..=i].to_vec();
            let var_type = RubyVariableType::Local(scopes);

            if let Ok(variable) = RubyVariable::new(&variable_name, var_type) {
                let fqn = FullyQualifiedName::variable(variable);

                debug!("Searching for variable: {:?}", fqn);

                // Check if this variable is defined in the current scope level
                if index.definitions.contains_key(&fqn) {
                    debug!(
                        "Adding local variable reference: {:?} at {:?}",
                        fqn, location
                    );
                    index.add_reference(fqn, location);
                    drop(index);
                    visit_local_variable_read_node(self, node);
                    return;
                }
            }
        }

        // If we get here, no matching definition was found in any scope
        debug!(
            "No definition found for local variable '{}' at {:?}",
            variable_name, location
        );
        drop(index);

        // Continue visiting children nodes
        visit_local_variable_read_node(self, node);
    }
}

#[cfg(test)]
mod tests {
    use crate::capabilities::references;
    use crate::handlers::helpers::{process_file_for_definitions, process_file_for_references};
    use crate::server::RubyLanguageServer;
    use crate::types::ruby_document::RubyDocument;
    use lsp_types::*;

    fn create_server() -> RubyLanguageServer {
        RubyLanguageServer::default()
    }

    fn open_file(server: &RubyLanguageServer, content: &str, uri: &Url) -> RubyDocument {
        open_file_with_options(server, content, uri, true)
    }

    fn open_file_with_options(
        server: &RubyLanguageServer,
        content: &str,
        uri: &Url,
        include_local_vars: bool,
    ) -> RubyDocument {
        let document = RubyDocument::new(uri.clone(), content.to_string(), 0);
        server
            .docs
            .lock()
            .unwrap()
            .insert(uri.clone(), document.clone());
        let _ = process_file_for_definitions(server, uri.clone());
        let _ = process_file_for_references(server, uri.clone(), include_local_vars);
        document
    }

    #[tokio::test]
    async fn test_reference_visitor() {
        let code = r#"
module Core
    module Platform
        module API
            module Users; end
        end

        module Something
            include API::Users
        end
    end
end
        "#;
        let server = create_server();
        let uri = Url::parse("file:///dummy.rb").unwrap();
        open_file(&server, code, &uri);

        let references =
            references::find_references_at_position(&server, &uri, Position::new(4, 19)).await;

        assert_eq!(references.unwrap().len(), 2);

        // ConstantReadNode
        let references =
            references::find_references_at_position(&server, &uri, Position::new(3, 15)).await;

        assert_eq!(references.unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_local_variable_references() {
        let code = r#"
def my_method
  local_var = 42
  puts local_var  # Reference to local_var
  
  local_var.times do |i|
    puts "Count: #{i}"
  end
  
  local_var  # Another reference
end

my_method
        "#;

        let server = create_server();
        let uri = Url::parse("file:///local_vars.rb").unwrap();

        // First test with local vars enabled
        open_file_with_options(&server, code, &uri, true);
        let index = server.index();
        let index_guard = index.lock().unwrap();

        // Should find local variable references
        let local_var_refs: Vec<_> = index_guard
            .references
            .iter()
            .filter(|(fqn, _)| fqn.to_string().contains("local_var"))
            .collect();

        assert!(
            !local_var_refs.is_empty(),
            "Should find local variable references when include_local_vars is true"
        );

        // Now test with local vars disabled
        let server = create_server();
        open_file_with_options(&server, code, &uri, false);
        let index = server.index();
        let index_guard = index.lock().unwrap();

        // Should not find any local variable references
        let local_var_refs: Vec<_> = index_guard
            .references
            .iter()
            .filter(|(fqn, _)| fqn.to_string().contains("local_var"))
            .collect();

        assert!(
            local_var_refs.is_empty(),
            "Should not find local variable references when include_local_vars is false"
        );
    }
}
