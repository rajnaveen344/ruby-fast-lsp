//! Type hierarchy: prepare, supertypes, and subtypes over analysis-engine graph facts.
//!
//! Implements the LSP Type Hierarchy feature for Ruby classes and modules.
//! This provides functionality similar to Ruby's `Class.ancestors` and finding
//! subclasses/subtypes.
//!
//! The type hierarchy has three parts:
//! 1. `prepare` - Find the class/module at cursor position
//! 2. `supertypes` - Get ancestors (superclass chain + included modules)
//! 3. `subtypes` - Get descendants (subclasses + mixers)
//!
//! ## Method Resolution Order (MRO)
//!
//! Ruby's MRO for instance methods follows this order:
//! 1. Prepended modules (last prepend first)
//! 2. The class itself
//! 3. Included modules (last include first)
//! 4. Superclass chain (recursively applying the same rules)
//!
//! The supertypes list follows this exact order (excluding self).

use log::{debug, info};
use ruby_analysis::core::FullyQualifiedName;
use ruby_analysis::engine::{TypeHierarchyEntry, TypeHierarchyNode, TypeHierarchyRelation, View};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::{
    Position, SymbolKind, TypeHierarchyItem, TypeHierarchyPrepareParams,
    TypeHierarchySubtypesParams, TypeHierarchySupertypesParams, Url,
};

use ruby_analysis::indexer::Identifier;

use crate::features::cursor::analysis_location::location_for_range;
use crate::features::cursor::{Cursor, EngineQuery};
use crate::server::RubyLanguageServer;
use crate::utils::lsp::source_position;

/// Handle `textDocument/prepareTypeHierarchy`.
pub async fn handle_prepare(
    server: &RubyLanguageServer,
    params: TypeHierarchyPrepareParams,
) -> LspResult<Option<Vec<TypeHierarchyItem>>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    info!(
        "Prepare type hierarchy request received for {:?}",
        uri.path()
    );
    let start_time = Instant::now();
    let content = server
        .documents
        .read()
        .get(&uri)
        .map(|document| document.read().content.clone());
    let result = content.and_then(|content| {
        EngineQuery::with_engine(server.analysis_engine_for_uri(&uri))
            .with_view(|cursor| prepare_at(cursor, &uri, position, &content))
    });
    info!(
        "[PERF] Prepare type hierarchy completed in {:?}",
        start_time.elapsed()
    );
    Ok(result)
}

/// Handle `typeHierarchy/supertypes`.
pub async fn handle_supertypes(
    server: &RubyLanguageServer,
    params: TypeHierarchySupertypesParams,
) -> LspResult<Option<Vec<TypeHierarchyItem>>> {
    info!("Supertypes request received for: {}", params.item.name);
    let start_time = Instant::now();
    let result = read_item(server, &params.item, supertypes);
    info!(
        "[PERF] Supertypes completed in {:?}, returned {} items",
        start_time.elapsed(),
        result.as_ref().map_or(0, Vec::len)
    );
    Ok(result)
}

/// Handle `typeHierarchy/subtypes`.
pub async fn handle_subtypes(
    server: &RubyLanguageServer,
    params: TypeHierarchySubtypesParams,
) -> LspResult<Option<Vec<TypeHierarchyItem>>> {
    info!("Subtypes request received for: {}", params.item.name);
    let start_time = Instant::now();
    let result = read_item(server, &params.item, subtypes);
    info!(
        "[PERF] Subtypes completed in {:?}, returned {} items",
        start_time.elapsed(),
        result.as_ref().map_or(0, Vec::len)
    );
    Ok(result)
}

/// Read a follow-up request's stored item identity under one view of its
/// owning engine.
fn read_item(
    server: &RubyLanguageServer,
    item: &TypeHierarchyItem,
    read: impl FnOnce(&View<'_>, &TypeHierarchyData) -> Option<Vec<TypeHierarchyItem>>,
) -> Option<Vec<TypeHierarchyItem>> {
    let data: TypeHierarchyData = item
        .data
        .as_ref()
        .and_then(|d| serde_json::from_value(d.clone()).ok())?;
    EngineQuery::with_engine(server.analysis_engine_for_uri(&item.uri))
        .with_view(|cursor| read(cursor.view, &data))
}

// ============================================================================
// Data Structures
// ============================================================================

/// Data stored in TypeHierarchyItem.data to identify the item for follow-up requests
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypeHierarchyData {
    /// The fully qualified name of the class/module
    pub fqn: String,
}

// ============================================================================
// Queries over one cursor
// ============================================================================

/// The type hierarchy item for the class or module name at `position`.
pub fn prepare_at(
    cursor: Cursor<'_>,
    uri: &Url,
    position: Position,
    content: &str,
) -> Option<Vec<TypeHierarchyItem>> {
    info!(
        "Prepare type hierarchy request for {:?} at {:?}",
        uri.path(),
        position
    );
    let (identifier, _, ancestors, _scope_id, _namespace_kind) = cursor
        .analyzer(uri, content, position)
        .get_identifier_at_position(source_position(position));
    let Identifier::RubyConstant { iden, .. } = identifier? else {
        debug!("Type hierarchy only supports constants (classes/modules)");
        return None;
    };
    let node = cursor
        .view
        .type_hierarchy_node_for_constant(&iden, &ancestors)?;
    let item = type_hierarchy_item_from_engine_node(cursor.view, node)?;
    info!("Found type hierarchy item: {}", item.name);
    Some(vec![item])
}

/// The ancestor chain of a class or module in Ruby's method resolution order:
/// prepended modules (last first), included modules (last first), the
/// superclass chain, then extended modules, which affect class methods.
///
/// A class reopened in several files collects mixins from all of them; mixins
/// outside the primary definition carry a warning because the runtime order
/// depends on require order.
///
/// `Some(vec![])` means no supertypes, including a type that no longer parses.
pub fn supertypes(view: &View<'_>, data: &TypeHierarchyData) -> Option<Vec<TypeHierarchyItem>> {
    info!("Supertypes request for: {}", data.fqn);
    related_types(view, data, "supertypes", |view, fqn| view.supertypes(fqn))
}

/// Direct subclasses, then the classes and modules that include, prepend, or
/// extend this module.
///
/// `Some(vec![])` means no subtypes, including a type that no longer parses.
pub fn subtypes(view: &View<'_>, data: &TypeHierarchyData) -> Option<Vec<TypeHierarchyItem>> {
    info!("Subtypes request for: {}", data.fqn);
    related_types(view, data, "subtypes", |view, fqn| view.subtypes(fqn))
}

fn related_types(
    view: &View<'_>,
    data: &TypeHierarchyData,
    relation: &str,
    related: impl FnOnce(&View<'_>, &FullyQualifiedName) -> Vec<TypeHierarchyEntry>,
) -> Option<Vec<TypeHierarchyItem>> {
    // A stored type that no longer parses may have been deleted.
    let Some(fqn) = view.parse_namespace_fqn(&data.fqn) else {
        info!("Could not parse FQN: {}", data.fqn);
        return Some(vec![]);
    };
    let items = related(view, &fqn)
        .into_iter()
        .filter_map(|entry| type_hierarchy_item_from_engine_entry(view, entry))
        .collect::<Vec<_>>();
    info!("Found {} {relation} for {}", items.len(), data.fqn);
    Some(items)
}

// ============================================================================
// Helper Functions
// ============================================================================

fn type_hierarchy_item_from_engine_entry(
    view: &View<'_>,
    entry: TypeHierarchyEntry,
) -> Option<TypeHierarchyItem> {
    let kind = match entry.node_kind {
        Some(kind) => graph_node_kind_to_symbol_kind(kind),
        None => relation_symbol_kind(entry.relation),
    };
    let mut detail = if entry.unresolved {
        format!(
            "{} ({}) ❓ definition not found",
            entry.fqn,
            relation_label(entry.relation)
        )
    } else {
        format!("{} ({})", entry.fqn, relation_label(entry.relation))
    };
    if let Some(edge_file_id) = entry.edge_file_id {
        detail = format!("{} ⚠️ from {}", detail, file_name_for(view, edge_file_id));
    }
    type_hierarchy_item_from_parts(view, &entry.fqn, kind, entry.range, Some(detail))
}

fn type_hierarchy_item_from_engine_node(
    view: &View<'_>,
    node: TypeHierarchyNode,
) -> Option<TypeHierarchyItem> {
    type_hierarchy_item_from_parts(
        view,
        &node.fqn,
        graph_node_kind_to_symbol_kind(node.node_kind),
        node.range,
        None,
    )
}

fn type_hierarchy_item_from_parts(
    view: &View<'_>,
    fqn: &ruby_analysis::core::FullyQualifiedName,
    kind: SymbolKind,
    range: ruby_analysis::core::TextRange,
    detail: Option<String>,
) -> Option<TypeHierarchyItem> {
    let location = location_for_range(view, range)?;
    Some(TypeHierarchyItem {
        name: fqn.name(),
        kind,
        tags: None,
        detail: detail.or_else(|| Some(fqn.to_string())),
        uri: location.uri,
        range: location.range,
        selection_range: location.range,
        data: Some(
            serde_json::to_value(TypeHierarchyData {
                fqn: fqn.to_string(),
            })
            .ok()?,
        ),
    })
}

fn graph_node_kind_to_symbol_kind(kind: ruby_analysis::core::GraphNodeKind) -> SymbolKind {
    match kind {
        ruby_analysis::core::GraphNodeKind::Class => SymbolKind::CLASS,
        ruby_analysis::core::GraphNodeKind::Module => SymbolKind::MODULE,
    }
}

fn relation_label(relation: TypeHierarchyRelation) -> &'static str {
    match relation {
        TypeHierarchyRelation::Superclass => "superclass",
        TypeHierarchyRelation::Include => "include",
        TypeHierarchyRelation::Prepend => "prepend",
        TypeHierarchyRelation::Extend => "extend",
        TypeHierarchyRelation::Subclass => "subclass",
        TypeHierarchyRelation::IncludedBy => "included by",
        TypeHierarchyRelation::PrependedBy => "prepended by",
        TypeHierarchyRelation::ExtendedBy => "extended by",
    }
}

fn relation_symbol_kind(relation: TypeHierarchyRelation) -> SymbolKind {
    match relation {
        TypeHierarchyRelation::Superclass | TypeHierarchyRelation::Subclass => SymbolKind::CLASS,
        TypeHierarchyRelation::Include
        | TypeHierarchyRelation::IncludedBy
        | TypeHierarchyRelation::Prepend
        | TypeHierarchyRelation::PrependedBy
        | TypeHierarchyRelation::Extend
        | TypeHierarchyRelation::ExtendedBy => SymbolKind::MODULE,
    }
}

fn file_name_for(view: &View<'_>, file_id: ruby_analysis::core::SourceFileId) -> String {
    view.file(file_id)
        .and_then(|file| {
            file.path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "unknown".to_string())
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_hierarchy_data_serialization() {
        let data = TypeHierarchyData {
            fqn: "Foo::Bar".to_string(),
        };

        let json = serde_json::to_value(&data).unwrap();
        let parsed: TypeHierarchyData = serde_json::from_value(json).unwrap();

        assert_eq!(parsed.fqn, "Foo::Bar");
    }
}
