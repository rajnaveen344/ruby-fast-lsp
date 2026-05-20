//! Type Hierarchy Query — Resolves type hierarchy items from the index.
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
use ruby_analysis_core::{GraphEdgeFact, GraphEdgeKind, GraphNodeKind};
use serde::{Deserialize, Serialize};
use tower_lsp::lsp_types::{Position, SymbolKind, TypeHierarchyItem, Url};

use crate::analyzer_prism::{Identifier, RubyPrismAnalyzer};
use crate::types::fully_qualified_name::{FullyQualifiedName, NamespaceKind};
use crate::types::ruby_namespace::RubyConstant;

use super::analysis_location::location_for_range;
use super::EngineQuery;

// ============================================================================
// Data Structures
// ============================================================================

/// Relationship type for a type hierarchy entry
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationType {
    /// Superclass inheritance (class Foo < Bar)
    Superclass,
    /// Module inclusion (include Foo)
    Include,
    /// Module prepending (prepend Foo)
    Prepend,
    /// Module extension (extend Foo)
    Extend,
    /// Direct subclass
    Subclass,
    /// Class/module that includes this module
    IncludedBy,
    /// Class/module that prepends this module
    PrependedBy,
    /// Class/module that extends this module
    ExtendedBy,
}

impl RelationType {
    /// Get a human-readable label for the relationship
    fn label(&self) -> &'static str {
        match self {
            RelationType::Superclass => "superclass",
            RelationType::Include => "include",
            RelationType::Prepend => "prepend",
            RelationType::Extend => "extend",
            RelationType::Subclass => "subclass",
            RelationType::IncludedBy => "included by",
            RelationType::PrependedBy => "prepended by",
            RelationType::ExtendedBy => "extended by",
        }
    }

    /// Get the SymbolKind for this relationship type
    ///
    /// - CLASS: superclass/subclass (class inheritance)
    /// - MODULE: include/prepend/extend and their reverses (module mixins)
    fn symbol_kind(&self) -> SymbolKind {
        match self {
            RelationType::Superclass | RelationType::Subclass => SymbolKind::CLASS,
            // All mixin types use MODULE since only modules can be mixed in
            RelationType::Include
            | RelationType::IncludedBy
            | RelationType::Prepend
            | RelationType::PrependedBy
            | RelationType::Extend
            | RelationType::ExtendedBy => SymbolKind::MODULE,
        }
    }
}

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
    /// it to a fully qualified name via the index.
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

        // Use analyzer to find identifier at position (before locking the index)
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

        // Resolve the FQN for this constant
        let fqn = resolve_constant_fqn_from_analysis(self, &constant_parts, &ancestors)?;

        if let Some(item) = self.type_hierarchy_item_from_analysis(&fqn) {
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
        let fqn = match parse_fqn_string(&data.fqn) {
            Some(f) => f,
            None => {
                info!("Could not parse FQN: {}", data.fqn);
                return Some(vec![]);
            }
        };

        let supertypes = self.supertypes_from_analysis(&fqn);

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
        let fqn = match parse_fqn_string(&data.fqn) {
            Some(f) => f,
            None => {
                info!("Could not parse FQN: {}", data.fqn);
                return Some(vec![]);
            }
        };

        let subtypes = self.subtypes_from_analysis(&fqn);

        info!("Found {} subtypes for {}", subtypes.len(), data.fqn);
        Some(subtypes)
    }

    fn subtypes_from_analysis(&self, fqn: &FullyQualifiedName) -> Vec<TypeHierarchyItem> {
        let engine = self.analysis_engine().expect(
            "INVARIANT VIOLATED: subtype query requires an analysis engine. \
             This is a bug because LSP typeHierarchy should be a thin wrapper over AnalysisEngine. \
             Fix: construct EngineQuery with with_engine().",
        );
        let engine = engine.lock();
        if engine.graph_nodes_for(fqn).is_empty() {
            return Vec::new();
        }

        let mut subclass_edges = Vec::new();
        let mut included_by_edges = Vec::new();
        let mut prepended_by_edges = Vec::new();
        let mut extended_by_edges = Vec::new();

        for edge in engine.all_graph_edges() {
            if &edge.target != fqn {
                continue;
            }
            match edge.kind {
                GraphEdgeKind::Superclass => subclass_edges.push(edge),
                GraphEdgeKind::Include
                    if edge.source.namespace_kind() == Some(NamespaceKind::Instance) =>
                {
                    included_by_edges.push(edge)
                }
                GraphEdgeKind::Prepend
                    if edge.source.namespace_kind() == Some(NamespaceKind::Instance) =>
                {
                    prepended_by_edges.push(edge)
                }
                GraphEdgeKind::Include | GraphEdgeKind::Prepend => {}
                GraphEdgeKind::Extend => extended_by_edges.push(edge),
            }
        }

        let mut subtypes = Vec::new();
        push_analysis_subtype_items(
            &engine,
            &mut subclass_edges,
            RelationType::Subclass,
            &mut subtypes,
        );
        push_analysis_subtype_items(
            &engine,
            &mut included_by_edges,
            RelationType::IncludedBy,
            &mut subtypes,
        );
        push_analysis_subtype_items(
            &engine,
            &mut prepended_by_edges,
            RelationType::PrependedBy,
            &mut subtypes,
        );
        push_analysis_subtype_items(
            &engine,
            &mut extended_by_edges,
            RelationType::ExtendedBy,
            &mut subtypes,
        );

        subtypes
    }

    fn supertypes_from_analysis(&self, fqn: &FullyQualifiedName) -> Vec<TypeHierarchyItem> {
        let engine = self.analysis_engine().expect(
            "INVARIANT VIOLATED: supertype query requires an analysis engine. \
             This is a bug because LSP typeHierarchy should be a thin wrapper over AnalysisEngine. \
             Fix: construct EngineQuery with with_engine().",
        );
        let engine = engine.lock();
        let primary_file_id = match engine.graph_nodes_for(fqn).first() {
            Some(node) => node.range.file_id,
            None => {
                return Vec::new();
            }
        };

        let edges = engine.graph_edges_from(fqn);
        let mut supertypes = Vec::new();

        push_analysis_supertype_items(
            &engine,
            edges,
            GraphEdgeKind::Prepend,
            RelationType::Prepend,
            primary_file_id,
            &mut supertypes,
        );
        push_analysis_supertype_items(
            &engine,
            edges,
            GraphEdgeKind::Include,
            RelationType::Include,
            primary_file_id,
            &mut supertypes,
        );
        push_analysis_supertype_items(
            &engine,
            edges,
            GraphEdgeKind::Superclass,
            RelationType::Superclass,
            primary_file_id,
            &mut supertypes,
        );
        push_analysis_supertype_items(
            &engine,
            edges,
            GraphEdgeKind::Extend,
            RelationType::Extend,
            primary_file_id,
            &mut supertypes,
        );
        push_unresolved_supertype_items(&engine, fqn, primary_file_id, &mut supertypes);

        supertypes
    }

    fn type_hierarchy_item_from_analysis(
        &self,
        fqn: &FullyQualifiedName,
    ) -> Option<TypeHierarchyItem> {
        let engine = self.analysis_engine().expect(
            "INVARIANT VIOLATED: type hierarchy item query requires an analysis engine. \
             This is a bug because LSP typeHierarchy should be a thin wrapper over AnalysisEngine. \
             Fix: construct EngineQuery with with_engine().",
        );
        let engine = engine.lock();
        let node = engine.graph_nodes_for(fqn).first()?;
        let location = location_for_range(&engine, node.range)?;
        let kind = match node.kind {
            GraphNodeKind::Class => SymbolKind::CLASS,
            GraphNodeKind::Module => SymbolKind::MODULE,
        };

        Some(TypeHierarchyItem {
            name: fqn.name(),
            kind,
            tags: None,
            detail: Some(fqn.to_string()),
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
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Parse a FQN string (e.g., "Foo::Bar::Baz") back to FullyQualifiedName
fn parse_fqn_string(fqn_str: &str) -> Option<FullyQualifiedName> {
    let parts: Vec<&str> = fqn_str.split("::").collect();
    let namespace: Vec<RubyConstant> = parts
        .iter()
        .filter_map(|p| RubyConstant::new(p).ok())
        .collect();

    if namespace.is_empty() {
        return None;
    }

    // Return as Namespace FQN (classes/modules are stored as Namespace)
    Some(FullyQualifiedName::namespace(namespace))
}

fn resolve_constant_fqn_from_analysis(
    query: &EngineQuery,
    constant_parts: &[RubyConstant],
    ancestors: &[RubyConstant],
) -> Option<FullyQualifiedName> {
    let engine = query.analysis_engine()?;
    let engine = engine.lock();
    let mut search_namespaces = ancestors.to_vec();

    while !search_namespaces.is_empty() {
        let mut combined_ns = search_namespaces.clone();
        combined_ns.extend(constant_parts.iter().cloned());

        let search_namespace_fqn = FullyQualifiedName::namespace(combined_ns.clone());
        if !engine.graph_nodes_for(&search_namespace_fqn).is_empty() {
            return Some(search_namespace_fqn);
        }

        let search_constant_fqn = FullyQualifiedName::Constant(combined_ns);
        if !engine.symbol_facts_for(&search_constant_fqn).is_empty() {
            return Some(search_constant_fqn);
        }

        search_namespaces.pop();
    }

    let root_namespace_fqn = FullyQualifiedName::namespace(constant_parts.to_vec());
    if !engine.graph_nodes_for(&root_namespace_fqn).is_empty() {
        return Some(root_namespace_fqn);
    }

    let root_fqn = FullyQualifiedName::Constant(constant_parts.to_vec());
    if !engine.symbol_facts_for(&root_fqn).is_empty() {
        return Some(root_fqn);
    }

    None
}

fn push_analysis_subtype_items(
    engine: &ruby_analysis_engine::AnalysisEngine,
    edges: &mut [ruby_analysis_core::GraphEdgeFact],
    relation: RelationType,
    items: &mut Vec<TypeHierarchyItem>,
) {
    edges.sort_by(|left, right| left.source.to_string().cmp(&right.source.to_string()));
    for edge in edges {
        if let Some(item) =
            graph_source_to_type_hierarchy_item_with_relation(engine, &edge.source, relation)
        {
            items.push(item);
        }
    }
}

fn push_analysis_supertype_items(
    engine: &ruby_analysis_engine::AnalysisEngine,
    edges: &[GraphEdgeFact],
    kind: GraphEdgeKind,
    relation: RelationType,
    primary_file_id: ruby_analysis_core::SourceFileId,
    items: &mut Vec<TypeHierarchyItem>,
) {
    edges
        .iter()
        .filter(|edge| edge.kind == kind)
        .rev()
        .filter_map(|edge| {
            graph_target_to_type_hierarchy_item_with_relation(
                engine,
                &edge.target,
                relation,
                Some((primary_file_id, edge.range.file_id)),
            )
        })
        .for_each(|item| items.push(item));
}

fn push_unresolved_supertype_items(
    engine: &ruby_analysis_engine::AnalysisEngine,
    fqn: &FullyQualifiedName,
    primary_file_id: ruby_analysis_core::SourceFileId,
    items: &mut Vec<TypeHierarchyItem>,
) {
    for edge in engine.unresolved_graph_edges() {
        if edge.source != *fqn {
            continue;
        }
        let relation = match edge.kind {
            GraphEdgeKind::Superclass => RelationType::Superclass,
            GraphEdgeKind::Include => RelationType::Include,
            GraphEdgeKind::Prepend => RelationType::Prepend,
            GraphEdgeKind::Extend => RelationType::Extend,
        };
        let display_fqn = FullyQualifiedName::Constant(edge.target_parts.clone());
        let Some(location) = location_for_range(engine, edge.range) else {
            continue;
        };
        let mut detail = format!(
            "{} ({}) ❓ definition not found",
            display_fqn,
            relation.label()
        );
        if edge.range.file_id != primary_file_id {
            detail = format!(
                "{} ⚠️ from {}",
                detail,
                file_name_for(engine, edge.range.file_id)
            );
        }

        items.push(TypeHierarchyItem {
            name: display_fqn.name(),
            kind: relation.symbol_kind(),
            tags: None,
            detail: Some(detail),
            uri: location.uri,
            range: location.range,
            selection_range: location.range,
            data: Some(
                serde_json::to_value(TypeHierarchyData {
                    fqn: display_fqn.to_string(),
                })
                .ok()
                .expect("INVARIANT VIOLATED: serializing type hierarchy data failed. This is a bug because TypeHierarchyData contains only a string. Fix: keep TypeHierarchyData serializable."),
            ),
        });
    }
}

fn graph_source_to_type_hierarchy_item_with_relation(
    engine: &ruby_analysis_engine::AnalysisEngine,
    fqn: &FullyQualifiedName,
    relation: RelationType,
) -> Option<TypeHierarchyItem> {
    let node = engine.graph_nodes_for(fqn).first()?;
    let location = location_for_range(engine, node.range)?;
    let detail = format!("{} ({})", fqn, relation.label());

    Some(TypeHierarchyItem {
        name: fqn.name(),
        kind: relation.symbol_kind(),
        tags: None,
        detail: Some(detail),
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

fn graph_target_to_type_hierarchy_item_with_relation(
    engine: &ruby_analysis_engine::AnalysisEngine,
    fqn: &FullyQualifiedName,
    relation: RelationType,
    file_context: Option<(
        ruby_analysis_core::SourceFileId,
        ruby_analysis_core::SourceFileId,
    )>,
) -> Option<TypeHierarchyItem> {
    let node = engine.graph_nodes_for(fqn).first()?;
    let location = location_for_range(engine, node.range)?;
    let mut detail = format!("{} ({})", fqn, relation.label());
    if let Some((primary_file_id, edge_file_id)) = file_context {
        if edge_file_id != primary_file_id {
            detail = format!("{} ⚠️ from {}", detail, file_name_for(engine, edge_file_id));
        }
    }

    Some(TypeHierarchyItem {
        name: fqn.name(),
        kind: relation.symbol_kind(),
        tags: None,
        detail: Some(detail),
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

fn file_name_for(
    engine: &ruby_analysis_engine::AnalysisEngine,
    file_id: ruby_analysis_core::SourceFileId,
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
    fn test_parse_fqn_string() {
        let fqn = parse_fqn_string("Foo::Bar::Baz").unwrap();
        assert_eq!(fqn.to_string(), "Foo::Bar::Baz");

        let fqn = parse_fqn_string("User").unwrap();
        assert_eq!(fqn.to_string(), "User");

        assert!(parse_fqn_string("").is_none());
    }

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
