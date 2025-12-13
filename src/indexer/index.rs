//! Ruby Index
//!
//! The central data structure for storing all indexed Ruby code information.
//! This includes definitions, references, method lookups, mixin relationships,
//! and prefix-based search capabilities.

use std::collections::{HashMap, HashSet};

use log::debug;
use tower_lsp::lsp_types::{Location, Url};

use crate::indexer::entry::{entry_kind::EntryKind, Entry, MixinType};
use crate::indexer::prefix_tree::PrefixTree;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_method::RubyMethod;

// Re-export for backward compatibility
pub use crate::types::unresolved_index::{UnresolvedEntry, UnresolvedIndex};

// ============================================================================
// Types
// ============================================================================

/// Represents a single mixin usage with its type and location
#[derive(Debug, Clone, PartialEq)]
pub struct MixinUsage {
    /// The class/module that uses the mixin
    pub user_fqn: FullyQualifiedName,
    /// The type of mixin (include, prepend, extend)
    pub mixin_type: MixinType,
    /// Location of the mixin call
    pub location: Location,
}

// ============================================================================
// RubyIndex
// ============================================================================

/// The main index storing all Ruby code information.
#[derive(Debug)]
pub struct RubyIndex {
    /// File URI to entries map (e.g., file:///test.rb => [Entry1, Entry2, ...])
    pub file_entries: HashMap<Url, Vec<Entry>>,

    /// FQN to definition entries map
    pub definitions: HashMap<FullyQualifiedName, Vec<Entry>>,

    /// FQN to reference locations map
    pub references: HashMap<FullyQualifiedName, Vec<Location>>,

    /// Reverse index: URI to FQNs that have references from that file
    /// Used for O(1) removal of references when a file changes
    references_by_uri: HashMap<Url, std::collections::HashSet<FullyQualifiedName>>,

    /// Method name to entries map (for method lookup without receiver type)
    pub methods_by_name: HashMap<RubyMethod, Vec<Entry>>,

    /// Reverse mixin tracking: module FQN -> list of classes/modules that include/extend/prepend it
    pub reverse_mixins: HashMap<FullyQualifiedName, Vec<FullyQualifiedName>>,

    /// Detailed mixin usage tracking for CodeLens (module FQN -> usages with type and location)
    pub mixin_usages: HashMap<FullyQualifiedName, Vec<MixinUsage>>,

    /// Reverse index: URI to module FQNs that have mixin usages from that file
    /// Used for O(1) removal of mixin usages when a file changes
    mixin_usages_by_uri: HashMap<Url, std::collections::HashSet<FullyQualifiedName>>,

    /// Prefix tree for fast auto-completion lookups
    pub prefix_tree: PrefixTree,

    /// Unresolved entries index (encapsulates forward and reverse lookups)
    pub unresolved: UnresolvedIndex,
}

impl RubyIndex {
    pub fn new() -> Self {
        RubyIndex {
            file_entries: HashMap::new(),
            definitions: HashMap::new(),
            references: HashMap::new(),
            references_by_uri: HashMap::new(),
            methods_by_name: HashMap::new(),
            reverse_mixins: HashMap::new(),
            mixin_usages: HashMap::new(),
            mixin_usages_by_uri: HashMap::new(),
            prefix_tree: PrefixTree::new(),
            unresolved: UnresolvedIndex::new(),
        }
    }

    // ========================================================================
    // Entry Management
    // ========================================================================

    /// Add an entry to the index
    pub fn add_entry(&mut self, entry: Entry) {
        // Add to file entries
        self.file_entries
            .entry(entry.location.uri.clone())
            .or_default()
            .push(entry.clone());

        // Add to definitions
        self.definitions
            .entry(entry.fqn.clone())
            .or_default()
            .push(entry.clone());

        // Add to methods_by_name if it's a method
        if let EntryKind::Method { name, .. } = &entry.kind {
            self.methods_by_name
                .entry(name.clone())
                .or_default()
                .push(entry.clone());
        }

        // Update mixin tracking
        self.update_reverse_mixins(&entry);

        // Add to prefix tree
        self.add_to_prefix_tree(&entry);
    }

    /// Remove all entries for a URI and return the FQNs that were completely removed
    /// (i.e., had no remaining definitions in other files)
    pub fn remove_entries_for_uri(&mut self, uri: &Url) -> Vec<FullyQualifiedName> {
        let Some(entries) = self.file_entries.remove(uri) else {
            return Vec::new();
        };

        // Track FQNs that are completely removed (no definitions left)
        let mut removed_fqns = Vec::new();

        // Remove from definitions
        for entry in &entries {
            if let Some(fqn_entries) = self.definitions.get_mut(&entry.fqn) {
                fqn_entries.retain(|e| e.location.uri != *uri);
                if fqn_entries.is_empty() {
                    self.definitions.remove(&entry.fqn);
                    removed_fqns.push(entry.fqn.clone());
                }
            }
        }

        // Clean up mixin tracking for completely removed FQNs
        // This handles the case where a module is deleted but files that include it
        // haven't been re-indexed yet
        for fqn in &removed_fqns {
            self.reverse_mixins.remove(fqn);
            self.mixin_usages.remove(fqn);
        }

        // Remove from methods_by_name
        let method_names: Vec<RubyMethod> = entries
            .iter()
            .filter_map(|e| {
                if let EntryKind::Method { name, .. } = &e.kind {
                    Some(name.clone())
                } else {
                    None
                }
            })
            .collect();

        for method_name in method_names {
            if let Some(method_entries) = self.methods_by_name.get_mut(&method_name) {
                method_entries.retain(|e| e.location.uri != *uri);
                if method_entries.is_empty() {
                    self.methods_by_name.remove(&method_name);
                }
            }
        }

        // Remove mixin usages - O(usages_in_file) instead of O(total_usages)
        if let Some(module_fqns) = self.mixin_usages_by_uri.remove(uri) {
            for module_fqn in module_fqns {
                if let Some(usages) = self.mixin_usages.get_mut(&module_fqn) {
                    usages.retain(|usage| usage.location.uri != *uri);
                    if usages.is_empty() {
                        self.mixin_usages.remove(&module_fqn);
                    }
                }
            }
        }

        // Remove from prefix tree
        self.remove_from_prefix_tree(&entries);

        removed_fqns
    }

    /// Mark references to the given FQNs as unresolved in their respective files
    /// Returns the set of URIs that were affected (for diagnostic publishing)
    ///
    /// OPTIMIZED: Uses HashSet for batch deduplication instead of O(N) Vec::contains()
    pub fn mark_references_as_unresolved(
        &mut self,
        removed_fqns: &[FullyQualifiedName],
    ) -> HashSet<Url> {
        self.unresolved
            .mark_references_as_unresolved(removed_fqns, &self.references)
    }

    /// Clear unresolved entries that match the given FQNs (now that they're defined)
    /// Returns the set of URIs that were affected (for diagnostic publishing)
    ///
    /// Uses Ruby's reverse namespace lookup to determine if a newly defined FQN
    /// would resolve an unresolved reference.
    ///
    /// OPTIMIZED: Uses `unresolved_by_name` reverse index to only check files
    /// that have unresolved refs matching the added FQN names.
    pub fn clear_resolved_entries(&mut self, added_fqns: &[FullyQualifiedName]) -> HashSet<Url> {
        self.unresolved
            .clear_resolved(added_fqns, Self::would_fqn_resolve_reference)
    }

    /// Check if any of the added FQNs would resolve the given unresolved reference
    /// using Ruby's reverse namespace lookup algorithm.
    ///
    /// Ruby looks for constants in this order:
    /// 1. Current namespace + name (e.g., Outer::Inner::Name for "Name" in Outer::Inner)
    /// 2. Parent namespace + name (e.g., Outer::Name)
    /// 3. ... up to root
    /// 4. Root namespace (e.g., ::Name)
    fn would_fqn_resolve_reference(
        unresolved_name: &str,
        namespace_context: &[String],
        added_fqns: &std::collections::HashSet<String>,
    ) -> bool {
        // If the unresolved name contains "::", it's an explicit path
        // Only exact match or path from current context would work
        if unresolved_name.contains("::") {
            // Try from each ancestor namespace
            let mut ancestors = namespace_context.to_vec();
            while !ancestors.is_empty() {
                let candidate = format!("{}::{}", ancestors.join("::"), unresolved_name);
                if added_fqns.contains(&candidate) {
                    return true;
                }
                ancestors.pop();
            }
            // Try as absolute path (root level)
            return added_fqns.contains(unresolved_name);
        }

        // Simple name - Ruby does reverse namespace lookup
        // Try from current namespace up to root
        let mut ancestors = namespace_context.to_vec();
        while !ancestors.is_empty() {
            let candidate = format!("{}::{}", ancestors.join("::"), unresolved_name);
            if added_fqns.contains(&candidate) {
                return true;
            }
            ancestors.pop();
        }

        // Finally check root namespace
        added_fqns.contains(unresolved_name)
    }

    /// Get entries by FQN
    pub fn get(&self, fqn: &FullyQualifiedName) -> Option<&Vec<Entry>> {
        self.definitions.get(fqn)
    }

    /// Get mutable entries by FQN
    pub fn get_mut(&mut self, fqn: &FullyQualifiedName) -> Option<&mut Vec<Entry>> {
        self.definitions.get_mut(fqn)
    }

    // ========================================================================
    // Reference Management
    // ========================================================================

    /// Add a reference to a FQN
    pub fn add_reference(&mut self, fqn: FullyQualifiedName, location: Location) {
        debug!("Adding reference: {:?}", fqn);
        // Track which FQNs have references from this URI (for fast removal)
        self.references_by_uri
            .entry(location.uri.clone())
            .or_default()
            .insert(fqn.clone());
        self.references.entry(fqn).or_default().push(location);
    }

    /// Remove all references for a URI - O(refs_in_file) instead of O(total_refs)
    pub fn remove_references_for_uri(&mut self, uri: &Url) {
        // Use the reverse index to only touch FQNs that have references from this URI
        if let Some(fqns) = self.references_by_uri.remove(uri) {
            for fqn in fqns {
                if let Some(refs) = self.references.get_mut(&fqn) {
                    refs.retain(|loc| loc.uri != *uri);
                    if refs.is_empty() {
                        self.references.remove(&fqn);
                    }
                }
            }
        }
    }

    // ========================================================================
    // Unresolved Entries (Diagnostics)
    // ========================================================================

    /// Add an unresolved entry for a file
    pub fn add_unresolved_entry(&mut self, uri: Url, entry: UnresolvedEntry) {
        self.unresolved.add(uri, entry);
    }

    /// Remove all unresolved entries for a file
    pub fn remove_unresolved_entries_for_uri(&mut self, uri: &Url) {
        self.unresolved.remove_for_uri(uri);
    }

    /// Get all unresolved entries for a file
    pub fn get_unresolved_entries(&self, uri: &Url) -> Vec<UnresolvedEntry> {
        self.unresolved.get(uri)
    }

    /// Get all unresolved constants for a file
    pub fn get_unresolved_constants(&self, uri: &Url) -> Vec<UnresolvedEntry> {
        self.unresolved
            .get(uri)
            .into_iter()
            .filter(|e| e.is_constant())
            .collect()
    }

    /// Get all unresolved methods for a file
    pub fn get_unresolved_methods(&self, uri: &Url) -> Vec<UnresolvedEntry> {
        self.unresolved
            .get(uri)
            .into_iter()
            .filter(|e| e.is_method())
            .collect()
    }

    // ========================================================================
    // Mixin Tracking
    // ========================================================================

    /// Update reverse mixin tracking when an entry with mixins is added
    pub fn update_reverse_mixins(&mut self, entry: &Entry) {
        use crate::indexer::ancestor_chain::resolve_mixin_ref;

        let (includes, extends, prepends) = match &entry.kind {
            EntryKind::Class {
                includes,
                extends,
                prepends,
                ..
            } => (includes, extends, prepends),
            EntryKind::Module {
                includes,
                extends,
                prepends,
                ..
            } => (includes, extends, prepends),
            _ => return,
        };

        debug!("Updating reverse mixins for entry: {:?}", entry.fqn);

        let mixin_groups = [
            (includes, MixinType::Include),
            (extends, MixinType::Extend),
            (prepends, MixinType::Prepend),
        ];

        for (mixin_refs, mixin_type) in mixin_groups {
            for mixin_ref in mixin_refs {
                let Some(resolved_fqn) = resolve_mixin_ref(self, mixin_ref, &entry.fqn) else {
                    debug!("Failed to resolve mixin ref: {:?}", mixin_ref);
                    continue;
                };

                debug!("Resolved mixin ref {:?} to {:?}", mixin_ref, resolved_fqn);

                // Update reverse_mixins
                let including = self.reverse_mixins.entry(resolved_fqn.clone()).or_default();
                if !including.contains(&entry.fqn) {
                    including.push(entry.fqn.clone());
                }

                // Update mixin_usages
                let usage = MixinUsage {
                    user_fqn: entry.fqn.clone(),
                    mixin_type,
                    location: entry.location.clone(),
                };

                // Track which modules have usages from this URI (for fast removal)
                self.mixin_usages_by_uri
                    .entry(entry.location.uri.clone())
                    .or_default()
                    .insert(resolved_fqn.clone());

                let usages = self.mixin_usages.entry(resolved_fqn).or_default();
                if !usages.contains(&usage) {
                    usages.push(usage);
                }
            }
        }
    }

    /// Update reverse mixin tracking with specific call location
    pub fn update_reverse_mixins_with_location(
        &mut self,
        entry: &Entry,
        mixin_refs: &[crate::indexer::entry::MixinRef],
        mixin_type: MixinType,
        call_location: Location,
    ) {
        use crate::indexer::ancestor_chain::resolve_mixin_ref;

        for mixin_ref in mixin_refs {
            let Some(resolved_fqn) = resolve_mixin_ref(self, mixin_ref, &entry.fqn) else {
                debug!("Failed to resolve mixin ref: {:?}", mixin_ref);
                continue;
            };

            // Update reverse_mixins
            let including = self.reverse_mixins.entry(resolved_fqn.clone()).or_default();
            if !including.contains(&entry.fqn) {
                including.push(entry.fqn.clone());
            }

            // Update mixin_usages with actual call location
            let usage = MixinUsage {
                user_fqn: entry.fqn.clone(),
                mixin_type,
                location: call_location.clone(),
            };

            // Track which modules have usages from this URI (for fast removal)
            self.mixin_usages_by_uri
                .entry(call_location.uri.clone())
                .or_default()
                .insert(resolved_fqn.clone());

            let usages = self.mixin_usages.entry(resolved_fqn).or_default();
            if !usages.contains(&usage) {
                usages.push(usage);
            }
        }
    }

    /// Resolve all mixin references (call after all definitions are indexed)
    pub fn resolve_all_mixins(&mut self) {
        debug!("Resolving all mixin references");

        let entries: Vec<_> = self.definitions.values().flatten().cloned().collect();

        for entry in entries {
            self.update_reverse_mixins(&entry);
        }

        debug!(
            "Resolved mixins: {} modules have usages tracked",
            self.mixin_usages.len()
        );
    }

    /// Resolve mixin references only for entries in a specific file
    /// This is more efficient than resolve_all_mixins for incremental updates
    pub fn resolve_mixins_for_uri(&mut self, uri: &Url) {
        // Get FQNs from file_entries (O(1) lookup), then fetch from definitions
        let fqns_to_resolve: Vec<_> = self
            .file_entries
            .get(uri)
            .map(|entries| entries.iter().map(|e| e.fqn.clone()).collect())
            .unwrap_or_default();

        self.resolve_mixins_for_fqns(&fqns_to_resolve);
    }

    /// Resolve mixin references for specific FQNs
    /// More efficient than iterating all definitions
    pub fn resolve_mixins_for_fqns(&mut self, fqns: &[FullyQualifiedName]) {
        for fqn in fqns {
            // Look up the entry in definitions (where mixins have been added)
            if let Some(entries) = self.definitions.get(fqn) {
                // Get the last entry (most recent definition)
                if let Some(entry) = entries.last().cloned() {
                    self.update_reverse_mixins(&entry);
                }
            }
        }
    }

    /// Get all classes/modules that include the given module
    pub fn get_including_classes(
        &self,
        module_fqn: &FullyQualifiedName,
    ) -> Vec<FullyQualifiedName> {
        self.reverse_mixins
            .get(module_fqn)
            .cloned()
            .unwrap_or_default()
    }

    /// Get all mixin usages for a given module (for CodeLens)
    pub fn get_mixin_usages(&self, module_fqn: &FullyQualifiedName) -> Vec<MixinUsage> {
        self.mixin_usages
            .get(module_fqn)
            .cloned()
            .unwrap_or_default()
    }

    /// Get class definition locations for classes that use a module
    pub fn get_class_definition_locations(&self, module_fqn: &FullyQualifiedName) -> Vec<Location> {
        self.get_transitive_mixin_classes(module_fqn)
            .keys()
            .filter_map(|class_fqn| {
                self.definitions.get(class_fqn).and_then(|entries| {
                    entries.first().and_then(|entry| {
                        if matches!(entry.kind, EntryKind::Class { .. }) {
                            Some(entry.location.clone())
                        } else {
                            None
                        }
                    })
                })
            })
            .collect()
    }

    /// Get all classes that transitively include/extend/prepend a module
    pub fn get_transitive_mixin_classes(
        &self,
        module_fqn: &FullyQualifiedName,
    ) -> HashMap<FullyQualifiedName, Vec<Vec<FullyQualifiedName>>> {
        let mut result = HashMap::new();
        let mut visited = std::collections::HashSet::new();
        self.collect_transitive_users(module_fqn, &mut result, &mut visited, vec![]);
        result
    }

    fn collect_transitive_users(
        &self,
        module_fqn: &FullyQualifiedName,
        result: &mut HashMap<FullyQualifiedName, Vec<Vec<FullyQualifiedName>>>,
        visited: &mut std::collections::HashSet<FullyQualifiedName>,
        path: Vec<FullyQualifiedName>,
    ) {
        if !visited.insert(module_fqn.clone()) {
            return;
        }

        if let Some(usages) = self.mixin_usages.get(module_fqn) {
            for usage in usages {
                if let Some(entries) = self.definitions.get(&usage.user_fqn) {
                    if let Some(entry) = entries.first() {
                        let mut current_path = path.clone();
                        current_path.push(module_fqn.clone());

                        match &entry.kind {
                            EntryKind::Class { .. } => {
                                result
                                    .entry(usage.user_fqn.clone())
                                    .or_default()
                                    .push(current_path);
                            }
                            EntryKind::Module { .. } => {
                                self.collect_transitive_users(
                                    &usage.user_fqn,
                                    result,
                                    visited,
                                    current_path,
                                );
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        visited.remove(module_fqn);
    }

    // ========================================================================
    // Prefix Tree (Auto-completion)
    // ========================================================================

    /// Search for entries by prefix
    pub fn search_by_prefix(&self, prefix: &str) -> Vec<Entry> {
        self.prefix_tree.search(prefix)
    }

    fn add_to_prefix_tree(&mut self, entry: &Entry) {
        let key = entry.fqn.name();
        if !key.is_empty() {
            self.prefix_tree.insert(&key, entry.clone());
        }
    }

    fn remove_from_prefix_tree(&mut self, entries: &[Entry]) {
        for entry in entries {
            let key = entry.fqn.name();
            if !key.is_empty() {
                self.prefix_tree.delete(&key);
                debug!("Removed entry from prefix tree: {}", key);
            }
        }
    }
}

impl Default for RubyIndex {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::entry::entry_builder::EntryBuilder;
    use crate::types::ruby_namespace::RubyConstant;

    fn create_test_entry(name: &str, uri: &Url) -> Entry {
        let fqn = FullyQualifiedName::from(vec![RubyConstant::try_from(name).unwrap()]);
        EntryBuilder::new()
            .fqn(fqn)
            .location(Location {
                uri: uri.clone(),
                range: Default::default(),
            })
            .kind(EntryKind::new_class(None))
            .build()
            .unwrap()
    }

    #[test]
    fn test_add_entry() {
        let mut index = RubyIndex::new();
        let uri = Url::parse("file://test.rb").unwrap();
        let entry = create_test_entry("Test", &uri);

        index.add_entry(entry);

        assert_eq!(index.definitions.len(), 1);
        assert_eq!(index.references.len(), 0);
    }

    #[test]
    fn test_remove_entries_for_uri() {
        let mut index = RubyIndex::new();
        let uri = Url::parse("file:///test.rb").unwrap();
        let entry = create_test_entry("Test", &uri);

        index.add_entry(entry);
        assert_eq!(index.definitions.len(), 1);

        index.remove_entries_for_uri(&uri);
        assert_eq!(index.definitions.len(), 0);
    }

    #[test]
    fn test_prefix_tree_search() {
        let mut index = RubyIndex::new();
        let uri = Url::parse("file://test.rb").unwrap();

        index.add_entry(create_test_entry("TestClass", &uri));
        index.add_entry(create_test_entry("TestModule", &uri));

        assert_eq!(index.search_by_prefix("Test").len(), 2);
        assert_eq!(index.search_by_prefix("TestC").len(), 1);
        assert_eq!(index.search_by_prefix("NonExistent").len(), 0);
    }
}
