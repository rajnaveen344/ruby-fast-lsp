pub mod constant;
pub mod method;
pub mod variable;
pub mod yard_type;

use log::info;
use tower_lsp::lsp_types::{Location, Position, Url};

use crate::analyzer_prism::{Identifier, RubyPrismAnalyzer};
use crate::server::RubyLanguageServer;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::yard::YardParser;

pub async fn find_definition_at_position(
    server: &RubyLanguageServer,
    uri: Url,
    position: Position,
) -> Option<Vec<Location>> {
    // Get the document content from the server (like references does)
    let doc = match server.get_doc(&uri) {
        Some(doc) => doc,
        None => {
            info!("Document not found for URI: {}", uri);
            return None;
        }
    };
    let content = doc.content.clone();

    // First, check if we're in a YARD comment type reference
    if let Some(yard_type) = YardParser::find_type_at_position(&content, position) {
        info!("Found YARD type at position: {}", yard_type.type_name);
        let index = server.index.lock();
        return yard_type::find_yard_type_definitions(&yard_type.type_name, &index);
    }

    let analyzer = RubyPrismAnalyzer::new(uri.clone(), content.to_string());
    let (identifier, ancestors, _scope_stack) = analyzer.get_identifier(position);

    let identifier = match identifier {
        Some(id) => id,
        None => {
            info!("No identifier found at position {:?}", position);
            return None;
        }
    };

    info!(
        "Looking for definition of: {}->{}",
        FullyQualifiedName::from(ancestors.clone()),
        identifier,
    );

    let index = server.index.lock();

    let result = match &identifier {
        Identifier::RubyConstant { namespace: _, iden } => {
            constant::find_constant_definitions(iden, &index, &ancestors)
        }
        Identifier::RubyMethod {
            namespace,
            receiver,
            iden,
        } => {
            drop(index); // Release lock before calling type-aware function
            method::find_method_definitions(
                namespace,
                receiver,
                iden,
                &server.index.lock(),
                &ancestors,
                &server.type_narrowing,
                &uri,
                position,
                &content,
            )
        }
        Identifier::RubyLocalVariable { name, scope, .. } => {
            drop(index); // Release lock before accessing document

            // LocalVariables are stored in document.lvars, not global index
            if let Some(doc_arc) = server.docs.lock().get(&uri).cloned() {
                let doc_read = doc_arc.read();
                variable::find_local_variable_definitions_at_position(
                    name, *scope, &doc_read, position,
                )
            } else {
                None
            }
        }
        Identifier::RubyInstanceVariable { name, .. } => {
            variable::find_instance_variable_definitions(name, &index, &ancestors)
        }
        Identifier::RubyClassVariable { name, .. } => {
            variable::find_class_variable_definitions(name, &index, &ancestors)
        }
        Identifier::RubyGlobalVariable { name, .. } => {
            variable::find_global_variable_definitions(name, &index, &ancestors)
        }
        Identifier::YardType { type_name, .. } => {
            yard_type::find_yard_type_definitions(type_name, &index)
        }
    };

    if result.is_none() {
        info!("No definition found for {:?}", identifier);
    }

    result
}
