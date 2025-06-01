use std::sync::{Arc, Mutex};

use lsp_types::Url;
use ruby_prism::{
    visit_class_node, visit_module_node, ClassNode, ConstantPathNode, ModuleNode, Visit,
};

use crate::{
    analyzer_prism::utils::collect_namespaces,
    indexer::index::RubyIndex,
    server::RubyLanguageServer,
    types::{
        fully_qualified_name::FullyQualifiedName, ruby_constant::RubyConstant,
        ruby_document::RubyDocument, ruby_method::RubyMethod, ruby_namespace::RubyNamespace,
    },
};

pub struct ReferenceVisitor {
    pub index: Arc<Mutex<RubyIndex>>,
    pub uri: Url,
    pub document: RubyDocument,
    pub namespace_stack: Vec<RubyNamespace>,
    pub current_method: Option<RubyMethod>,
}

impl ReferenceVisitor {
    pub fn new(server: &RubyLanguageServer, uri: Url) -> Self {
        let document = server.get_doc(&uri).unwrap();
        Self {
            index: server.index(),
            uri,
            document,
            namespace_stack: vec![],
            current_method: None,
        }
    }
}

impl Visit<'_> for ReferenceVisitor {
    fn visit_module_node(&mut self, node: &ModuleNode) {
        let name = String::from_utf8_lossy(node.name().as_slice());
        self.namespace_stack
            .push(RubyNamespace::new(&name).unwrap());
        visit_module_node(self, node);
        self.namespace_stack.pop();
    }

    fn visit_class_node(&mut self, node: &ClassNode) {
        let name = String::from_utf8_lossy(node.name().as_slice());
        self.namespace_stack
            .push(RubyNamespace::new(&name).unwrap());
        visit_class_node(self, node);
        self.namespace_stack.pop();
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
        let mut ancestors = self.namespace_stack.clone();
        let mut namespaces = Vec::new();
        collect_namespaces(node, &mut namespaces);

        while !ancestors.is_empty() {
            let mut combined_ns = ancestors.clone();
            combined_ns.extend(namespaces.iter().cloned());

            let fqn = if let Some(last_part) = combined_ns.last() {
                match RubyConstant::try_from(last_part.to_string()) {
                    Ok(ruby_constant) => {
                        combined_ns.pop(); // Remove the last namespace as it is the constant
                        FullyQualifiedName::constant(combined_ns, ruby_constant)
                    }
                    Err(_) => FullyQualifiedName::namespace(combined_ns.clone()),
                }
            } else {
                FullyQualifiedName::namespace(combined_ns.clone())
            };

            let key = fqn.clone();
            let mut index = self.index.lock().unwrap();
            let entries = index.definitions.get(&key);
            if let Some(_) = entries {
                let location = self
                    .document
                    .prism_location_to_lsp_location(&node.location());
                index.add_reference(fqn.clone(), location);

                return;
            }

            ancestors.pop();
        }
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
        let document = RubyDocument::new(uri.clone(), content.to_string(), 0);
        server
            .docs
            .lock()
            .unwrap()
            .insert(uri.clone(), document.clone());
        let _ = process_file_for_definitions(server, uri.clone());
        let _ = process_file_for_references(server, uri.clone());
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

        assert_eq!(references.unwrap().len(), 1);
    }
}
