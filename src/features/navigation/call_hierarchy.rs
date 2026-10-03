//! Call hierarchy: prepare, incoming, and outgoing calls from analysis facts.
//!
//! Implements the LSP Call Hierarchy feature for Ruby methods:
//! 1. `prepare` — Find the method at cursor position
//! 2. `incoming_calls` — Who calls this method?
//! 3. `outgoing_calls` — What does this method call?
//!
//! Call site data is stored at index time: each ReferenceFact records the FQN
//! of the enclosing method (the caller). This makes both incoming and outgoing
//! calls simple grouping operations on existing analysis data.

use log::info;
use ruby_analysis::engine::{CallHierarchyMethod, View};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyIncomingCallsParams, CallHierarchyItem,
    CallHierarchyOutgoingCall, CallHierarchyOutgoingCallsParams, CallHierarchyPrepareParams,
    Position, SymbolKind, SymbolTag, Url,
};

use ruby_analysis::indexer::Identifier;

use crate::features::cursor::analysis_location::{location_for_range, lsp_ranges_for_ranges};
use crate::features::cursor::{method, Cursor, EngineQuery};
use crate::server::Server;
use crate::utils::lsp::source_position;

/// Handle `textDocument/prepareCallHierarchy`.
pub async fn handle_prepare(
    server: &Server,
    params: CallHierarchyPrepareParams,
) -> LspResult<Option<Vec<CallHierarchyItem>>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    info!(
        "Prepare call hierarchy request received for {:?}",
        uri.path()
    );
    let start_time = Instant::now();
    let content = server
        .documents
        .read()
        .get(&uri)
        .map(|document| document.read().content.clone());
    let result = content.and_then(|content| {
        EngineQuery::with_project(server.project_for_uri(&uri))
            .with_view(|cursor| prepare_at(cursor, &uri, position, &content))
    });
    info!(
        "[PERF] Prepare call hierarchy completed in {:?}",
        start_time.elapsed()
    );
    Ok(result)
}

/// Handle `callHierarchy/incomingCalls`.
pub async fn handle_incoming(
    server: &Server,
    params: CallHierarchyIncomingCallsParams,
) -> LspResult<Option<Vec<CallHierarchyIncomingCall>>> {
    info!("Incoming calls request received for: {}", params.item.name);
    let start_time = Instant::now();
    let result = read_item(server, &params.item, incoming_calls);
    info!(
        "[PERF] Incoming calls completed in {:?}, returned {} items",
        start_time.elapsed(),
        result.as_ref().map_or(0, Vec::len)
    );
    Ok(result)
}

/// Handle `callHierarchy/outgoingCalls`.
pub async fn handle_outgoing(
    server: &Server,
    params: CallHierarchyOutgoingCallsParams,
) -> LspResult<Option<Vec<CallHierarchyOutgoingCall>>> {
    info!("Outgoing calls request received for: {}", params.item.name);
    let start_time = Instant::now();
    let result = read_item(server, &params.item, outgoing_calls);
    info!(
        "[PERF] Outgoing calls completed in {:?}, returned {} items",
        start_time.elapsed(),
        result.as_ref().map_or(0, Vec::len)
    );
    Ok(result)
}

/// Read a follow-up request's stored item identity under one view of its
/// owning engine.
fn read_item<R>(
    server: &Server,
    item: &CallHierarchyItem,
    read: impl FnOnce(&View<'_>, &CallHierarchyData) -> Option<R>,
) -> Option<R> {
    let data: CallHierarchyData = item
        .data
        .as_ref()
        .and_then(|d| serde_json::from_value(d.clone()).ok())?;
    EngineQuery::with_project(server.project_for_uri(&item.uri))
        .with_view(|cursor| read(cursor.view, &data))
}

// ============================================================================
// Data Structures
// ============================================================================

/// Data stored in CallHierarchyItem.data to identify the item for follow-up requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallHierarchyData {
    /// The fully qualified name of the method (e.g., "Foo::Bar#baz")
    pub fqn: String,
}

// ============================================================================
// Queries over one cursor
// ============================================================================

/// The call hierarchy item for the method at `position`.
pub fn prepare_at(
    cursor: Cursor<'_>,
    uri: &Url,
    position: Position,
    content: &str,
) -> Option<Vec<CallHierarchyItem>> {
    info!(
        "Prepare call hierarchy request for {:?} at {:?}",
        uri.path(),
        position
    );
    let (identifier, _, ancestors, _scope_id, namespace_kind) = cursor
        .analyzer(uri, content, position)
        .get_identifier_at_position(source_position(position));
    let identifier = identifier?;
    let Identifier::RubyMethod { receiver, iden, .. } = &identifier else {
        info!(
            "Call hierarchy only supports methods, got: {:?}",
            identifier
        );
        return None;
    };
    let owner_fqn =
        method::receiver_namespace(cursor, receiver, &ancestors, namespace_kind, position)?;
    let method = cursor
        .view
        .call_hierarchy_method_for_owner(&owner_fqn, iden)?;
    Some(vec![call_hierarchy_item(cursor.view, method)?])
}

/// The methods that call the stored method.
pub fn incoming_calls(
    view: &View<'_>,
    data: &CallHierarchyData,
) -> Option<Vec<CallHierarchyIncomingCall>> {
    let method_fqn = view.parse_method_fqn(&data.fqn)?;
    Some(
        view.incoming_calls(&method_fqn)
            .into_iter()
            .filter_map(|call| {
                Some(CallHierarchyIncomingCall {
                    from: call_hierarchy_item(view, call.from)?,
                    from_ranges: lsp_ranges_for_ranges(view, call.from_ranges),
                })
            })
            .collect(),
    )
}

/// The methods the stored method calls.
pub fn outgoing_calls(
    view: &View<'_>,
    data: &CallHierarchyData,
) -> Option<Vec<CallHierarchyOutgoingCall>> {
    let method_fqn = view.parse_method_fqn(&data.fqn)?;
    Some(
        view.outgoing_calls(&method_fqn)
            .into_iter()
            .filter_map(|call| {
                Some(CallHierarchyOutgoingCall {
                    to: call_hierarchy_item(view, call.to)?,
                    from_ranges: lsp_ranges_for_ranges(view, call.from_ranges),
                })
            })
            .collect(),
    )
}

// ============================================================================
// Helpers
// ============================================================================

fn call_hierarchy_item(view: &View<'_>, method: CallHierarchyMethod) -> Option<CallHierarchyItem> {
    let location = location_for_range(view, method.range)?;
    Some(CallHierarchyItem {
        name: method.fqn.name(),
        kind: SymbolKind::METHOD,
        tags: Some(Vec::<SymbolTag>::new()),
        detail: Some(method.fqn.to_string()),
        uri: location.uri,
        range: location.range,
        selection_range: location.range,
        data: Some(
            serde_json::to_value(CallHierarchyData {
                fqn: method.fqn.to_string(),
            })
            .ok()?,
        ),
    })
}
