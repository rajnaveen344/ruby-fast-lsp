//! Definitions Capability - Go to definition support
//!
//! Handles definition requests by dispatching to:
//! - Require/require_relative string path resolution (LocationLink with full-string origin)
//! - `EngineQuery` for constants, methods (via method resolution), and globals
//! - Document analysis for local variables
//! - YARD parser for type comments

use std::path::PathBuf;

use tower_lsp::lsp_types::{
    GotoDefinitionResponse, Location, LocationLink, Position, Range, Url,
};

use crate::indexer::require_paths::{
    find_require_string_at_offset, location_for_require_target, resolve_require_path,
    RequireStringTarget,
};
use crate::query::EngineQuery;
use crate::server::RubyLanguageServer;
use ruby_analysis::indexer::RubyDocument;

pub(crate) fn navigation_demand_keys_at_position(
    server: &RubyLanguageServer,
    uri: &Url,
    position: Position,
) -> Option<crate::query::definition::DefinitionNavigationDemandKeys> {
    let content = {
        let documents = server.docs.lock();
        let content = documents.get(uri)?.read().content.clone();
        content
    };
    crate::query::definition::definition_navigation_demand_keys(uri, position, &content)
}

/// Find definition at position using the unified EngineQuery layer.
pub async fn find_definition_at_position(
    server: &RubyLanguageServer,
    uri: Url,
    position: Position,
) -> Option<GotoDefinitionResponse> {
    let (content, doc_arc, document, byte_offset) = {
        let doc_guard = server.docs.lock();
        let doc_arc = doc_guard.get(&uri)?.clone();
        let doc = doc_arc.read();
        let byte_offset = doc.position_to_analysis_offset(position);
        (
            doc.content.clone(),
            doc_arc.clone(),
            doc.clone(),
            byte_offset,
        )
    };

    if let Some(response) =
        require_path_definitions(server, &uri, &content, &document, byte_offset as usize)
    {
        return Some(response);
    }

    let query = EngineQuery::with_doc_and_engine(doc_arc, server.analysis_engine_for_uri(&uri));
    let locations = query.find_definitions_at_position(&uri, position, &content)?;
    Some(GotoDefinitionResponse::Array(locations))
}

/// LSP range covering the require string contents (excluding surrounding quotes).
pub(crate) fn require_string_lsp_range(
    document: &RubyDocument,
    target: &RequireStringTarget,
) -> Range {
    let (start_byte, end_byte) = target.content_byte_range(&document.content);
    Range::new(
        document.offset_to_position(start_byte),
        document.offset_to_position(end_byte),
    )
}

fn require_path_definitions(
    server: &RubyLanguageServer,
    uri: &Url,
    content: &str,
    document: &RubyDocument,
    byte_offset: usize,
) -> Option<GotoDefinitionResponse> {
    let target = find_require_string_at_offset(content, byte_offset)?;
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
    let dependency_roots = server.dependency_require_paths_for_uri(uri);
    let engine = server.analysis_engine_for_uri(uri);
    let engine_guard = engine.read();
    let resolved = resolve_require_path(
        target.kind,
        &target.argument,
        &current_file,
        &project_root,
        &load_paths,
        &dependency_roots,
        Some(&engine_guard),
    )?;
    let location = location_for_require_target(&resolved, Some(&engine_guard))?;
    let origin = require_string_lsp_range(document, &target);
    Some(GotoDefinitionResponse::Link(vec![LocationLink {
        origin_selection_range: Some(origin),
        target_uri: location.uri,
        target_range: location.range,
        target_selection_range: location.range,
    }]))
}

/// Collect target URIs from a definition response for external-project retention.
pub(crate) fn definition_target_uris(response: &GotoDefinitionResponse) -> Vec<Url> {
    match response {
        GotoDefinitionResponse::Scalar(location) => vec![location.uri.clone()],
        GotoDefinitionResponse::Array(locations) => {
            locations.iter().map(|location| location.uri.clone()).collect()
        }
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
