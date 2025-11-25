//! Ruby Index
//!
//! The central data structure for storing all indexed Ruby code information.
//! This includes definitions, references, method lookups, mixin relationships,
//! and prefix-based search capabilities.

use std::collections::HashMap;

use log::debug;
use tower_lsp::lsp_types::{Location, Url};

use crate::indexer::entry::{entry_kind::EntryKind, Entry, MixinType};
use crate::indexer::prefix_tree::PrefixTree;
use crate::types::{fully_qualified_name::FullyQualifiedName, ruby_method::RubyMethod};

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

/// Represents an unresolved reference for diagnostics.
/// Used to report missing constants/classes/modules/methods.
#[derive(Debug, Clone, PartialEq)]
pub enum UnresolvedEntry {
    /// An unresolved constant reference (e.g., `Foo::Bar`)
    Constant {
        /// The constant name as written in the source (e.g., "Foo::Bar" or just "Bar")
        name: String,
        /// The namespace context where this reference was written
        /// e.g., ["Outer", "Inner"] for code inside `module Outer; module Inner; ... end; end`
        /// Used to determine if a newly defined constant would resolve this reference
        /// via Ruby's reverse namespace lookup
        namespace_context: Vec<String>,
        /// Location where the constant was referenced
        location: Location,
    },
    /// An unresolved method call (e.g., `foo.bar` or `bar`)
    Method {
        /// The method name as written in the source
        name: String,
        /// The receiver type if known (e.g., "Foo::Bar" for `Foo::Bar.method`)
        /// None for method calls without explicit receiver
        receiver: Option<String>,
        /// Location where the method was called
        location: Location,
    },
}

impl UnresolvedEntry {
    /// Create an unresolved constant entry with namespace context
    pub fn constant_with_context(
        name: String,
        namespace_context: Vec<String>,
        location: Location,
    ) -> Self {
        Self::Constant {
            name,
            namespace_context,
            location,
        }
    }

    /// Create an unresolved constant entry (legacy, assumes root context)
    pub fn constant(name: String, location: Location) -> Self {
        Self::Constant {
            name,
            namespace_context: Vec::new(),
            location,
        }
    }

    /// Create an unresolved method entry
    pub fn method(name: String, receiver: Option<String>, location: Location) -> Self {
        Self::Method {
            name,
            receiver,
            location,
        }
    }

    /// Get the location of this unresolved entry
    pub fn location(&self) -> &Location {
        match self {
            Self::Constant { location, .. } => location,
            Self::Method { location, .. } => location,
        }
    }

    /// Check if this is a constant entry
    pub fn is_constant(&self) -> bool {
        matches!(self, Self::Constant { .. })
    }

    /// Check if this is a method entry
    pub fn is_method(&self) -> bool {
        matches!(self, Self::Method { .. })
    }
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

    /// Method name to entries map (for method lookup without receiver type)
    pub methods_by_name: HashMap<RubyMethod, Vec<Entry>>,

    /// Reverse mixin tracking: module FQN -> list of classes/modules that include/extend/prepend it
    pub reverse_mixins: HashMap<FullyQualifiedName, Vec<FullyQualifiedName>>,

    /// Detailed mixin usage tracking for CodeLens (module FQN -> usages with type and location)
    pub mixin_usages: HashMap<FullyQualifiedName, Vec<MixinUsage>>,

    /// Prefix tree for fast auto-completion lookups
    pub prefix_tree: PrefixTree,

    /// Unresolved entries per file for diagnostics (constants and methods)
    pub unresolved_entries: HashMap<Url, Vec<UnresolvedEntry>>,
}

impl RubyIndex {
    pub fn new() -> Self {
        RubyIndex {
            file_entries: HashMap::new(),
            definitions: HashMap::new(),
            references: HashMap::new(),
            methods_by_name: HashMap::new(),
            reverse_mixins: HashMap::new(),
            mixin_usages: HashMap::new(),
            prefix_tree: PrefixTree::new(),
            unresolved_entries: HashMap::new(),
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

        // Remove mixin usages
        for usages in self.mixin_usages.values_mut() {
            usages.retain(|usage| usage.location.uri != *uri);
        }
        self.mixin_usages.retain(|_, usages| !usages.is_empty());

        // Remove from prefix tree
        self.remove_from_prefix_tree(&entries);

        removed_fqns
    }

    /// Mark references to the given FQNs as unresolved in their respective files
    /// Returns the set of URIs that were affected (for diagnostic publishing)
    pub fn mark_references_as_unresolved(
        &mut self,
        removed_fqns: &[FullyQualifiedName],
    ) -> std::collections::HashSet<Url> {
        let mut affected_uris = std::collections::HashSet::new();

        for fqn in removed_fqns {
            if let Some(ref_locations) = self.references.get(fqn) {
                for location in ref_locations {
                    affected_uris.insert(location.uri.clone());

                    // Add unresolved entry for this reference (avoiding duplicates)
                    let unresolved = UnresolvedEntry::constant(fqn.to_string(), location.clone());
                    let entries = self
                        .unresolved_entries
                        .entry(location.uri.clone())
                        .or_default();

                    // Only add if not already present
                    if !entries.contains(&unresolved) {
                        entries.push(unresolved);
                    }
                }
            }
        }

        affected_uris
    }

    /// Clear unresolved entries that match the given FQNs (now that they're defined)
    /// Returns the set of URIs that were affected (for diagnostic publishing)
    ///
    /// Uses Ruby's reverse namespace lookup to determine if a newly defined FQN
    /// would resolve an unresolved reference:
    ///
    /// For example, if we have unresolved "Inner" written inside `module Outer`:
    /// - Defining `Outer::Inner` WOULD resolve it (Ruby finds Outer::Inner first)
    /// - Defining `Other::Inner` would NOT resolve it (different namespace)
    /// - Defining `::Inner` (root) WOULD resolve it (Ruby falls back to root)
    pub fn clear_resolved_entries(
        &mut self,
        added_fqns: &[FullyQualifiedName],
    ) -> std::collections::HashSet<Url> {
        let mut affected_uris = std::collections::HashSet::new();

        // Build a set of all FQN strings for quick lookup
        let fqn_strings: std::collections::HashSet<String> =
            added_fqns.iter().map(|fqn| fqn.to_string()).collect();

        for (uri, entries) in self.unresolved_entries.iter_mut() {
            let original_len = entries.len();

            entries.retain(|entry| {
                if let UnresolvedEntry::Constant {
                    name,
                    namespace_context,
                    ..
                } = entry
                {
                    // Check if any added FQN would resolve this reference
                    // using Ruby's reverse namespace lookup
                    !Self::would_fqn_resolve_reference(name, namespace_context, &fqn_strings)
                } else {
                    true // Keep non-constant entries
                }
            });

            if entries.len() != original_len {
                affected_uris.insert(uri.clone());
            }
        }

        // Clean up empty entries
        self.unresolved_entries
            .retain(|_, entries| !entries.is_empty());

        affected_uris
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
        self.references.entry(fqn).or_default().push(location);
    }

    /// Remove all references for a URI
    pub fn remove_references_for_uri(&mut self, uri: &Url) {
        for refs in self.references.values_mut() {
            refs.retain(|loc| loc.uri != *uri);
        }
        self.references.retain(|_, refs| !refs.is_empty());
    }

    // ========================================================================
    // Unresolved Entries (Diagnostics)
    // ========================================================================

    /// Add an unresolved entry for a file
    pub fn add_unresolved_entry(&mut self, uri: Url, entry: UnresolvedEntry) {
        match &entry {
            UnresolvedEntry::Constant { name, .. } => {
                debug!("Adding unresolved constant: {} at {:?}", name, uri);
            }
            UnresolvedEntry::Method { name, receiver, .. } => {
                if let Some(recv) = receiver {
                    debug!("Adding unresolved method: {}.{} at {:?}", recv, name, uri);
                } else {
                    debug!("Adding unresolved method: {} at {:?}", name, uri);
                }
            }
        }

        // Avoid adding duplicate entries
        let entries = self.unresolved_entries.entry(uri).or_default();
        if !entries.contains(&entry) {
            entries.push(entry);
        }
    }

    /// Remove all unresolved entries for a file
    pub fn remove_unresolved_entries_for_uri(&mut self, uri: &Url) {
        self.unresolved_entries.remove(uri);
    }

    /// Get all unresolved entries for a file
    pub fn get_unresolved_entries(&self, uri: &Url) -> Vec<UnresolvedEntry> {
        self.unresolved_entries
            .get(uri)
            .cloned()
            .unwrap_or_default()
    }

    /// Get all unresolved constants for a file
    pub fn get_unresolved_constants(&self, uri: &Url) -> Vec<&UnresolvedEntry> {
        self.unresolved_entries
            .get(uri)
            .map(|entries| entries.iter().filter(|e| e.is_constant()).collect())
            .unwrap_or_default()
    }

    /// Get all unresolved methods for a file
    pub fn get_unresolved_methods(&self, uri: &Url) -> Vec<&UnresolvedEntry> {
        self.unresolved_entries
            .get(uri)
            .map(|entries| entries.iter().filter(|e| e.is_method()).collect())
            .unwrap_or_default()
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
        // Collect entries from definitions that belong to this URI
        // We use definitions (not file_entries) because mixins are mutated
        // on entries in definitions after they're added via get_mut()
        let entries_to_resolve: Vec<_> = self
            .definitions
            .values()
            .flatten()
            .filter(|e| e.location.uri == *uri)
            .cloned()
            .collect();

        for entry in entries_to_resolve {
            self.update_reverse_mixins(&entry);
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
