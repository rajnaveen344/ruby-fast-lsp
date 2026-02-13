//! Type Hierarchy Capability — Thin wrapper over `query::type_hierarchy`.
//!
//! Extracts LSP parameters and delegates to the query layer.

use crate::query::IndexQuery;
use crate::server::RubyLanguageServer;
use log::info;
use tower_lsp::lsp_types::{
    TypeHierarchyItem, TypeHierarchyPrepareParams, TypeHierarchySubtypesParams,
    TypeHierarchySupertypesParams,
};

pub use crate::query::type_hierarchy::{RelationType, TypeHierarchyData};

/// Handle `textDocument/prepareTypeHierarchy` request
pub async fn handle_prepare_type_hierarchy(
    server: &RubyLanguageServer,
    params: TypeHierarchyPrepareParams,
) -> Option<Vec<TypeHierarchyItem>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    let doc = server.get_doc(&uri)?;
    let content = doc.content.clone();
    let query = IndexQuery::new(server.index.clone());
    query.prepare_type_hierarchy(&uri, position, content)
}

/// Handle `typeHierarchy/supertypes` request
pub async fn handle_supertypes(
    server: &RubyLanguageServer,
    params: TypeHierarchySupertypesParams,
) -> Option<Vec<TypeHierarchyItem>> {
    let data: TypeHierarchyData = params
        .item
        .data
        .as_ref()
        .and_then(|d| serde_json::from_value(d.clone()).ok())?;
    info!("Supertypes request for: {}", data.fqn);
    let query = IndexQuery::new(server.index.clone());
    query.get_supertypes(&data)
}

/// Handle `typeHierarchy/subtypes` request
pub async fn handle_subtypes(
    server: &RubyLanguageServer,
    params: TypeHierarchySubtypesParams,
) -> Option<Vec<TypeHierarchyItem>> {
    let data: TypeHierarchyData = params
        .item
        .data
        .as_ref()
        .and_then(|d| serde_json::from_value(d.clone()).ok())?;
    info!("Subtypes request for: {}", data.fqn);
    let query = IndexQuery::new(server.index.clone());
    query.get_subtypes(&data)
}
