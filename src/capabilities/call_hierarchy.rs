//! Call Hierarchy Capability — Thin wrapper over `query::call_hierarchy`.
//!
//! Extracts LSP parameters and delegates to the query layer.

use crate::query::call_hierarchy::CallHierarchyData;
use crate::query::IndexQuery;
use crate::server::RubyLanguageServer;
use log::info;
use tower_lsp::lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyIncomingCallsParams, CallHierarchyItem,
    CallHierarchyOutgoingCall, CallHierarchyOutgoingCallsParams, CallHierarchyPrepareParams,
};

/// Handle `textDocument/prepareCallHierarchy` request
pub async fn handle_prepare_call_hierarchy(
    server: &RubyLanguageServer,
    params: CallHierarchyPrepareParams,
) -> Option<Vec<CallHierarchyItem>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    let doc = server.get_doc(&uri)?;
    let content = doc.content.clone();
    let query = IndexQuery::with_engine(server.index_for_uri(&uri), server.analysis_engine.clone());
    query.prepare_call_hierarchy(&uri, position, content)
}

/// Handle `callHierarchy/incomingCalls` request
pub async fn handle_incoming_calls(
    server: &RubyLanguageServer,
    params: CallHierarchyIncomingCallsParams,
) -> Option<Vec<CallHierarchyIncomingCall>> {
    let item_uri = params.item.uri.clone();
    let data: CallHierarchyData = params
        .item
        .data
        .as_ref()
        .and_then(|d| serde_json::from_value(d.clone()).ok())?;
    info!("Incoming calls request for: {}", data.fqn);
    let query = IndexQuery::with_engine(
        server.index_for_uri(&item_uri),
        server.analysis_engine.clone(),
    );
    query.get_incoming_calls(&data)
}

/// Handle `callHierarchy/outgoingCalls` request
pub async fn handle_outgoing_calls(
    server: &RubyLanguageServer,
    params: CallHierarchyOutgoingCallsParams,
) -> Option<Vec<CallHierarchyOutgoingCall>> {
    let item_uri = params.item.uri.clone();
    let data: CallHierarchyData = params
        .item
        .data
        .as_ref()
        .and_then(|d| serde_json::from_value(d.clone()).ok())?;
    info!("Outgoing calls request for: {}", data.fqn);
    let query = IndexQuery::with_engine(
        server.index_for_uri(&item_uri),
        server.analysis_engine.clone(),
    );
    query.get_outgoing_calls(&data)
}
