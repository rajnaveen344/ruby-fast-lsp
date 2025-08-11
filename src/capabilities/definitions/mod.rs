pub mod constant;
pub mod method;
pub mod variable;

use log::info;
use tower_lsp::lsp_types::{Location, Position, Url};

use crate::analyzer_prism::{Identifier, RubyPrismAnalyzer};
use crate::server::RubyLanguageServer;
use crate::types::fully_qualified_name::FullyQualifiedName;

pub async fn find_definition_at_position(
    server: &RubyLanguageServer,
    uri: Url,
    position: Position,
    content: &str,
) -> Option<Vec<Location>> {
    let analyzer = RubyPrismAnalyzer::new(uri, content.to_string());
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

    // First check if this is a Ruby core class in stub files
    if let Identifier::RubyConstant { namespace: _, iden } = &identifier {
        let stubs = server.stubs.lock();
        
        // Check if the stub system is initialized
        if !stubs.is_initialized() {
            info!("Stub system not initialized, skipping core class lookup");
        } else {
            // Try each constant in the identifier chain as a potential core class
            for constant in iden.iter() {
                let class_name = constant.to_string();
                info!("Checking if '{}' is a Ruby core class", class_name);
                
                if stubs.is_core_class(&class_name) {
                    if let Some(location) = stubs.get_class_definition(&class_name) {
                        info!("Found core class definition for {} in stub files", class_name);
                        return Some(vec![location]);
                    }
                }
            }
            
            info!("No core class found for identifier: {:?}", iden);
        }
    }

    let index = server.index.lock();

    let result = match &identifier {
        Identifier::RubyConstant { namespace: _, iden } => {
            constant::find_constant_definitions(iden, &index, &ancestors)
        }
        Identifier::RubyMethod {
            namespace,
            receiver_kind,
            receiver,
            iden,
        } => method::find_method_definitions(
            namespace,
            receiver_kind,
            receiver,
            iden,
            &index,
            &ancestors,
        ),
        Identifier::RubyVariable { iden } => {
            variable::find_variable_definitions_at_position(iden, &index, &ancestors, position)
        }
    };

    if result.is_none() {
        info!("No definition found for {:?}", identifier);
    }

    result
}
