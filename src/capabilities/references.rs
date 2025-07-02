use log::{debug, info};
use lsp_types::{Location, Position, Url};

use crate::analyzer_prism::Identifier;
use crate::analyzer_prism::RubyPrismAnalyzer;
use crate::server::RubyLanguageServer;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_variable::RubyVariableType;

/// Find all references to a symbol at the given position.
pub async fn find_references_at_position(
    server: &RubyLanguageServer,
    uri: &Url,
    position: Position,
) -> Option<Vec<Location>> {
    // Get the document content from the server
    let doc = match server.get_doc(uri) {
        Some(doc) => doc,
        None => {
            info!("Document not found for URI: {}", uri);
            return None;
        }
    };
    let content = doc.content.clone();

    // Use the analyzer to find the identifier at the position and get its fully qualified name
    let analyzer = RubyPrismAnalyzer::new(uri.clone(), content.to_string());
    let (identifier_opt, ancestors, _scope_stack) = analyzer.get_identifier(position);

    if let None = identifier_opt {
        info!("No identifier found at position {:?}", position);
        return None;
    }

    let identifier = identifier_opt.unwrap();

    // Create FQN from identifier, incorporating ancestors if needed
    let fqn: FullyQualifiedName;

    match &identifier {
        Identifier::RubyConstant(ns) => {
            // For namespaces, combine ancestors with the namespace parts
            let mut combined_ns = ancestors.clone();
            combined_ns.extend(ns.clone());
            fqn = FullyQualifiedName::namespace(combined_ns);
        }
        Identifier::RubyMethod(ns, method) => {
            // For methods, combine ancestors with the namespace parts
            let mut combined_ns = ancestors.clone();
            combined_ns.extend(ns.clone());
            fqn = FullyQualifiedName::method(combined_ns, method.clone());
        }
        Identifier::RubyVariable(variable) => {
            // For variables, use ancestors as the namespace
            fqn = FullyQualifiedName::variable(variable.clone());
        }
    }

    debug!("Looking for references to: {:?}", fqn);

    let index = server.index.lock().unwrap();

    if let Some(entries) = index.references.get(&fqn) {
        if !entries.is_empty() {
            let filtered_entries: Vec<Location> = match &identifier {
                Identifier::RubyVariable(variable) => match variable.variable_type() {
                    RubyVariableType::Local(_) => entries
                        .iter()
                        .filter(|loc| loc.uri == *uri && loc.range.start >= position)
                        .cloned()
                        .collect(),
                    _ => entries.to_owned(),
                },
                _ => entries.to_owned(),
            };

            if !filtered_entries.is_empty() {
                info!("Found {} references to: {}", filtered_entries.len(), fqn);
                return Some(filtered_entries);
            }
        }
    }

    info!("No references found for {}", fqn);
    None
}
