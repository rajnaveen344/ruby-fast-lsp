pub mod constant;
pub mod method;
pub mod variable;

use log::info;
use lsp_types::{Location, Position, Url};

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
