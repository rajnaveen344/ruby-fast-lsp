//! Go to implementation: where methods and modules are concretely implemented.
//!
//! Answers "textDocument/implementation":
//! - For a method: find all overrides in descendant classes and including classes
//! - For a module/class: find all classes that include/prepend/extend it

use log::{info, trace};
use ruby_analysis::core::FullyQualifiedName;
use ruby_analysis::indexer::Identifier;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{GotoDefinitionParams, GotoDefinitionResponse, Location, Position, Url};

use crate::features::cursor::analysis_location::{locations_for_ranges, non_empty_locations};
use crate::features::cursor::{method, Cursor, EngineQuery};
use crate::features::navigation::definition::constant_fqn;
use crate::server::Server;
use crate::utils::lsp::source_position;

/// Handle `textDocument/implementation`.
pub async fn handle(
    server: &Server,
    params: GotoDefinitionParams,
) -> LspResult<Option<GotoDefinitionResponse>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    server.await_document_semantic_commit(&uri).await;

    let doc_arc = server.open_document(&uri);
    let implementations = doc_arc.and_then(|doc_arc| {
        EngineQuery::with_doc_and_project(doc_arc, server.project_for_uri(&uri)).with_view(
            |cursor| {
                let content = &cursor.document?.content;
                implementations_at(cursor, &uri, position, content)
            },
        )
    });

    match implementations {
        Some(locations) => {
            trace!("Returning {} implementation locations", locations.len());
            Ok(Some(GotoDefinitionResponse::Array(locations)))
        }
        None => {
            info!("No implementations found for position {:?}", position);
            Ok(None)
        }
    }
}

/// Implementations of the identifier at `position`.
///
/// - A method: its overrides in descendants and includers.
/// - A class or module name: the classes that include, prepend, or extend it,
///   plus its subclasses.
pub fn implementations_at(
    cursor: Cursor<'_>,
    uri: &Url,
    position: Position,
    content: &str,
) -> Option<Vec<Location>> {
    let (identifier, _, ancestors, _scope_stack, namespace_kind) = cursor
        .analyzer(uri, content, position)
        .get_identifier_at_position(source_position(position));
    let Some(identifier) = identifier else {
        info!("No identifier found at position {:?}", position);
        return None;
    };
    info!(
        "Looking for implementations of: {}->{}",
        FullyQualifiedName::from(ancestors.clone()),
        identifier,
    );

    let view = cursor.view;
    let ranges = match &identifier {
        Identifier::RubyMethod { receiver, iden, .. } => {
            let owner =
                method::receiver_namespace(cursor, receiver, &ancestors, namespace_kind, position)?;
            view.method_implementation_ranges(&owner, iden)
        }
        Identifier::RubyConstant { iden, .. } => {
            view.namespace_implementation_ranges(&constant_fqn(view, iden, &ancestors))
        }
        _ => {
            info!(
                "Implementation not supported for identifier type: {:?}",
                identifier
            );
            return None;
        }
    };
    non_empty_locations(locations_for_ranges(view, ranges))
}
