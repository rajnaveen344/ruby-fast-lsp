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
        Identifier::RubyLocalVariable { name, scope, .. } => {
            variable::find_local_variable_definitions_at_position(name, scope, &index, position)
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
    };

    if result.is_none() {
        info!("No definition found for {:?}", identifier);
    }

    result
}
