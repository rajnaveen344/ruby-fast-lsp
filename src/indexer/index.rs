//! Ruby Index
//!
//! The central data structure for storing all indexed Ruby code information.
//! This includes definitions, references, method lookups, inheritance relationships,
//! and prefix-based search capabilities.

use std::collections::{HashMap, HashSet};

use log::debug;
use slotmap::{new_key_type, SlotMap};
use tower_lsp::lsp_types::{Location, Url};

use crate::analyzer_prism::utils;
use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::entry::{Entry, MixinRef, MixinType};
use crate::indexer::graph::Graph;
use crate::indexer::interner::Interner;
use crate::indexer::prefix_tree::PrefixTree;
use crate::types::compact_location::CompactLocation;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_method::RubyMethod;

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

// Re-export for backward compatibility
pub use crate::types::unresolved_index::{UnresolvedEntry, UnresolvedIndex};

// ============================================================================
// Types
// ============================================================================

// SlotMap key type for entries
new_key_type! { pub struct EntryId; }
new_key_type! { pub struct FileId; }
new_key_type! { pub struct FqnId; }

#[derive(Debug)]
pub struct RubyIndex {
    // Central Store - Single source of truth
    entries: SlotMap<EntryId, Entry>,

    // Indexes - Just IDs, no data duplication
    by_uri: HashMap<Url, Vec<EntryId>>,
    by_fqn: HashMap<FullyQualifiedName, Vec<EntryId>>,
    by_method_name: HashMap<RubyMethod, Vec<EntryId>>,

    // Interned storage for compact IDs
    files: Interner<FileId, Url>,
    fqns: Interner<FqnId, FullyQualifiedName>,

    // Inheritance graph for method resolution order
    pub graph: Graph,

    // Prefix tree for fast auto-completion lookups
    pub prefix_tree: PrefixTree,

    // Unresolved entries index
    pub unresolved: UnresolvedIndex,
}

impl RubyIndex {
    pub fn new() -> Self {
        RubyIndex {
            // Central store
            entries: SlotMap::with_key(),

            // Indexes
            by_uri: HashMap::new(),
            by_fqn: HashMap::new(),
            by_method_name: HashMap::new(),

            // Interned storage
            files: Interner::new(),
            fqns: Interner::new(),

            // Inheritance graph
            graph: Graph::new(),

            // Other indexes
            prefix_tree: PrefixTree::new(),
            unresolved: UnresolvedIndex::new(),
        }
    }

    // ========================================================================
    // File ID Management (delegates to Interner)
    // ========================================================================

    /// Get or insert a URL, returning its FileId
    pub fn get_or_insert_file(&mut self, url: &Url) -> FileId {
        self.files.get_or_insert(url)
    }

    /// Get URL for a FileId
    pub fn get_file_url(&self, file_id: FileId) -> Option<&Url> {
        self.files.get(file_id)
    }

    /// Get FileId for a URL (if it exists)
    pub fn get_file_id(&self, url: &Url) -> Option<FileId> {
        self.files.get_id(url).copied()
    }

    /// Convert CompactLocation to LSP Location
    pub fn to_lsp_location(&self, compact: &CompactLocation) -> Option<Location> {
        let url = self.get_file_url(compact.file_id)?;
        Some(Location {
            uri: url.clone(),
            range: compact.range,
        })
    }

    // ========================================================================
    // FQN Interning (delegates to Interner)
    // ========================================================================

    /// Get or intern a FullyQualifiedName
    pub fn intern_fqn(&mut self, fqn: FullyQualifiedName) -> FqnId {
        self.fqns.get_or_insert(&fqn)
    }

    /// Get FQN for an ID
    pub fn get_fqn(&self, id: FqnId) -> Option<&FullyQualifiedName> {
        self.fqns.get(id)
    }

    /// Get FqnId for a FullyQualifiedName
    pub fn get_fqn_id(&self, fqn: &FullyQualifiedName) -> Option<FqnId> {
        self.fqns.get_id(fqn).copied()
    }

    /// Resolve FQN ID to owned FullyQualifiedName
    pub fn resolve_fqn(&self, id: FqnId) -> Option<FullyQualifiedName> {
        self.fqns.get(id).cloned()
    }

    // ========================================================================
    // Ancestor Chain / Method Resolution Order
    // ========================================================================

    /// Builds the complete ancestor chain for a given class or module
    ///
    /// For class methods: includes singleton class + normal ancestor chain
    /// For instance methods: includes normal ancestor chain (current class -> mixins -> superclass)
    ///
    /// The chain represents the method lookup order in Ruby's method resolution.
    /// This delegates to the InheritanceGraph for efficient traversal.
    pub fn get_ancestor_chain(
        &self,
        fqn: &FullyQualifiedName,
        is_class_method: bool,
    ) -> Vec<FullyQualifiedName> {
        // Get the FqnId for this FQN
        let Some(fqn_id) = self.get_fqn_id(fqn) else {
            // If FQN not in index, return just itself
            return vec![fqn.clone()];
        };

        // Use the inheritance graph for traversal
        let fqn_ids = if is_class_method {
            self.graph.singleton_lookup_chain(fqn_id)
        } else {
            self.graph.method_lookup_chain(fqn_id)
        };

        // Convert FqnIds back to FullyQualifiedNames
        let mut chain: Vec<FullyQualifiedName> = fqn_ids
            .into_iter()
            .filter_map(|id| self.get_fqn(id).cloned())
            .collect();

        // Add root namespace as fallback for implicit superclass
        // This allows classes without explicit superclass to access top-level methods
        // (Ruby implicitly inherits from Object which can access top-level methods)
        // We add root even if it's not in the index, as methods may be defined at top-level
        let root = FullyQualifiedName::Constant(Vec::new());
        if !chain.contains(&root) && fqn.to_string() != "BasicObject" {
            // Only add root if we have a non-empty namespace (i.e., we're inside a class/module)
            if !fqn.namespace_parts().is_empty() {
                chain.push(root);
            }
        }

        chain
    }

    /// Get entry IDs for a given RubyMethod key
    pub fn get_method_ids(
        &self,
        method: &crate::types::ruby_method::RubyMethod,
    ) -> Option<&Vec<EntryId>> {
        self.by_method_name.get(method)
    }

    // ========================================================================
    // Entry Management
    // ========================================================================

    /// Add an entry to the index and return its ID
    pub fn add_entry(&mut self, entry: Entry) -> EntryId {
        // Look up URI from file_id for indexing
        let uri = self.get_file_url(entry.location.file_id).cloned();

        // Resolve FQN for indexing
        // Note: We use unwrap here because the FQN ID must have been interned by EntryBuilder
        let fqn = self
            .get_fqn(entry.fqn_id)
            .expect("FQN ID not found")
            .clone();

        // Insert into central store
        let id = self.entries.insert(entry.clone());

        // Add to indexes
        if let Some(uri) = uri {
            self.by_uri.entry(uri).or_default().push(id);
        }
        self.by_fqn.entry(fqn).or_default().push(id);

        // Add to method name index if it's a method
        if let EntryKind::Method(data) = &entry.kind {
            self.by_method_name.entry(data.name).or_default().push(id);
        }

        // Add to prefix tree
        self.add_to_prefix_tree(&entry);

        id
    }

    /// Remove all entries for a URI and return the FQNs that were completely removed
    /// (i.e., had no remaining definitions in other files)
    ///
    /// # Logic & Optimization
    /// 1. **Identify**: Look up `by_uri` to find all Entry IDs belonging to this file.
    /// 2. **Collect & Remove**: Iterate through these IDs and remove them from the central `SlotMap`.
    ///    - *Optimization*: We remove immediately to take ownership of the `Entry` struct.
    ///    - This allows us to reuse the `FullyQualifiedName` (string) without cloning it.
    /// 3. **Dedup**: Sort and deduplicate the collected FQNs to ensure we only process each unique FQN once.
    /// 4. **Cleanup Indexes**:
    ///    - For each unique FQN, update the secondary index `by_fqn` (maps FQN -> List of IDs).
    ///    - Remove the stale IDs we just deleted.
    ///    - If a FQN has no more entries (definitions) left system-wide, it counts as "completely removed".
    pub fn remove_entries_for_uri(&mut self, uri: &Url) -> Vec<FullyQualifiedName> {
        let ids_to_remove = self.by_uri.remove(uri).unwrap_or_default();

        // Use HashSet for O(1) amortized dedup
        let mut unique_fqns: HashSet<FullyQualifiedName> =
            HashSet::with_capacity(ids_to_remove.len() / 4);
        let mut unique_method_names: HashSet<RubyMethod> =
            HashSet::with_capacity(ids_to_remove.len() / 100);
        let mut removed_ids_set: HashSet<_> = HashSet::with_capacity(ids_to_remove.len());

        // 1. Remove entries and collect metadata
        for id in &ids_to_remove {
            removed_ids_set.insert(*id);
            if let Some(entry) = self.entries.remove(*id) {
                if let Some(fqn) = self.get_fqn(entry.fqn_id) {
                    unique_fqns.insert(fqn.clone());
                }
                if let EntryKind::Method(data) = entry.kind {
                    unique_method_names.insert(data.name);
                }
            }
        }

        // 2. Clean up by_fqn index
        for fqn in &unique_fqns {
            if let Some(ids) = self.by_fqn.get_mut(fqn) {
                ids.retain(|id| !removed_ids_set.contains(id));
                if ids.is_empty() {
                    self.by_fqn.remove(fqn);
                }
            }
        }

        // 3. Clean up by_method_name index
        for method_name in &unique_method_names {
            if let Some(ids) = self.by_method_name.get_mut(method_name) {
                ids.retain(|id| !removed_ids_set.contains(id));
                if ids.is_empty() {
                    self.by_method_name.remove(method_name);
                }
            }
        }

        // Compute removed FQNs (FQNs with no remaining entries)
        let removed_fqns: Vec<FullyQualifiedName> = unique_fqns
            .iter()
            .filter(|fqn| !self.by_fqn.contains_key(*fqn))
            .cloned()
            .collect();

        // Remove edges from the inheritance graph for this file
        if let Some(file_id) = self.files.get_id(uri).copied() {
            self.graph.remove_edges_from_file(file_id);
        }

        // Remove nodes from inheritance graph for completely removed FQNs
        for fqn in &removed_fqns {
            if let Some(fqn_id) = self.fqns.get_id(fqn).copied() {
                self.graph.remove_node(fqn_id);
            }
        }

        // Prefix tree cleanup: only remove keys if the FQN is completely gone
        for fqn in &removed_fqns {
            let key = fqn.name();
            if !key.is_empty() {
                self.prefix_tree.delete(&key);
            }
        }

        removed_fqns
    }

    /// Mark references to the given FQNs as unresolved in their respective files
    /// Returns the set of URIs that were affected (for diagnostic publishing)
    ///
    /// OPTIMIZED: Uses HashSet for batch deduplication instead of O(N) Vec::contains()
    /// Mark references to the given FQNs as unresolved in their respective files
    /// Returns the set of URIs that were affected (for diagnostic publishing)
    pub fn mark_references_as_unresolved(
        &mut self,
        removed_fqns: &[FullyQualifiedName],
    ) -> HashSet<Url> {
        let mut references_map: HashMap<FullyQualifiedName, Vec<Location>> = HashMap::new();
        for fqn in removed_fqns {
            let refs = self.references(fqn);
            if !refs.is_empty() {
                references_map.insert(fqn.clone(), refs);
            }
        }

        self.unresolved
            .mark_references_as_unresolved(removed_fqns, &references_map)
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

    /// Get entries by FQN (returns Vec for compatibility)
    /// Filters out references to return only definitions (legacy behavior)
    pub fn get(&self, fqn: &FullyQualifiedName) -> Option<Vec<&Entry>> {
        self.by_fqn
            .get(fqn)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.entries.get(*id))
                    // Only return definitions, not references
                    .filter(|e| !matches!(e.kind, EntryKind::Reference))
                    .collect()
            })
            .filter(|v: &Vec<&Entry>| !v.is_empty())
    }

    /// Get all references for a given FQN
    pub fn references(&self, fqn: &FullyQualifiedName) -> Vec<Location> {
        self.by_fqn
            .get(fqn)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.entries.get(*id))
                    .filter(|e| matches!(e.kind, EntryKind::Reference))
                    .filter_map(|e| self.to_lsp_location(&e.location))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Check if FQN has any definitions
    pub fn contains_fqn(&self, fqn: &FullyQualifiedName) -> bool {
        self.by_fqn
            .get(fqn)
            .map(|ids| {
                ids.iter().any(|id| {
                    self.entries
                        .get(*id)
                        .map_or(false, |e| !matches!(e.kind, EntryKind::Reference))
                })
            })
            .unwrap_or(false)
    }

    // ========================================================================
    // Query Methods - Definitions & Methods
    // ========================================================================

    /// Iterate over all definitions grouped by FQN
    pub fn definitions(&self) -> impl Iterator<Item = (&FullyQualifiedName, Vec<&Entry>)> {
        self.by_fqn.iter().filter_map(|(fqn, ids)| {
            let entries: Vec<&Entry> = ids
                .iter()
                .filter_map(|id| self.entries.get(*id))
                .filter(|e| !matches!(e.kind, EntryKind::Reference))
                .collect();
            if entries.is_empty() {
                None
            } else {
                Some((fqn, entries))
            }
        })
    }

    /// Get all entries for a file
    pub fn file_entries(&self, uri: &Url) -> Vec<&Entry> {
        self.by_uri
            .get(uri)
            .map(|ids| ids.iter().filter_map(|id| self.entries.get(*id)).collect())
            .unwrap_or_default()
    }

    /// Iterate over all methods grouped by name
    pub fn methods_by_name(&self) -> impl Iterator<Item = (&RubyMethod, Vec<&Entry>)> {
        self.by_method_name.iter().filter_map(|(method, ids)| {
            let entries: Vec<&Entry> = ids.iter().filter_map(|id| self.entries.get(*id)).collect();
            if entries.is_empty() {
                None
            } else {
                Some((method, entries))
            }
        })
    }

    /// Get methods by name
    pub fn get_methods_by_name(&self, method: &RubyMethod) -> Option<Vec<&Entry>> {
        let entries: Vec<&Entry> = self
            .by_method_name
            .get(method)?
            .iter()
            .filter_map(|id| self.entries.get(*id))
            .collect();
        if entries.is_empty() {
            None
        } else {
            Some(entries)
        }
    }

    /// Check if method exists in index
    pub fn contains_method(&self, method: &RubyMethod) -> bool {
        self.by_method_name
            .get(method)
            .map(|ids| !ids.is_empty())
            .unwrap_or(false)
    }

    /// Get number of unique FQNs in index
    pub fn definitions_len(&self) -> usize {
        self.by_fqn.len()
    }

    // ========================================================================
    // Entry Access by ID
    // ========================================================================

    /// Get an entry by ID
    pub fn get_entry(&self, id: EntryId) -> Option<&Entry> {
        self.entries.get(id)
    }

    /// Get entry IDs for a URI
    pub fn get_entry_ids_for_uri(&self, uri: &Url) -> Vec<EntryId> {
        self.by_uri.get(uri).cloned().unwrap_or_default()
    }

    /// Get entry IDs for an FQN
    pub fn get_entry_ids_for_fqn(&self, fqn: &FullyQualifiedName) -> Option<&Vec<EntryId>> {
        self.by_fqn.get(fqn)
    }

    /// Get mutable reference to the last definition entry (for updating mixins)
    pub fn get_last_definition_mut(&mut self, fqn: &FullyQualifiedName) -> Option<&mut Entry> {
        let id = *self.by_fqn.get(fqn)?.last()?;
        self.entries.get_mut(id)
    }

    /// Update the return type of a method entry
    pub fn update_method_return_type(
        &mut self,
        entry_id: EntryId,
        return_type: crate::inferrer::r#type::ruby::RubyType,
    ) -> bool {
        if let Some(entry) = self.entries.get_mut(entry_id) {
            if let EntryKind::Method(data) = &mut entry.kind {
                data.return_type = Some(return_type);
                return true;
            }
        }
        false
    }

    pub fn entries_len(&self) -> usize {
        self.entries.len()
    }

    /// Iterate over all entries in the index.
    pub fn all_entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.values()
    }

    /// Get all methods that need return type inference.
    /// Returns a list of (entry_id, file_id, line) for methods without return types.
    pub fn get_methods_needing_inference(&self) -> Vec<(EntryId, FileId, u32)> {
        self.entries
            .iter()
            .filter_map(|(entry_id, entry)| {
                if let EntryKind::Method(data) = &entry.kind {
                    // Only include methods without return types that have position info
                    if data.return_type.is_none() {
                        if let Some(pos) = data.return_type_position {
                            return Some((entry_id, entry.location.file_id, pos.line));
                        }
                    }
                }
                None
            })
            .collect()
    }

    /// Get the number of files indexed.
    pub fn files_count(&self) -> usize {
        self.files.len()
    }

    /// Iterate over all references grouped by FQN
    pub fn all_references(&self) -> HashMap<&FullyQualifiedName, Vec<Location>> {
        let mut refs: HashMap<&FullyQualifiedName, Vec<Location>> = HashMap::new();
        for (fqn, ids) in &self.by_fqn {
            let locations: Vec<Location> = ids
                .iter()
                .filter_map(|id| self.entries.get(*id))
                .filter(|e| matches!(e.kind, EntryKind::Reference))
                .filter_map(|e| self.to_lsp_location(&e.location))
                .collect();
            if !locations.is_empty() {
                refs.insert(fqn, locations);
            }
        }
        refs
    }

    /// Count entries by type
    pub fn count_entries_by_type(&self) -> HashMap<&'static str, usize> {
        let mut counts = HashMap::new();
        for entry in self.entries.values() {
            let type_name = match &entry.kind {
                EntryKind::Class(_) => "Class",
                EntryKind::Module(_) => "Module",
                EntryKind::Method(_) => "Method",
                EntryKind::Constant(_) => "Constant",
                EntryKind::LocalVariable(_) => "LocalVariable",
                EntryKind::InstanceVariable(_) => "InstanceVariable",
                EntryKind::ClassVariable(_) => "ClassVariable",
                EntryKind::GlobalVariable(_) => "GlobalVariable",
                EntryKind::Reference => "Reference",
            };
            *counts.entry(type_name).or_insert(0) += 1;
        }
        counts
    }

    // ========================================================================
    // Reference Management
    // ========================================================================

    /// Add a reference to a FQN
    pub fn add_reference(&mut self, fqn: FullyQualifiedName, location: Location) {
        // Convert Location to CompactLocation
        let file_id = self.get_or_insert_file(&location.uri);
        let compact_location =
            crate::types::compact_location::CompactLocation::new(file_id, location.range);

        let fqn_id = self.intern_fqn(fqn);
        let entry = Entry {
            fqn_id,
            location: compact_location,
            kind: EntryKind::new_reference(),
        };
        self.add_entry(entry);
    }

    /// Remove all references for a URI - Now handled by remove_entries_for_uri
    pub fn remove_references_for_uri(&mut self, _uri: &Url) {
        // No-op: References are now stored in the central SlotMap and removed automatically
        // by remove_entries_for_uri via the by_file index.
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
    // Inheritance Graph
    // ========================================================================

    /// Resolve all mixin references and build the inheritance graph
    /// (call after all definitions are indexed)
    pub fn resolve_all_mixins(&mut self) {
        use crate::indexer::graph::NodeKind;

        debug!("Building inheritance graph from indexed entries");

        // Collect entries with their FQNs and file IDs first to avoid borrow conflicts
        let entries_data: Vec<(FqnId, FileId, Entry)> = self
            .definitions()
            .filter_map(|(fqn, entries)| {
                let entry = (*entries.first()?).clone();
                let fqn_id = self.fqns.get_id(fqn).copied()?;
                let file_id = entry.location.file_id;
                Some((fqn_id, file_id, entry))
            })
            .collect();

        for (fqn_id, file_id, entry) in entries_data {
            let fqn = match self.get_fqn(fqn_id).cloned() {
                Some(f) => f,
                None => continue,
            };

            match &entry.kind {
                EntryKind::Class(data) => {
                    self.graph.ensure_node(fqn_id, NodeKind::Class);
                    self.resolve_and_add_edges(
                        fqn_id,
                        file_id,
                        &fqn,
                        &data.superclass,
                        &data.includes,
                        &data.prepends,
                        &data.extends,
                    );
                }
                EntryKind::Module(data) => {
                    self.graph.ensure_node(fqn_id, NodeKind::Module);
                    self.resolve_and_add_edges(
                        fqn_id,
                        file_id,
                        &fqn,
                        &None,
                        &data.includes,
                        &data.prepends,
                        &data.extends,
                    );
                }
                _ => {}
            }
        }

        debug!("Inheritance graph built successfully");
    }

    /// Helper to resolve mixin refs and add edges to the inheritance graph
    fn resolve_and_add_edges(
        &mut self,
        fqn_id: FqnId,
        file_id: FileId,
        context_fqn: &FullyQualifiedName,
        superclass: &Option<MixinRef>,
        includes: &[MixinRef],
        prepends: &[MixinRef],
        extends: &[MixinRef],
    ) {
        // Process superclass
        if let Some(superclass_ref) = superclass {
            if let Some(resolved_fqn) = utils::resolve_constant_fqn_from_parts(
                self,
                &superclass_ref.parts,
                superclass_ref.absolute,
                context_fqn,
            ) {
                if let Some(parent_id) = self.fqns.get_id(&resolved_fqn).copied() {
                    self.graph.set_superclass(fqn_id, parent_id, file_id);
                }
            }
        }

        // Process includes
        for mixin_ref in includes {
            if let Some(resolved_fqn) = utils::resolve_constant_fqn_from_parts(
                self,
                &mixin_ref.parts,
                mixin_ref.absolute,
                context_fqn,
            ) {
                if let Some(module_id) = self.fqns.get_id(&resolved_fqn).copied() {
                    self.graph.add_include(fqn_id, module_id, file_id);
                }
            }
        }

        // Process prepends
        for mixin_ref in prepends {
            if let Some(resolved_fqn) = utils::resolve_constant_fqn_from_parts(
                self,
                &mixin_ref.parts,
                mixin_ref.absolute,
                context_fqn,
            ) {
                if let Some(module_id) = self.fqns.get_id(&resolved_fqn).copied() {
                    self.graph.add_prepend(fqn_id, module_id, file_id);
                }
            }
        }

        // Process extends
        for mixin_ref in extends {
            if let Some(resolved_fqn) = utils::resolve_constant_fqn_from_parts(
                self,
                &mixin_ref.parts,
                mixin_ref.absolute,
                context_fqn,
            ) {
                if let Some(module_id) = self.fqns.get_id(&resolved_fqn).copied() {
                    self.graph.add_extend(fqn_id, module_id, file_id);
                }
            }
        }
    }

    /// Resolve mixin references only for entries in a specific file
    /// This is more efficient than resolve_all_mixins for incremental updates
    pub fn resolve_mixins_for_uri(&mut self, uri: &Url) {
        use crate::indexer::graph::NodeKind;

        let Some(file_id) = self.files.get_id(uri).copied() else {
            return;
        };

        // Collect entries from this file
        let entries_data: Vec<(FqnId, Entry)> = self
            .file_entries(uri)
            .into_iter()
            .filter_map(|entry| {
                // entry.fqn_id is already the correct FqnId
                Some((entry.fqn_id, (*entry).clone()))
            })
            .collect();

        for (fqn_id, entry) in entries_data {
            let fqn = match self.get_fqn(fqn_id).cloned() {
                Some(f) => f,
                None => continue,
            };

            match &entry.kind {
                EntryKind::Class(data) => {
                    self.graph.ensure_node(fqn_id, NodeKind::Class);
                    self.resolve_and_add_edges(
                        fqn_id,
                        file_id,
                        &fqn,
                        &data.superclass,
                        &data.includes,
                        &data.prepends,
                        &data.extends,
                    );
                }
                EntryKind::Module(data) => {
                    self.graph.ensure_node(fqn_id, NodeKind::Module);
                    self.resolve_and_add_edges(
                        fqn_id,
                        file_id,
                        &fqn,
                        &None,
                        &data.includes,
                        &data.prepends,
                        &data.extends,
                    );
                }
                _ => {}
            }
        }
    }

    /// Get all classes/modules that include the given module
    ///
    /// Uses the inheritance graph for efficient lookup, but falls back to scanning
    /// all entries if the graph doesn't have the information. This fallback is needed
    /// because edges are only added when `resolve_all_mixins` or `resolve_mixins_for_uri`
    /// is called, and during incremental indexing the module might be defined after
    /// the class that includes it.
    pub fn get_including_classes(
        &self,
        module_fqn: &FullyQualifiedName,
    ) -> Vec<FullyQualifiedName> {
        // First try the graph
        let mut includers: Vec<FullyQualifiedName> =
            if let Some(fqn_id) = self.fqns.get_id(module_fqn).copied() {
                self.graph
                    .mixers(fqn_id)
                    .iter()
                    .filter_map(|&id| self.get_fqn(id).cloned())
                    .collect()
            } else {
                Vec::new()
            };

        // Fall back to scanning if graph is empty
        // This handles the case where the module was indexed after the class
        if includers.is_empty() {
            for (fqn, entries) in &self.by_fqn {
                for entry_id in entries.iter().rev() {
                    let Some(entry) = self.entries.get(*entry_id) else {
                        continue;
                    };

                    let (includes, extends, prepends) = match &entry.kind {
                        EntryKind::Class(data) => (&data.includes, &data.extends, &data.prepends),
                        EntryKind::Module(data) => (&data.includes, &data.extends, &data.prepends),
                        _ => continue,
                    };

                    let all_mixins = includes.iter().chain(extends.iter()).chain(prepends.iter());

                    for mixin_ref in all_mixins {
                        if let Some(resolved) = utils::resolve_constant_fqn_from_parts(
                            self,
                            &mixin_ref.parts,
                            mixin_ref.absolute,
                            fqn,
                        ) {
                            if resolved == *module_fqn && !includers.contains(fqn) {
                                includers.push(fqn.clone());
                                break;
                            }
                        }
                    }
                }
            }
        }

        includers
    }

    /// Get class definition locations for classes that use a module (transitively)
    /// This includes:
    /// - Classes that directly include/prepend/extend the module
    /// - Classes that include/prepend/extend a module that includes this module (transitively)
    pub fn get_class_definition_locations(&self, module_fqn: &FullyQualifiedName) -> Vec<Location> {
        let transitive_classes = self.get_transitive_mixin_classes(module_fqn);
        transitive_classes
            .iter()
            .filter_map(|class_fqn| {
                self.get(class_fqn).and_then(|entries| {
                    entries.first().and_then(|entry| {
                        if matches!(entry.kind, EntryKind::Class(_)) {
                            self.to_lsp_location(&entry.location)
                        } else {
                            None
                        }
                    })
                })
            })
            .collect()
    }

    /// Get all classes that transitively include/extend/prepend a module
    /// This follows the chain: if A includes B and B includes C,
    /// and class D includes A, then D is a transitive user of C
    pub fn get_transitive_mixin_classes(
        &self,
        module_fqn: &FullyQualifiedName,
    ) -> Vec<FullyQualifiedName> {
        let mut result = Vec::new();
        let mut visited = std::collections::HashSet::new();
        self.collect_transitive_classes(module_fqn, &mut result, &mut visited);
        result
    }

    fn collect_transitive_classes(
        &self,
        module_fqn: &FullyQualifiedName,
        result: &mut Vec<FullyQualifiedName>,
        visited: &mut std::collections::HashSet<FullyQualifiedName>,
    ) {
        if !visited.insert(module_fqn.clone()) {
            return;
        }

        // Get direct mixers (classes and modules that include/prepend/extend this module)
        let mixers = self.get_including_classes(module_fqn);

        for mixer_fqn in mixers {
            if let Some(entries) = self.get(&mixer_fqn) {
                if let Some(entry) = entries.first() {
                    match &entry.kind {
                        EntryKind::Class(_) => {
                            // It's a class - add to result
                            if !result.contains(&mixer_fqn) {
                                result.push(mixer_fqn.clone());
                            }
                        }
                        EntryKind::Module(_) => {
                            // It's a module - recursively find classes that use this module
                            self.collect_transitive_classes(&mixer_fqn, result, visited);
                        }
                        _ => {}
                    }
                }
            }
        }

        visited.remove(module_fqn);
    }

    /// Get all mixin usages for a given module (for CodeLens)
    ///
    /// This retrieves usage information by:
    /// 1. Finding all classes/modules that use this module via the inheritance graph
    /// 2. Looking up the MixinRef locations stored in ClassData/ModuleData
    pub fn get_mixin_usages(&self, module_fqn: &FullyQualifiedName) -> Vec<MixinUsage> {
        let mut usages = Vec::new();

        // Get all classes/modules that include/prepend/extend this module
        let mixer_fqns = self.get_including_classes(module_fqn);

        for mixer_fqn in mixer_fqns {
            // Find entries for this mixer
            let Some(entries) = self.get(&mixer_fqn) else {
                continue;
            };

            for entry in entries {
                let mixin_refs_with_types: Vec<(&crate::indexer::entry::MixinRef, MixinType)> =
                    match &entry.kind {
                        EntryKind::Class(data) => {
                            let mut refs = Vec::new();
                            refs.extend(data.includes.iter().map(|r| (r, MixinType::Include)));
                            refs.extend(data.prepends.iter().map(|r| (r, MixinType::Prepend)));
                            refs.extend(data.extends.iter().map(|r| (r, MixinType::Extend)));
                            refs
                        }
                        EntryKind::Module(data) => {
                            let mut refs = Vec::new();
                            refs.extend(data.includes.iter().map(|r| (r, MixinType::Include)));
                            refs.extend(data.prepends.iter().map(|r| (r, MixinType::Prepend)));
                            refs.extend(data.extends.iter().map(|r| (r, MixinType::Extend)));
                            refs
                        }
                        _ => continue,
                    };

                for (mixin_ref, mixin_type) in mixin_refs_with_types {
                    // Check if this mixin_ref resolves to the target module
                    if let Some(resolved_fqn) = utils::resolve_constant_fqn_from_parts(
                        self,
                        &mixin_ref.parts,
                        mixin_ref.absolute,
                        &mixer_fqn,
                    ) {
                        if resolved_fqn == *module_fqn {
                            // Convert CompactLocation to Location
                            if let Some(location) = self.to_lsp_location(&mixin_ref.location) {
                                usages.push(MixinUsage {
                                    user_fqn: mixer_fqn.clone(),
                                    mixin_type,
                                    location,
                                });
                            }
                        }
                    }
                }
            }
        }

        usages
    }

    // ========================================================================
    // Prefix Tree (Auto-completion)
    // ========================================================================

    /// Search for entries by prefix
    pub fn search_by_prefix(&self, prefix: &str) -> Vec<Entry> {
        self.prefix_tree.search(prefix)
    }

    fn add_to_prefix_tree(&mut self, entry: &Entry) {
        // Do not index references in the prefix tree (too many, and not useful for completion)
        if matches!(entry.kind, EntryKind::Reference) {
            return;
        }

        // Get FQN from index
        if let Some(fqn) = self.get_fqn(entry.fqn_id) {
            let key = fqn.name();
            if !key.is_empty() {
                self.prefix_tree.insert(&key, entry.clone());
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

    fn create_test_entry(index: &mut RubyIndex, name: &str, uri: &Url) -> Entry {
        let fqn = FullyQualifiedName::from(vec![RubyConstant::try_from(name).unwrap()]);
        let mut entry = EntryBuilder::new()
            .fqn(fqn)
            .location(Location {
                uri: uri.clone(),
                range: Default::default(),
            })
            .kind(EntryKind::new_class(None))
            .build(index)
            .unwrap();

        // Register file and update entry
        let file_id = index.get_or_insert_file(uri);
        entry.location.file_id = file_id;
        entry
    }

    #[test]
    fn test_add_entry() {
        let mut index = RubyIndex::new();
        let uri = Url::parse("file://test.rb").unwrap();
        let entry = create_test_entry(&mut index, "Test", &uri);

        index.add_entry(entry);

        assert_eq!(index.definitions_len(), 1);
        assert_eq!(index.definitions_len(), 1);
    }

    #[test]
    fn test_remove_entries_for_uri() {
        let mut index = RubyIndex::new();
        let uri = Url::parse("file:///test.rb").unwrap();
        let entry = create_test_entry(&mut index, "Test", &uri);

        index.add_entry(entry);
        assert_eq!(index.definitions_len(), 1);

        index.remove_entries_for_uri(&uri);
        assert_eq!(index.definitions_len(), 0);
    }

    #[test]
    fn test_prefix_tree_search() {
        let mut index = RubyIndex::new();
        let uri = Url::parse("file://test.rb").unwrap();

        let entry1 = create_test_entry(&mut index, "TestClass", &uri);
        index.add_entry(entry1);
        let entry2 = create_test_entry(&mut index, "TestModule", &uri);
        index.add_entry(entry2);

        assert_eq!(index.search_by_prefix("Test").len(), 2);
        assert_eq!(index.search_by_prefix("TestC").len(), 1);
        assert_eq!(index.search_by_prefix("NonExistent").len(), 0);
    }
}
