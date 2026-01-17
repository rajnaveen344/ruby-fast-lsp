//! Type Hierarchy Capability
//!
//! Implements the LSP Type Hierarchy feature for Ruby classes and modules.
//! This provides functionality similar to Ruby's `Class.ancestors` and finding
//! subclasses/subtypes.
//!
//! The type hierarchy has three parts:
//! 1. `textDocument/prepareTypeHierarchy` - Find the class/module at cursor position
//! 2. `typeHierarchy/supertypes` - Get ancestors (superclass chain + included modules)
//! 3. `typeHierarchy/subtypes` - Get descendants (subclasses + mixers)
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
use serde::{Deserialize, Serialize};
use tower_lsp::lsp_types::{
    SymbolKind, TypeHierarchyItem, TypeHierarchyPrepareParams, TypeHierarchySubtypesParams,
    TypeHierarchySupertypesParams,
};

use crate::analyzer_prism::utils::resolve_constant_fqn_from_parts;
use crate::analyzer_prism::{Identifier, RubyPrismAnalyzer};
use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::entry::MixinRef;
use crate::indexer::index::{FileId, FqnId, RubyIndex};
use crate::server::RubyLanguageServer;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_namespace::RubyConstant;

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
// Prepare Type Hierarchy
// ============================================================================

/// Handle `textDocument/prepareTypeHierarchy` request
///
/// Returns a TypeHierarchyItem for the class or module at the cursor position.
pub async fn handle_prepare_type_hierarchy(
    server: &RubyLanguageServer,
    params: TypeHierarchyPrepareParams,
) -> Option<Vec<TypeHierarchyItem>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    info!(
        "Prepare type hierarchy request for {:?} at {:?}",
        uri.path(),
        position
    );

    // Get document content
    let doc = server.get_doc(&uri)?;
    let content = doc.content.clone();

    // Use analyzer to find identifier at position
    let analyzer = RubyPrismAnalyzer::new(uri.clone(), content);
    let (identifier, _, ancestors, _scope_id) = analyzer.get_identifier(position);

    let identifier = identifier?;

    // Only handle constants (class/module names)
    let constant_parts = match &identifier {
        Identifier::RubyConstant { namespace: _, iden } => iden.clone(),
        _ => {
            debug!("Type hierarchy only supports constants (classes/modules)");
            return None;
        }
    };

    let index = server.index.lock();

    // Resolve the FQN for this constant
    let fqn = resolve_constant_fqn(&constant_parts, &ancestors, &index)?;

    // Get the entry for this FQN to get location info
    let entries = index.get(&fqn)?;
    let entry = entries.first()?;

    // Only support classes and modules
    let kind = match &entry.kind {
        EntryKind::Class(_) => SymbolKind::CLASS,
        EntryKind::Module(_) => SymbolKind::MODULE,
        _ => {
            debug!("Type hierarchy only supports classes and modules");
            return None;
        }
    };

    let location = index.to_lsp_location(&entry.location)?;

    let item = TypeHierarchyItem {
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
    };

    info!("Found type hierarchy item: {}", fqn);
    Some(vec![item])
}

// ============================================================================
// Supertypes (Ancestors)
// ============================================================================

/// Handle `typeHierarchy/supertypes` request
///
/// Returns the ancestor chain for a class/module in Ruby's Method Resolution Order:
/// 1. Prepended modules (last prepend first)
/// 2. Included modules (last include first)
/// 3. Superclass chain (recursively applying the same rules)
///
/// Extended modules are shown separately as they affect class methods.
///
/// This mirrors Ruby's `Module#ancestors` method behavior.
///
/// When a class is reopened in multiple files, mixins from all files are collected.
/// Mixins from files other than the primary definition show a warning indicator
/// since the actual MRO depends on runtime require order which we can't determine statically.
///
/// Returns `Some(vec![])` when there are no supertypes (valid but empty result).
/// Returns `None` only when the request data is malformed.
pub async fn handle_supertypes(
    server: &RubyLanguageServer,
    params: TypeHierarchySupertypesParams,
) -> Option<Vec<TypeHierarchyItem>> {
    let item = &params.item;

    // Extract FQN from data - return None only if data is malformed
    let data: TypeHierarchyData = match item
        .data
        .as_ref()
        .and_then(|d| serde_json::from_value(d.clone()).ok())
    {
        Some(d) => d,
        None => {
            info!("Supertypes request has malformed data");
            return None;
        }
    };

    info!("Supertypes request for: {}", data.fqn);

    let index = server.index.lock();

    // Parse the FQN string - return empty if can't parse (type might have been deleted)
    let fqn = match parse_fqn_string(&data.fqn) {
        Some(f) => f,
        None => {
            info!("Could not parse FQN: {}", data.fqn);
            return Some(vec![]);
        }
    };

    // Get ALL entries for this FQN (class may be reopened in multiple files)
    let entries = match index.get(&fqn) {
        Some(e) => e,
        None => {
            info!("Entry not found for: {}", data.fqn);
            return Some(vec![]);
        }
    };

    if entries.is_empty() {
        info!("No entries for: {}", data.fqn);
        return Some(vec![]);
    }

    // The primary file is where the first definition lives
    let primary_file_id = entries.first().unwrap().location.file_id;

    // Collect mixins from ALL entries (all reopenings of the class)
    let mut all_prepends: Vec<(&MixinRef, FileId)> = Vec::new();
    let mut all_includes: Vec<(&MixinRef, FileId)> = Vec::new();
    let mut all_extends: Vec<(&MixinRef, FileId)> = Vec::new();
    let mut superclass_ref: Option<(&MixinRef, FileId)> = None;

    for entry in &entries {
        let entry_file_id = entry.location.file_id;
        match &entry.kind {
            EntryKind::Class(class_data) => {
                // First superclass found wins (should only be one)
                if superclass_ref.is_none() {
                    if let Some(ref sc) = class_data.superclass {
                        superclass_ref = Some((sc, entry_file_id));
                    }
                }
                for mixin in &class_data.prepends {
                    all_prepends.push((mixin, entry_file_id));
                }
                for mixin in &class_data.includes {
                    all_includes.push((mixin, entry_file_id));
                }
                for mixin in &class_data.extends {
                    all_extends.push((mixin, entry_file_id));
                }
            }
            EntryKind::Module(module_data) => {
                for mixin in &module_data.prepends {
                    all_prepends.push((mixin, entry_file_id));
                }
                for mixin in &module_data.includes {
                    all_includes.push((mixin, entry_file_id));
                }
                for mixin in &module_data.extends {
                    all_extends.push((mixin, entry_file_id));
                }
            }
            _ => continue,
        }
    }

    let mut supertypes = Vec::new();

    // Build supertypes in MRO order (matching Ruby's Module#ancestors):
    // 1. Prepended modules (in reverse order - last prepend first)
    for (mixin_ref, entry_file_id) in all_prepends.iter().rev() {
        if let Some(item) = mixin_ref_to_type_hierarchy_item(
            &index,
            mixin_ref,
            RelationType::Prepend,
            &fqn,
            primary_file_id,
            *entry_file_id,
        ) {
            supertypes.push(item);
        }
    }

    // 2. Included modules (in reverse order - last include first)
    for (mixin_ref, entry_file_id) in all_includes.iter().rev() {
        if let Some(item) = mixin_ref_to_type_hierarchy_item(
            &index,
            mixin_ref,
            RelationType::Include,
            &fqn,
            primary_file_id,
            *entry_file_id,
        ) {
            supertypes.push(item);
        }
    }

    // 3. Superclass (if any)
    if let Some((superclass, entry_file_id)) = superclass_ref {
        if let Some(item) = mixin_ref_to_type_hierarchy_item(
            &index,
            superclass,
            RelationType::Superclass,
            &fqn,
            primary_file_id,
            entry_file_id,
        ) {
            supertypes.push(item);
        }
    }

    // 4. Extended modules (shown separately - these affect singleton class/class methods)
    // Shown in reverse order for consistency
    for (mixin_ref, entry_file_id) in all_extends.iter().rev() {
        if let Some(item) = mixin_ref_to_type_hierarchy_item(
            &index,
            mixin_ref,
            RelationType::Extend,
            &fqn,
            primary_file_id,
            *entry_file_id,
        ) {
            supertypes.push(item);
        }
    }

    info!("Found {} supertypes for {}", supertypes.len(), data.fqn);
    Some(supertypes)
}

// ============================================================================
// Subtypes (Descendants)
// ============================================================================

/// Handle `typeHierarchy/subtypes` request
///
/// Returns types that inherit from or mix in this class/module:
/// - For classes: direct subclasses
/// - For modules: classes/modules that include/prepend/extend this module
///
/// Subtypes are grouped by relationship type:
/// 1. Direct subclasses first
/// 2. Classes/modules that include this module
/// 3. Classes/modules that prepend this module
/// 4. Classes/modules that extend this module
///
/// Returns `Some(vec![])` when there are no subtypes (valid but empty result).
/// Returns `None` only when the request data is malformed.
pub async fn handle_subtypes(
    server: &RubyLanguageServer,
    params: TypeHierarchySubtypesParams,
) -> Option<Vec<TypeHierarchyItem>> {
    let item = &params.item;

    // Extract FQN from data - return None only if data is malformed
    let data: TypeHierarchyData = match item
        .data
        .as_ref()
        .and_then(|d| serde_json::from_value(d.clone()).ok())
    {
        Some(d) => d,
        None => {
            info!("Subtypes request has malformed data");
            return None;
        }
    };

    info!("Subtypes request for: {}", data.fqn);

    let index = server.index.lock();

    // Parse the FQN string - return empty if can't parse (type might have been deleted)
    let fqn = match parse_fqn_string(&data.fqn) {
        Some(f) => f,
        None => {
            info!("Could not parse FQN: {}", data.fqn);
            return Some(vec![]);
        }
    };

    // Get FQN ID - return empty if not in index (type might have been deleted)
    let fqn_id = match index.get_fqn_id(&fqn) {
        Some(id) => id,
        None => {
            info!("FQN not found in index: {}", data.fqn);
            return Some(vec![]);
        }
    };

    // Get the graph node - return empty if no node (no relationships recorded)
    let node = match index.graph.get_node(fqn_id) {
        Some(n) => n,
        None => {
            info!("No graph node for: {}", data.fqn);
            return Some(vec![]);
        }
    };

    let mut subtypes = Vec::new();

    // 1. Direct subclasses first (children in class hierarchy)
    for &child_id in &node.children {
        if let Some(item) =
            fqn_id_to_type_hierarchy_item_with_relation(&index, child_id, RelationType::Subclass)
        {
            subtypes.push(item);
        }
    }

    // 2. Classes/modules that include this module
    for &includer_id in &node.included_by {
        if let Some(item) = fqn_id_to_type_hierarchy_item_with_relation(
            &index,
            includer_id,
            RelationType::IncludedBy,
        ) {
            subtypes.push(item);
        }
    }

    // 3. Classes/modules that prepend this module
    for &prepender_id in &node.prepended_by {
        if let Some(item) = fqn_id_to_type_hierarchy_item_with_relation(
            &index,
            prepender_id,
            RelationType::PrependedBy,
        ) {
            subtypes.push(item);
        }
    }

    // Note: "extend" relationships are now modeled as includes on Singleton nodes,
    // so they're already covered by the included_by section above.

    info!("Found {} subtypes for {}", subtypes.len(), data.fqn);
    Some(subtypes)
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

/// Resolve a constant to its FQN, searching through ancestor namespaces
fn resolve_constant_fqn(
    constant_parts: &[RubyConstant],
    ancestors: &[RubyConstant],
    index: &RubyIndex,
) -> Option<FullyQualifiedName> {
    // Start with the current namespace and ancestors
    let mut search_namespaces = ancestors.to_vec();

    // Search through ancestor namespaces
    while !search_namespaces.is_empty() {
        let mut combined_ns = search_namespaces.clone();
        combined_ns.extend(constant_parts.iter().cloned());

        // Try as Namespace first (for class/module definitions)
        let search_namespace_fqn = FullyQualifiedName::namespace(combined_ns.clone());
        if index.get(&search_namespace_fqn).is_some() {
            return Some(search_namespace_fqn);
        }

        // Then try as Constant (for value constants)
        let search_constant_fqn = FullyQualifiedName::Constant(combined_ns);
        if index.get(&search_constant_fqn).is_some() {
            return Some(search_constant_fqn);
        }

        // Pop the last namespace and try again
        search_namespaces.pop();
    }

    // Try at root level - Namespace first, then Constant
    let root_namespace_fqn = FullyQualifiedName::namespace(constant_parts.to_vec());
    if index.get(&root_namespace_fqn).is_some() {
        return Some(root_namespace_fqn);
    }

    let root_fqn = FullyQualifiedName::Constant(constant_parts.to_vec());
    if index.get(&root_fqn).is_some() {
        return Some(root_fqn);
    }

    None
}

/// Convert a MixinRef to TypeHierarchyItem with relation type information
///
/// This version takes the mixin reference (which has location info) and compares
/// where the include/prepend/extend statement was written vs the primary class definition.
///
/// Parameters:
/// - `context_fqn`: The FQN of the class/module containing the mixin (for constant resolution)
/// - `primary_file_id`: The file where the class/module was first defined
/// - `entry_file_id`: The file where this particular entry (with this mixin) came from
///
/// If the entry is from a different file than the primary definition, a warning is added
/// since the MRO depends on runtime require order which we can't determine statically.
///
/// If the mixin module isn't found in the index, we still show it with a warning
/// indicating the definition wasn't found (could be from an external gem, stdlib, etc).
fn mixin_ref_to_type_hierarchy_item(
    index: &RubyIndex,
    mixin_ref: &MixinRef,
    relation: RelationType,
    context_fqn: &FullyQualifiedName,
    primary_file_id: FileId,
    entry_file_id: FileId,
) -> Option<TypeHierarchyItem> {
    // Resolve the mixin ref to a fully qualified name using Ruby's constant lookup rules
    // This handles cases like `include Users` inside `module GoshPosh::Platform::API`
    // which should resolve to `GoshPosh::Platform::API::Users`
    let resolved_fqn =
        resolve_constant_fqn_from_parts(index, &mixin_ref.parts, mixin_ref.absolute, context_fqn);

    // Create a display name for the mixin (using parts if not resolved)
    let display_fqn = resolved_fqn
        .clone()
        .unwrap_or_else(|| FullyQualifiedName::Constant(mixin_ref.parts.clone()));

    // Try to find the entry in the index using the resolved FQN
    let found_entry = resolved_fqn.as_ref().and_then(|fqn| {
        index.get(fqn).and_then(|entries| {
            entries.first().and_then(|entry| {
                // Verify this is a class or module
                match &entry.kind {
                    EntryKind::Class(_) | EntryKind::Module(_) => {
                        index.to_lsp_location(&entry.location)
                    }
                    _ => None,
                }
            })
        })
    });

    // If not found, create an item at the mixin_ref location with a warning
    let location = match found_entry {
        Some(loc) => loc,
        None => {
            // Use the location of the include/prepend/extend statement itself
            let loc = index.to_lsp_location(&mixin_ref.location)?;

            // Build a warning detail for unresolved mixin
            let detail = format!(
                "{} ({}) ❓ definition not found",
                display_fqn,
                relation.label()
            );

            return Some(TypeHierarchyItem {
                name: display_fqn.name(),
                kind: relation.symbol_kind(),
                tags: None,
                detail: Some(detail),
                uri: loc.uri,
                range: loc.range,
                selection_range: loc.range,
                data: Some(
                    serde_json::to_value(TypeHierarchyData {
                        fqn: display_fqn.to_string(),
                    })
                    .ok()?,
                ),
            });
        }
    };

    // Use relation-based SymbolKind for visual differentiation
    let kind = relation.symbol_kind();

    // Check if this mixin comes from a reopened class in a different file
    // The entry_file_id tells us which file contained this include/prepend/extend
    let is_cross_file = entry_file_id != primary_file_id;

    // Build detail with relation label
    // Format: "FQN (label)" or "FQN (label) ⚠️ from filename"
    let detail = if is_cross_file {
        // Get the filename for context
        if let Some(file_url) = index.get_file_url(entry_file_id) {
            // Extract filename from URL path
            let filename = file_url
                .path_segments()
                .and_then(|segs| segs.last())
                .unwrap_or("unknown");
            format!(
                "{} ({}) ⚠️ from {}",
                display_fqn,
                relation.label(),
                filename
            )
        } else {
            format!("{} ({}) ⚠️ different file", display_fqn, relation.label())
        }
    } else {
        format!("{} ({})", display_fqn, relation.label())
    };

    Some(TypeHierarchyItem {
        name: display_fqn.name(),
        kind,
        tags: None,
        detail: Some(detail),
        uri: location.uri,
        range: location.range,
        selection_range: location.range,
        data: Some(
            serde_json::to_value(TypeHierarchyData {
                fqn: display_fqn.to_string(),
            })
            .ok()?,
        ),
    })
}

/// Convert FqnId to TypeHierarchyItem with relation type information
///
/// The relation type is used to:
/// - Set an appropriate SymbolKind that visually distinguishes relationships
/// - Include emoji + relationship label in the detail field
///
/// This version is used for subtypes where we don't have location info about
/// where the include/prepend/extend statement was written.
fn fqn_id_to_type_hierarchy_item_with_relation(
    index: &RubyIndex,
    fqn_id: FqnId,
    relation: RelationType,
) -> Option<TypeHierarchyItem> {
    let fqn = index.get_fqn(fqn_id)?;
    let entries = index.get(fqn)?;
    let entry = entries.first()?;

    // Verify this is a class or module
    match &entry.kind {
        EntryKind::Class(_) | EntryKind::Module(_) => {}
        _ => return None,
    };

    let location = index.to_lsp_location(&entry.location)?;

    // Use relation-based SymbolKind for visual differentiation
    let kind = relation.symbol_kind();

    // Include relation label in detail for clarity
    // Format: "FQN (label)"
    let detail = format!("{} ({})", fqn, relation.label());

    Some(TypeHierarchyItem {
        name: fqn.name(),
        kind,
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
