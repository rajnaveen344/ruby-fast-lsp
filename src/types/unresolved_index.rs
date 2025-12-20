//! Unresolved Index
//!
//! Manages unresolved references (constants, methods) for diagnostics.
//! Encapsulates the forward map (URI -> entries) and reverse lookup (name -> URIs)
//! to ensure they stay in sync atomically.

use std::collections::{HashMap, HashSet};

use tower_lsp::lsp_types::{Location, Url};

use crate::types::fully_qualified_name::FullyQualifiedName;

use crate::type_inference::ruby_type::RubyType;

// ============================================================================
// UnresolvedEntry
// ============================================================================

/// Represents an unresolved reference for diagnostics.
/// Used to report missing constants/classes/modules/methods.
#[derive(Debug, Clone, PartialEq, Eq)]
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
        /// The receiver type if known
        /// None for method calls without explicit receiver (implicit self)
        /// Some(RubyType::Unknown) means explicit receiver with unknown type
        receiver_type: Option<RubyType>,
        /// Location where the method was called
        location: Location,
    },
}

// Manual Hash implementation since Location from tower_lsp doesn't implement Hash
impl std::hash::Hash for UnresolvedEntry {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            UnresolvedEntry::Constant {
                name,
                namespace_context,
                location,
            } => {
                0u8.hash(state); // discriminant
                name.hash(state);
                namespace_context.hash(state);
                location.uri.hash(state);
                location.range.start.line.hash(state);
                location.range.start.character.hash(state);
                location.range.end.line.hash(state);
                location.range.end.character.hash(state);
            }
            UnresolvedEntry::Method {
                name,
                receiver_type,
                location,
            } => {
                1u8.hash(state); // discriminant
                name.hash(state);
                receiver_type.hash(state);
                location.uri.hash(state);
                location.range.start.line.hash(state);
                location.range.start.character.hash(state);
                location.range.end.line.hash(state);
                location.range.end.character.hash(state);
            }
        }
    }
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
    pub fn method(name: String, receiver_type: Option<RubyType>, location: Location) -> Self {
        Self::Method {
            name,
            receiver_type,
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

    /// Get the name of this entry (constant name or method name)
    pub fn name(&self) -> &str {
        match self {
            Self::Constant { name, .. } => name,
            Self::Method { name, .. } => name,
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
// UnresolvedIndex
// ============================================================================

/// Manages unresolved references with forward and reverse lookups.
/// Keeps both maps in sync atomically.
#[derive(Debug, Default)]
pub struct UnresolvedIndex {
    /// Forward map: URI -> list of unresolved entries in that file
    entries: HashMap<Url, Vec<UnresolvedEntry>>,

    /// Reverse lookup: constant name -> set of URIs with unresolved refs to that name
    /// Used for O(1) lookup during clear_resolved_entries
    by_name: HashMap<String, HashSet<Url>>,
}

impl UnresolvedIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an unresolved entry for a file
    /// Maintains both forward and reverse indexes atomically
    pub fn add(&mut self, uri: Url, entry: UnresolvedEntry) {
        // Update reverse index for constants
        if let UnresolvedEntry::Constant { ref name, .. } = entry {
            self.by_name
                .entry(name.clone())
                .or_default()
                .insert(uri.clone());
        }

        // Avoid adding duplicate entries
        let entries = self.entries.entry(uri).or_default();
        if !entries.contains(&entry) {
            entries.push(entry);
        }
    }

    /// Remove all unresolved entries for a file
    /// Cleans up both forward and reverse indexes atomically
    pub fn remove_for_uri(&mut self, uri: &Url) {
        // Clean up reverse index first
        if let Some(entries) = self.entries.get(uri) {
            for entry in entries {
                if let UnresolvedEntry::Constant { name, .. } = entry {
                    if let Some(uris) = self.by_name.get_mut(name) {
                        uris.remove(uri);
                        if uris.is_empty() {
                            self.by_name.remove(name);
                        }
                    }
                }
            }
        }
        self.entries.remove(uri);
    }

    /// Get all unresolved entries for a file
    pub fn get(&self, uri: &Url) -> Vec<UnresolvedEntry> {
        self.entries.get(uri).cloned().unwrap_or_default()
    }

    /// Get the number of files with unresolved entries
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Check if there are no unresolved entries
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Get all URIs with unresolved entries
    pub fn uris(&self) -> Vec<Url> {
        self.entries.keys().cloned().collect()
    }

    /// Get URIs that have unresolved references to the given name
    /// Used for O(1) lookup during clear_resolved_entries
    pub fn get_uris_for_name(&self, name: &str) -> Option<&HashSet<Url>> {
        self.by_name.get(name)
    }

    /// Get mutable access to entries for a URI (for retain operations)
    pub fn get_entries_mut(&mut self, uri: &Url) -> Option<&mut Vec<UnresolvedEntry>> {
        self.entries.get_mut(uri)
    }

    /// Get mutable iterator over all entries (for retain operations)
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&Url, &mut Vec<UnresolvedEntry>)> {
        self.entries.iter_mut()
    }

    /// Retain entries based on predicate, cleaning up reverse index
    pub fn retain<F>(&mut self, mut predicate: F)
    where
        F: FnMut(&Url, &Vec<UnresolvedEntry>) -> bool,
    {
        self.entries.retain(|uri, entries| {
            let keep = predicate(uri, entries);
            if !keep {
                // Clean up reverse index for removed entries
                for entry in entries.iter() {
                    if let UnresolvedEntry::Constant { name, .. } = entry {
                        if let Some(uris) = self.by_name.get_mut(name) {
                            uris.remove(uri);
                        }
                    }
                }
            }
            keep
        });
        // Clean up empty by_name entries
        self.by_name.retain(|_, uris| !uris.is_empty());
    }

    /// Mark references to removed FQNs as unresolved
    /// Returns URIs that were affected
    ///
    /// OPTIMIZED: Uses HashSet for batch deduplication instead of O(N) Vec::contains()
    pub fn mark_references_as_unresolved(
        &mut self,
        removed_fqns: &[FullyQualifiedName],
        references: &HashMap<FullyQualifiedName, Vec<Location>>,
    ) -> HashSet<Url> {
        let mut affected_uris = HashSet::new();

        // Collect all new unresolved entries per URI using HashSet for O(1) dedup
        let mut new_entries_by_uri: HashMap<Url, HashSet<UnresolvedEntry>> = HashMap::new();

        for fqn in removed_fqns {
            if let Some(ref_locations) = references.get(fqn) {
                for location in ref_locations {
                    affected_uris.insert(location.uri.clone());

                    // Add to batch (HashSet handles dedup automatically)
                    let unresolved = UnresolvedEntry::constant(fqn.to_string(), location.clone());
                    new_entries_by_uri
                        .entry(location.uri.clone())
                        .or_default()
                        .insert(unresolved);
                }
            }
        }

        // Now merge with existing entries - only add truly new ones
        for (uri, new_entries) in new_entries_by_uri {
            let existing = self.entries.entry(uri.clone()).or_default();

            // Build a HashSet of existing entries for O(1) lookup
            let existing_set: HashSet<_> = existing.iter().cloned().collect();

            for entry in new_entries {
                if !existing_set.contains(&entry) {
                    // Also update the reverse index
                    if let UnresolvedEntry::Constant { ref name, .. } = entry {
                        self.by_name
                            .entry(name.clone())
                            .or_default()
                            .insert(uri.clone());
                    }
                    existing.push(entry);
                }
            }
        }

        affected_uris
    }

    /// Clear resolved entries that match the given FQNs
    /// Returns URIs that were affected
    ///
    /// OPTIMIZED: Uses reverse lookup to only check affected files
    pub fn clear_resolved(
        &mut self,
        added_fqns: &[FullyQualifiedName],
        would_resolve: impl Fn(&str, &[String], &HashSet<String>) -> bool,
    ) -> HashSet<Url> {
        let mut affected_uris = HashSet::new();

        // Build a set of all FQN strings for quick lookup
        let fqn_strings: HashSet<String> = added_fqns.iter().map(|fqn| fqn.to_string()).collect();

        // Extract just the final name component from each FQN for reverse lookup
        let fqn_final_names: HashSet<String> = added_fqns
            .iter()
            .map(|fqn| fqn.name())
            .filter(|name| !name.is_empty())
            .collect();

        // Use reverse index to find only files that might be affected
        let mut potentially_affected_uris = HashSet::new();
        for name in &fqn_final_names {
            if let Some(uris) = self.by_name.get(name) {
                potentially_affected_uris.extend(uris.iter().cloned());
            }
        }
        // Also check full FQN strings (for qualified references like "Foo::Bar")
        for fqn_str in &fqn_strings {
            if let Some(uris) = self.by_name.get(fqn_str) {
                potentially_affected_uris.extend(uris.iter().cloned());
            }
        }

        // Only iterate over potentially affected files
        for uri in potentially_affected_uris {
            if let Some(entries) = self.entries.get_mut(&uri) {
                let original_len = entries.len();

                entries.retain(|entry| {
                    if let UnresolvedEntry::Constant {
                        name,
                        namespace_context,
                        ..
                    } = entry
                    {
                        // Check if any added FQN would resolve this reference
                        !would_resolve(name, namespace_context, &fqn_strings)
                    } else {
                        true // Keep non-constant entries
                    }
                });

                if entries.len() != original_len {
                    affected_uris.insert(uri.clone());
                }
            }
        }

        // Clean up empty entries and reverse index
        let mut names_to_clean = Vec::new();
        self.entries.retain(|uri, entries| {
            if entries.is_empty() {
                // Collect names that need cleanup from reverse index
                for name in fqn_final_names.iter() {
                    if let Some(uris) = self.by_name.get_mut(name) {
                        uris.remove(uri);
                        if uris.is_empty() {
                            names_to_clean.push(name.clone());
                        }
                    }
                }
                false
            } else {
                true
            }
        });
        for name in names_to_clean {
            self.by_name.remove(&name);
        }

        affected_uris
    }
}
