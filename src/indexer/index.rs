//! Ruby Index
//!
//! The central data structure for storing all indexed Ruby code information.
//! This includes definitions, references, method lookups, mixin relationships,
//! and prefix-based search capabilities.

use std::collections::{HashMap, HashSet};

use log::debug;
use slotmap::{new_key_type, SlotMap};
use tower_lsp::lsp_types::{Location, Url};

use crate::indexer::entry::{entry_kind::EntryKind, Entry, MixinType};
use crate::indexer::prefix_tree::PrefixTree;
use crate::types::compact_location::CompactLocation;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_method::RubyMethod;

// Re-export for backward compatibility
pub use crate::types::unresolved_index::{UnresolvedEntry, UnresolvedIndex};

// ============================================================================
// Types
// ============================================================================

// SlotMap key type for entries
new_key_type! { pub struct EntryId; }
new_key_type! { pub struct FileId; }
new_key_type! { pub struct FqnId; }

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

#[derive(Debug)]
pub struct RubyIndex {
    // Central Store - Single source of truth
    entries: SlotMap<EntryId, Entry>,

    // Indexes - Just IDs, no data duplication
    by_uri: HashMap<Url, Vec<EntryId>>,
    by_fqn: HashMap<FullyQualifiedName, Vec<EntryId>>,
    by_method_name: HashMap<RubyMethod, Vec<EntryId>>,

    // File storage with reverse lookup
    files: SlotMap<FileId, Url>,
    url_to_file_id: HashMap<Url, FileId>,

    // FQN storage with reverse lookup
    fqns: SlotMap<FqnId, FullyQualifiedName>,
    fqn_to_id: HashMap<FullyQualifiedName, FqnId>,

    // Mixin & Other Tracking
    pub reverse_mixins: HashMap<FullyQualifiedName, Vec<FullyQualifiedName>>,
    pub mixin_usages: HashMap<FullyQualifiedName, Vec<MixinUsage>>,
    mixin_usages_by_uri: HashMap<Url, HashSet<FullyQualifiedName>>,

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

            // File storage
            files: SlotMap::with_key(),
            url_to_file_id: HashMap::new(),

            // FQN storage
            fqns: SlotMap::with_key(),
            fqn_to_id: HashMap::new(),

            // Mixin tracking
            reverse_mixins: HashMap::new(),
            mixin_usages: HashMap::new(),
            mixin_usages_by_uri: HashMap::new(),
            prefix_tree: PrefixTree::new(),
            unresolved: UnresolvedIndex::new(),
        }
    }

    // ========================================================================
    // File ID Management
    // ========================================================================

    /// Get or insert a URL, returning its FileId
    pub fn get_or_insert_file(&mut self, url: &Url) -> FileId {
        if let Some(&file_id) = self.url_to_file_id.get(url) {
            return file_id;
        }
        let file_id = self.files.insert(url.clone());
        self.url_to_file_id.insert(url.clone(), file_id);
        file_id
    }

    /// Get URL for a FileId
    pub fn get_file_url(&self, file_id: FileId) -> Option<&Url> {
        self.files.get(file_id)
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
    // FQN Interning
    // ========================================================================

    /// Get or intern a FullyQualifiedName
    pub fn intern_fqn(&mut self, fqn: FullyQualifiedName) -> FqnId {
        if let Some(&id) = self.fqn_to_id.get(&fqn) {
            return id;
        }
        let id = self.fqns.insert(fqn.clone());
        self.fqn_to_id.insert(fqn, id);
        id
    }

    /// Get FQN for an ID
    pub fn get_fqn(&self, id: FqnId) -> Option<&FullyQualifiedName> {
        self.fqns.get(id)
    }

    /// Resolve FQN ID to owned FullyQualifiedName
    pub fn resolve_fqn(&self, id: FqnId) -> Option<FullyQualifiedName> {
        self.fqns.get(id).cloned()
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

        // Update mixin tracking
        self.update_reverse_mixins(&entry);

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

        // Clean up mixin tracking for completely removed FQNs
        for fqn in &removed_fqns {
            self.reverse_mixins.remove(fqn);
            self.mixin_usages.remove(fqn);
        }

        // Remove mixin usages
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
            let refs = self.get_references_iter(fqn);
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
        self.get_references_iter(fqn)
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
    // COMPATIBILITY METHODS (for callers that used old field access)
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

    /// Get entries for a file (compatibility method)
    pub fn file_entries(&self, uri: &Url) -> Vec<&Entry> {
        self.get_entries_for_uri(uri)
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

    /// Get methods by name (compatibility method)
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
    // NEW: SlotMap-based access methods
    // ========================================================================

    /// Get an entry by ID
    pub fn get_entry(&self, id: EntryId) -> Option<&Entry> {
        self.entries.get(id)
    }

    /// Get entries for a URI (using new SlotMap)
    pub fn get_entries_for_uri(&self, uri: &Url) -> Vec<&Entry> {
        self.by_uri
            .get(uri)
            .map(|ids| ids.iter().filter_map(|id| self.entries.get(*id)).collect())
            .unwrap_or_default()
    }

    /// Get entry IDs for a URI
    pub fn get_entry_ids_for_uri(&self, uri: &Url) -> Vec<EntryId> {
        self.by_uri.get(uri).cloned().unwrap_or_default()
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
        return_type: crate::type_inference::ruby_type::RubyType,
    ) -> bool {
        if let Some(entry) = self.entries.get_mut(entry_id) {
            if let EntryKind::Method(data) = &mut entry.kind {
                data.return_type = Some(return_type);
                return true;
            }
        }
        false
    }

    /// Get definitions by FQN (using new SlotMap, filters out Reference entries)
    pub fn get_definitions_iter(&self, fqn: &FullyQualifiedName) -> Vec<&Entry> {
        self.by_fqn
            .get(fqn)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.entries.get(*id))
                    .filter(|e| !matches!(e.kind, EntryKind::Reference))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get reference locations for FQN (using new SlotMap)
    pub fn get_references_iter(&self, fqn: &FullyQualifiedName) -> Vec<Location> {
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

    pub fn entries_len(&self) -> usize {
        self.entries.len()
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
    // Mixin Tracking
    // ========================================================================

    /// Update reverse mixin tracking when an entry with mixins is added
    pub fn update_reverse_mixins(&mut self, entry: &Entry) {
        use crate::indexer::ancestor_chain::resolve_mixin_ref;

        let (includes, extends, prepends) = match &entry.kind {
            EntryKind::Class(data) => (&data.includes, &data.extends, &data.prepends),
            EntryKind::Module(data) => (&data.includes, &data.extends, &data.prepends),
            _ => return,
        };

        let fqn = match self.get_fqn(entry.fqn_id).cloned() {
            Some(f) => f,
            None => return,
        };

        debug!("Updating reverse mixins for entry: {:?}", fqn);

        let mixin_groups = [
            (includes, MixinType::Include),
            (extends, MixinType::Extend),
            (prepends, MixinType::Prepend),
        ];

        for (mixin_refs, mixin_type) in mixin_groups {
            for mixin_ref in mixin_refs {
                let Some(resolved_fqn) = resolve_mixin_ref(self, mixin_ref, &fqn) else {
                    debug!("Failed to resolve mixin ref: {:?}", mixin_ref);
                    continue;
                };

                debug!("Resolved mixin ref {:?} to {:?}", mixin_ref, resolved_fqn);

                // Update reverse_mixins
                let including = self.reverse_mixins.entry(resolved_fqn.clone()).or_default();
                if !including.contains(&fqn) {
                    including.push(fqn.clone());
                }

                // Update mixin_usages - need to convert CompactLocation to Location
                if let Some(lsp_location) = self.to_lsp_location(&entry.location) {
                    let usage = MixinUsage {
                        user_fqn: fqn.clone(),
                        mixin_type,
                        location: lsp_location.clone(),
                    };

                    // Track which modules have usages from this URI (for fast removal)
                    self.mixin_usages_by_uri
                        .entry(lsp_location.uri.clone())
                        .or_default()
                        .insert(resolved_fqn.clone());

                    let usages = self.mixin_usages.entry(resolved_fqn).or_default();
                    if !usages.contains(&usage) {
                        usages.push(usage);
                    }
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

        let fqn = match self.get_fqn(entry.fqn_id).cloned() {
            Some(f) => f,
            None => return,
        };

        for mixin_ref in mixin_refs {
            let Some(resolved_fqn) = resolve_mixin_ref(self, mixin_ref, &fqn) else {
                debug!("Failed to resolve mixin ref: {:?}", mixin_ref);
                continue;
            };

            // Update reverse_mixins
            let including = self.reverse_mixins.entry(resolved_fqn.clone()).or_default();
            if !including.contains(&fqn) {
                including.push(fqn.clone());
            }

            // Update mixin_usages with actual call location
            let usage = MixinUsage {
                user_fqn: fqn.clone(),
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

        // Collect entries first to avoid borrow conflicts
        let entries: Vec<Entry> = self
            .definitions()
            .flat_map(|(_, entries)| entries.into_iter().cloned())
            .collect();

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
            .file_entries(uri)
            .iter()
            .filter_map(|e| self.get_fqn(e.fqn_id).cloned())
            .collect();

        self.resolve_mixins_for_fqns(&fqns_to_resolve);
    }

    /// Resolve mixin references for specific FQNs
    /// More efficient than iterating all definitions
    pub fn resolve_mixins_for_fqns(&mut self, fqns: &[FullyQualifiedName]) {
        // Collect entries first to avoid borrow conflicts
        let entries_to_update: Vec<Entry> = fqns
            .iter()
            .filter_map(|fqn| {
                self.get(fqn)
                    .and_then(|entries| entries.last().copied().cloned())
            })
            .collect();

        for entry in entries_to_update {
            self.update_reverse_mixins(&entry);
        }
    }

    /// Get all classes/modules that include the given module
    ///
    /// This method first checks the localized reverse_mixins index.
    /// If that is empty, it falls back to a full scan of all entries to find includers.
    /// This fallback is necessary because reverse_mixins are computed at indexing time,
    /// so if the module was indexed AFTER the including class, the link might be missing.
    pub fn get_including_classes(
        &self,
        module_fqn: &FullyQualifiedName,
    ) -> Vec<FullyQualifiedName> {
        let mut includers = self
            .reverse_mixins
            .get(module_fqn)
            .cloned()
            .unwrap_or_default();

        // If we found nothing (or even if we did?), we might want to scan if we suspect missing links.
        // For performance, let's only scan if we are in "desperate mode" or if we want high accuracy.
        // Given this is for Goto Definition (user interactive), a few ms scan is acceptable.
        // Let's ALWAYS scan if the result is empty? Or always merge?
        // To be safe against the race condition, we should really scan or maintain consistency.
        // Re-resolving 10k classes' mixins might be 50-100ms.
        // Let's try scanning only if the reverse index is empty first.
        // If the user has *some* includers but missing others, this optimization hurts.
        // But usually it's "all or nothing" for a specific file pair.
        // Let's merge both.

        let scanned = self.scan_for_including_classes(module_fqn);
        for fqn in scanned {
            if !includers.contains(&fqn) {
                includers.push(fqn);
            }
        }

        includers
    }

    /// Fallback: Scan all entries to find classes that include/extend/prepend the target module.
    /// This is slower (O(N)) but robust against indexing order.
    pub fn scan_for_including_classes(
        &self,
        target_module_fqn: &FullyQualifiedName,
    ) -> Vec<FullyQualifiedName> {
        use crate::indexer::ancestor_chain::resolve_mixin_ref;
        let mut found = Vec::new();

        // Iterate all definitions
        // We can optimize by using by_fqn and checking latest entry for each FQN
        for (fqn, entries) in &self.by_fqn {
            // Check all entries for this FQN, as some might be References or separate definitions
            // We iterate in reverse to find the most recent definition first (optimistic),
            // but we must skip References.
            for entry_id in entries.iter().rev() {
                let entry = match self.entries.get(*entry_id) {
                    Some(e) => e,
                    None => continue,
                };

                // Check if it's a class or module
                let (includes, extends, prepends) = match &entry.kind {
                    EntryKind::Class(data) => (&data.includes, &data.extends, &data.prepends),
                    EntryKind::Module(data) => (&data.includes, &data.extends, &data.prepends),
                    _ => continue,
                };

                let all_mixins = includes.iter().chain(extends.iter()).chain(prepends.iter());

                for mixin_ref in all_mixins {
                    // Try to resolve. Now that target_module_fqn is known (indexed), this should succeed.
                    // We only care if it resolves to target_module_fqn.

                    if let Some(resolved) = resolve_mixin_ref(self, mixin_ref, fqn) {
                        if resolved == *target_module_fqn {
                            found.push(fqn.clone());
                            break; // Found one usage, no need to check other mixins of this class
                        }
                    }
                }

                // If we processed a valid definition (Class/Module), should we stop for this FQN?
                // A class can be reopened. Only one of the definitions might have the include.
                // So we should probably continue to check other definitions of the same Class.
                // e.g.
                // class A; end
                // class A; include M; end
                // Both are Class entries.
            }
        }

        found
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
                if let Some(entries) = self.get(&usage.user_fqn) {
                    if let Some(entry) = entries.first() {
                        let mut current_path = path.clone();
                        current_path.push(module_fqn.clone());

                        match &entry.kind {
                            EntryKind::Class(_) => {
                                result
                                    .entry(usage.user_fqn.clone())
                                    .or_default()
                                    .push(current_path);
                            }
                            EntryKind::Module(_) => {
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
