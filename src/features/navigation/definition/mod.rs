//! Go to definition: require-string paths, then one engine view's resolution
//! of constants, methods, variables, and YARD type references. A request that
//! arrives while its target is still indexing waits on an exact navigation
//! demand (`demand`).

mod demand;
mod query;

pub(crate) use query::constant_fqn;
pub use query::definitions_at;

use std::path::PathBuf;

use log::info;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{
    GotoDefinitionParams, GotoDefinitionResponse, Location, LocationLink, Position, Range, Url,
};

use crate::features::cursor::EngineQuery;
use crate::loader::require_paths::{
    find_require_string_at_offset, location_for_require_target, resolve_require_path,
    RequireStringTarget,
};
use crate::server::RubyLanguageServer;
use crate::utils::lsp::{lsp_position, source_position};
use ruby_analysis::indexer::RubyDocument;

/// Handle `textDocument/definition`.
pub async fn handle(
    server: &RubyLanguageServer,
    params: GotoDefinitionParams,
) -> LspResult<Option<GotoDefinitionResponse>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let project = server.analysis_workspace_for_uri(&uri);
    let mut definition = find_definition_at_position(server, uri.clone(), position).await;
    if definition.is_none() {
        let demand_keys = demand::navigation_demand_keys_at_position(server, &uri, position);
        if let (Some(project), Some(demand_keys)) = (&project, &demand_keys) {
            definition =
                demand::wait_for_demanded_definition(server, project, demand_keys, &uri, position)
                    .await?;
        }
    }

    match definition {
        Some(response) => {
            if let Some(project) = &project {
                for target_uri in definition_target_uris(&response) {
                    server.retain_external_document_project(&target_uri, project);
                }
            }
            Ok(Some(response))
        }
        None => {
            info!("No definition found for position {:?}", position);
            Ok(None)
        }
    }
}

/// Find the definition at `position`: a require string's target file, then
/// the identifier's definitions under one engine view.
pub async fn find_definition_at_position(
    server: &RubyLanguageServer,
    uri: Url,
    position: Position,
) -> Option<GotoDefinitionResponse> {
    let doc_arc = server.documents.read().get(&uri)?.clone();
    let require = {
        let document = doc_arc.read();
        let byte_offset = document.position_to_analysis_offset(source_position(position));
        find_require_string_at_offset(&document.content, byte_offset as usize)
            .map(|target| (require_string_lsp_range(&document, &target), target))
    };
    if let Some((origin, target)) = require {
        if let Some(response) = require_path_definitions(server, &uri, origin, &target) {
            return Some(response);
        }
    }

    let locations = EngineQuery::with_doc_and_engine(doc_arc, server.analysis_engine_for_uri(&uri))
        .with_view(|cursor| {
            let content = &cursor.document?.content;
            definitions_at(cursor, &uri, position, content)
        })?;
    Some(GotoDefinitionResponse::Array(locations))
}

/// LSP range covering the require string contents (excluding surrounding quotes).
pub(crate) fn require_string_lsp_range(
    document: &RubyDocument,
    target: &RequireStringTarget,
) -> Range {
    let (start_byte, end_byte) = target.content_byte_range(&document.content);
    Range::new(
        lsp_position(document.offset_to_position(start_byte)),
        lsp_position(document.offset_to_position(end_byte)),
    )
}

fn require_path_definitions(
    server: &RubyLanguageServer,
    uri: &Url,
    origin: Range,
    target: &RequireStringTarget,
) -> Option<GotoDefinitionResponse> {
    let current_file = uri.to_file_path().ok()?;
    let project_root = server
        .workspace_for_uri(uri)
        .map(|workspace| workspace.root_path)
        .or_else(|| current_file.parent().map(PathBuf::from))?;
    let load_paths = server
        .config
        .lock()
        .indexing
        .load_paths
        .paths_for_project(&project_root)
        .to_vec();
    let feature_index = server.require_feature_index_for_uri(uri);
    let engine = server.analysis_engine_for_uri(uri);
    let engine_guard = engine.read();
    let resolved = resolve_require_path(
        target.kind,
        &target.argument,
        &current_file,
        &project_root,
        &load_paths,
        &feature_index,
        Some(&engine_guard),
    )?;
    let location = location_for_require_target(&resolved, Some(&engine_guard))?;
    Some(GotoDefinitionResponse::Link(vec![LocationLink {
        origin_selection_range: Some(origin),
        target_uri: location.uri,
        target_range: location.range,
        target_selection_range: location.range,
    }]))
}

/// Collect target URIs from a definition response for external-project retention.
fn definition_target_uris(response: &GotoDefinitionResponse) -> Vec<Url> {
    match response {
        GotoDefinitionResponse::Scalar(location) => vec![location.uri.clone()],
        GotoDefinitionResponse::Array(locations) => locations
            .iter()
            .map(|location| location.uri.clone())
            .collect(),
        GotoDefinitionResponse::Link(links) => {
            links.iter().map(|link| link.target_uri.clone()).collect()
        }
    }
}

/// Flatten definition targets to locations (callers that ignore origin range).
pub fn definition_locations(response: GotoDefinitionResponse) -> Vec<Location> {
    match response {
        GotoDefinitionResponse::Scalar(location) => vec![location],
        GotoDefinitionResponse::Array(locations) => locations,
        GotoDefinitionResponse::Link(links) => links
            .into_iter()
            .map(|link| Location {
                uri: link.target_uri,
                range: link.target_range,
            })
            .collect(),
    }
}
