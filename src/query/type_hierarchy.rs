//! Type Hierarchy Query — LSP adapter over analysis-engine graph facts.
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
use ruby_analysis::engine::{AnalysisQuery, TypeHierarchyEntry, TypeHierarchyRelation};
use serde::{Deserialize, Serialize};
use tower_lsp::lsp_types::{Position, SymbolKind, TypeHierarchyItem, Url};

use ruby_analysis::indexer::{Identifier, RubyPrismAnalyzer};
use crate::types::fully_qualified_name::FullyQualifiedName;

use super::analysis_location::location_for_range;
use super::EngineQuery;

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
// EngineQuery entry points
// ============================================================================

impl EngineQuery {
    /// Find the class/module at the cursor position and return a TypeHierarchyItem.
    ///
    /// Uses RubyPrismAnalyzer to find the identifier at position, then resolves
    /// it to a fully qualified name via the analysis engine.
    pub fn prepare_type_hierarchy(
        &self,
        uri: &Url,
        position: Position,
        content: String,
    ) -> Option<Vec<TypeHierarchyItem>> {
        info!(
            "Prepare type hierarchy request for {:?} at {:?}",
            uri.path(),
            position
        );

        // Use analyzer to find identifier at position before locking analysis state.
        let analyzer = RubyPrismAnalyzer::new(uri.clone(), content);
        let (identifier, _, ancestors, _scope_id, _namespace_kind) =
            analyzer.get_identifier(position);

        let identifier = identifier?;

        // Only handle constants (class/module names)
        let constant_parts = match &identifier {
            Identifier::RubyConstant { namespace: _, iden } => iden.clone(),
            _ => {
                debug!("Type hierarchy only supports constants (classes/modules)");
                return None;
            }
        };

        let engine_ref = self.analysis_engine().expect(
            "INVARIANT VIOLATED: type hierarchy prepare requires an analysis engine. \
             This is a bug because LSP typeHierarchy should be a thin wrapper over AnalysisEngine. \
             Fix: construct EngineQuery with with_engine().",
        );
        let engine = engine_ref.lock();
        let query = AnalysisQuery::new(&engine);
        let fqn = query.resolve_constant_in_context(&constant_parts, &ancestors)?;

        if let Some(item) = query.type_hierarchy_node(&fqn).and_then(|(kind, range)| {
            type_hierarchy_item_from_parts(
                &engine,
                &fqn,
                graph_node_kind_to_symbol_kind(kind),
                range,
                None,
            )
        }) {
            info!("Found type hierarchy item: {}", fqn);
            return Some(vec![item]);
        }
        None
    }

    /// Get the ancestor chain for a class/module in Ruby's Method Resolution Order.
    ///
    /// Returns supertypes in MRO order:
    /// 1. Prepended modules (last prepend first)
    /// 2. Included modules (last include first)
    /// 3. Superclass chain (recursively applying the same rules)
    /// 4. Extended modules (shown separately as they affect class methods)
    ///
    /// When a class is reopened in multiple files, mixins from all files are collected.
    /// Mixins from files other than the primary definition show a warning indicator
    /// since the actual MRO depends on runtime require order which we can't determine statically.
    ///
    /// Returns `Some(vec![])` when there are no supertypes (valid but empty result).
    /// Returns `None` only when the request data is malformed.
    pub fn get_supertypes(&self, data: &TypeHierarchyData) -> Option<Vec<TypeHierarchyItem>> {
        info!("Supertypes request for: {}", data.fqn);

        // Parse the FQN string - return empty if can't parse (type might have been deleted)
        let engine_ref = self.analysis_engine().expect(
            "INVARIANT VIOLATED: supertype query requires an analysis engine. \
             This is a bug because LSP typeHierarchy should be a thin wrapper over AnalysisEngine. \
             Fix: construct EngineQuery with with_engine().",
        );
        let engine = engine_ref.lock();
        let query = AnalysisQuery::new(&engine);
        let fqn = match query.parse_namespace_fqn(&data.fqn) {
            Some(f) => f,
            None => {
                info!("Could not parse FQN: {}", data.fqn);
                return Some(vec![]);
            }
        };

        let supertypes = query
            .supertypes(&fqn)
            .into_iter()
            .filter_map(|entry| type_hierarchy_item_from_engine_entry(&engine, entry))
            .collect::<Vec<_>>();

        info!("Found {} supertypes for {}", supertypes.len(), data.fqn);
        Some(supertypes)
    }

    /// Get types that inherit from or mix in this class/module.
    ///
    /// Returns:
    /// - For classes: direct subclasses
    /// - For modules: classes/modules that include/prepend/extend this module
    ///
    /// Subtypes are grouped by relationship type:
    /// 1. Direct subclasses first
    /// 2. Classes/modules that include this module
    /// 3. Classes/modules that prepend this module
    ///
    /// Returns `Some(vec![])` when there are no subtypes (valid but empty result).
    /// Returns `None` only when the request data is malformed.
    pub fn get_subtypes(&self, data: &TypeHierarchyData) -> Option<Vec<TypeHierarchyItem>> {
        info!("Subtypes request for: {}", data.fqn);

        // Parse the FQN string - return empty if can't parse (type might have been deleted)
        let engine_ref = self.analysis_engine().expect(
            "INVARIANT VIOLATED: subtype query requires an analysis engine. \
             This is a bug because LSP typeHierarchy should be a thin wrapper over AnalysisEngine. \
             Fix: construct EngineQuery with with_engine().",
        );
        let engine = engine_ref.lock();
        let query = AnalysisQuery::new(&engine);
        let fqn = match query.parse_namespace_fqn(&data.fqn) {
            Some(f) => f,
            None => {
                info!("Could not parse FQN: {}", data.fqn);
                return Some(vec![]);
            }
        };

        let subtypes = query
            .subtypes(&fqn)
            .into_iter()
            .filter_map(|entry| type_hierarchy_item_from_engine_entry(&engine, entry))
            .collect::<Vec<_>>();

        info!("Found {} subtypes for {}", subtypes.len(), data.fqn);
        Some(subtypes)
    }
}

// ============================================================================
// Helper Functions
// ============================================================================

fn type_hierarchy_item_from_engine_entry(
    engine: &ruby_analysis::engine::AnalysisEngine,
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
        detail = format!("{} ⚠️ from {}", detail, file_name_for(engine, edge_file_id));
    }
    type_hierarchy_item_from_parts(engine, &entry.fqn, kind, entry.range, Some(detail))
}

fn type_hierarchy_item_from_parts(
    engine: &ruby_analysis::engine::AnalysisEngine,
    fqn: &FullyQualifiedName,
    kind: SymbolKind,
    range: ruby_analysis::core::TextRange,
    detail: Option<String>,
) -> Option<TypeHierarchyItem> {
    let location = location_for_range(engine, range)?;
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

fn file_name_for(
    engine: &ruby_analysis::engine::AnalysisEngine,
    file_id: ruby_analysis::core::SourceFileId,
) -> String {
    engine
        .file(file_id)
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
