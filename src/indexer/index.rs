use std::collections::HashMap;

use log::debug;
use tower_lsp::lsp_types::{Location, Url};

use crate::indexer::entry::{entry_kind::EntryKind, Entry};
use crate::indexer::prefix_tree::PrefixTree;
use crate::types::{fully_qualified_name::FullyQualifiedName, ruby_method::RubyMethod};

#[derive(Debug)]
pub struct RubyIndex {
    // File to entries map
    // Eg. file:///test.rb => [Entry1, Entry2, ...]
    pub file_entries: HashMap<Url, Vec<Entry>>,

    // Definitions are the definitions of a fully qualified name.
    // For example, if we have a method Foo#bar, its definition is the method definition.
    pub definitions: HashMap<FullyQualifiedName, Vec<Entry>>,

    // References are the references to a fully qualified name.
    pub references: HashMap<FullyQualifiedName, Vec<Location>>,

    // Temporarily used to find definitions by name until we have logic to determine the type of the receiver
    // For example, if we have a method Foo#bar, its method by name is bar.
    pub methods_by_name: HashMap<RubyMethod, Vec<Entry>>,

    // Reverse mixin tracking: module/class FQN -> list of classes/modules that include/extend/prepend it
    // For example, if class Foo includes module Bar, then reverse_mixins[Bar] contains Foo
    pub reverse_mixins: HashMap<FullyQualifiedName, Vec<FullyQualifiedName>>,

    // Prefix tree for auto-completion
    // Single prefix tree that holds all entries for fast lookup during completion
    pub prefix_tree: PrefixTree,
}

impl RubyIndex {
    pub fn new() -> Self {
        RubyIndex {
            file_entries: HashMap::new(),
            definitions: HashMap::new(),
            references: HashMap::new(),
            methods_by_name: HashMap::new(),
            reverse_mixins: HashMap::new(),
            prefix_tree: PrefixTree::new(),
        }
    }

    pub fn add_entry(&mut self, entry: Entry) {
        // Add to the file_entries map for this file
        let file_entries = self
            .file_entries
            .entry(entry.location.uri.clone())
            .or_insert_with(Vec::new);
        file_entries.push(entry.clone());

        // Add to the definitions map
        let definition_entries = self
            .definitions
            .entry(entry.fqn.clone())
            .or_insert_with(Vec::new);

        definition_entries.push(entry.clone());

        // Add to the methods_by_name map if the entry is of kind Method
        if let EntryKind::Method { name, .. } = &entry.kind {
            let method_entries = self
                .methods_by_name
                .entry(name.clone())
                .or_insert_with(Vec::new);
            method_entries.push(entry.clone());
        }

        // Update reverse mixin tracking for classes and modules with mixins
        self.update_reverse_mixins(&entry);

        // Add to prefix trees for auto-completion
        self.add_to_prefix_trees(&entry);
    }

    pub fn add_reference(&mut self, fully_qualified_name: FullyQualifiedName, location: Location) {
        debug!("Adding reference: {:?}", fully_qualified_name);

        self.references
            .entry(fully_qualified_name)
            .or_insert_with(Vec::new)
            .push(location);
    }

    pub fn remove_entries_for_uri(&mut self, uri: &Url) {
        let entries = match self.file_entries.remove(uri) {
            Some(entries) => entries,
            None => return, // No entries for this URI
        };

        for entry in &entries {
            if let Some(fqn_entries) = self.definitions.get_mut(&entry.fqn) {
                fqn_entries.retain(|e| e.location.uri != *uri);

                if fqn_entries.is_empty() {
                    self.definitions.remove(&entry.fqn);
                }
            }
        }

        let method_names: Vec<RubyMethod> = entries
            .iter()
            .filter_map(|entry| {
                if let EntryKind::Method { name, .. } = &entry.kind {
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

        // Remove entries from prefix trees
        self.remove_from_prefix_trees(&entries, uri);
    }

    pub fn get(&self, fqn: &FullyQualifiedName) -> Option<&Vec<Entry>> {
        self.definitions.get(fqn)
    }

    pub fn get_mut(&mut self, fqn: &FullyQualifiedName) -> Option<&mut Vec<Entry>> {
        self.definitions.get_mut(fqn)
    }

    pub fn remove_references_for_uri(&mut self, uri: &Url) {
        for refs in self.references.values_mut() {
            refs.retain(|loc| loc.uri != *uri);
        }

        self.references.retain(|_, refs| !refs.is_empty());
    }

    /// Update reverse mixin tracking when an entry with mixins is added
    pub fn update_reverse_mixins(&mut self, entry: &Entry) {
        use crate::indexer::entry::entry_kind::EntryKind;
        use crate::indexer::ancestor_chain::resolve_mixin_ref;

        match &entry.kind {
            EntryKind::Class { includes, extends, prepends, .. } | 
            EntryKind::Module { includes, extends, prepends, .. } => {
                debug!("Updating reverse mixins for entry: {:?}", entry.fqn);
                // Process includes, extends, and prepends
                for mixin_refs in [includes, extends, prepends] {
                    for mixin_ref in mixin_refs {
                        debug!("Processing mixin ref: {:?}", mixin_ref);
                        if let Some(resolved_fqn) = resolve_mixin_ref(self, mixin_ref, &entry.fqn) {
                            debug!("Resolved mixin ref {:?} to {:?}, adding reverse mapping: {:?} -> {:?}", 
                                   mixin_ref, resolved_fqn, resolved_fqn, entry.fqn);
                            let including_classes = self.reverse_mixins
                                .entry(resolved_fqn)
                                .or_insert_with(Vec::new);
                            
                            // Avoid duplicates
                            if !including_classes.contains(&entry.fqn) {
                                including_classes.push(entry.fqn.clone());
                            }
                        } else {
                            debug!("Failed to resolve mixin ref: {:?}", mixin_ref);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    /// Get all classes/modules that include the given module
    pub fn get_including_classes(&self, module_fqn: &FullyQualifiedName) -> Vec<FullyQualifiedName> {
        let result = self.reverse_mixins
            .get(module_fqn)
            .map(|classes| classes.clone())
            .unwrap_or_default();
        debug!("get_including_classes for {:?}: {:?}", module_fqn, result);
        result
    }

    /// Add entry to appropriate prefix trees for auto-completion
    fn add_to_prefix_trees(&mut self, entry: &Entry) {
        let key = entry.fqn.name();
        if key.is_empty() {
            return;
        }

        // Add all entry types to the single prefix tree
        self.prefix_tree.insert(&key, entry.clone());
    }

    /// Search for entries by prefix
    pub fn search_by_prefix(&self, prefix: &str) -> Vec<Entry> {
        self.prefix_tree.search(prefix)
    }

    /// Remove entries from prefix trees when a file is removed
    fn remove_from_prefix_trees(&mut self, entries: &[Entry], _uri: &Url) {
        for entry in entries {
            let key = entry.fqn.name();
            if !key.is_empty() {
                self.prefix_tree.delete(&key);
                debug!("Removed entry from prefix tree: {}", key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{indexer::entry::entry_builder::EntryBuilder, types::ruby_namespace::RubyConstant};

    use super::*;

    #[test]
    fn test_add_entry() {
        let mut index = RubyIndex::new();
        let uri = Url::parse("file://test.rb").unwrap();
        let fqn = FullyQualifiedName::from(vec![RubyConstant::try_from("Test").unwrap()]);
        let entry = EntryBuilder::new()
            .fqn(fqn)
            .location(Location {
                uri: uri.clone(),
                range: Default::default(),
            })
            .kind(EntryKind::new_class(None))
            .build()
            .unwrap();
        index.add_entry(entry);
        assert_eq!(index.definitions.len(), 1);
        assert_eq!(index.references.len(), 0);
    }

    #[test]
    fn test_remove_entries_for_uri() {
        let mut index = RubyIndex::new();
        let uri = Url::parse("file:///test.rb").unwrap();

        let fqn = FullyQualifiedName::from(vec![RubyConstant::try_from("Test").unwrap()]);
        let entry = EntryBuilder::new()
            .fqn(fqn)
            .location(Location {
                uri: uri.clone(),
                range: Default::default(),
            })
            .kind(EntryKind::new_class(None))
            .build()
            .unwrap();
        index.add_entry(entry);
        assert_eq!(index.definitions.len(), 1);
        assert_eq!(index.references.len(), 0);
        index.remove_entries_for_uri(&uri);
        assert_eq!(index.definitions.len(), 0);
        assert_eq!(index.references.len(), 0);
    }

    #[test]
    fn test_prefix_tree_search() {
        let mut index = RubyIndex::new();
        let uri = Url::parse("file://test.rb").unwrap();

        // Add a class
        let fqn1 = FullyQualifiedName::from(vec![RubyConstant::try_from("TestClass").unwrap()]);
        let entry1 = EntryBuilder::new()
            .fqn(fqn1)
            .location(Location {
                uri: uri.clone(),
                range: Default::default(),
            })
            .kind(EntryKind::new_class(None))
            .build()
            .unwrap();
        index.add_entry(entry1);

        // Add a module
        let fqn2 = FullyQualifiedName::from(vec![RubyConstant::try_from("TestModule").unwrap()]);
        let entry2 = EntryBuilder::new()
            .fqn(fqn2)
            .location(Location {
                uri: uri.clone(),
                range: Default::default(),
            })
            .kind(EntryKind::new_module())
            .build()
            .unwrap();
        index.add_entry(entry2);

        // Test prefix search
        let results = index.search_by_prefix("Test");
        assert_eq!(results.len(), 2);

        let results = index.search_by_prefix("TestC");
        assert_eq!(results.len(), 1);

        let results = index.search_by_prefix("TestM");
        assert_eq!(results.len(), 1);

        let results = index.search_by_prefix("NonExistent");
        assert_eq!(results.len(), 0);
    }

    #[test]
    fn test_prefix_tree_all_entry_types() {
        let mut index = RubyIndex::new();
        let uri = Url::parse("file://test.rb").unwrap();

        // Add a constant
        let fqn1 = FullyQualifiedName::from(vec![RubyConstant::try_from("MY_CONSTANT").unwrap()]);
        let entry1 = EntryBuilder::new()
            .fqn(fqn1)
            .location(Location {
                uri: uri.clone(),
                range: Default::default(),
            })
            .kind(EntryKind::Constant {
                value: Some("42".to_string()),
                visibility: None,
            })
            .build()
            .unwrap();
        index.add_entry(entry1);

        // Add a class
        let fqn2 = FullyQualifiedName::from(vec![RubyConstant::try_from("MyClass").unwrap()]);
        let entry2 = EntryBuilder::new()
            .fqn(fqn2)
            .location(Location {
                uri: uri.clone(),
                range: Default::default(),
            })
            .kind(EntryKind::new_class(None))
            .build()
            .unwrap();
        index.add_entry(entry2);

        // Add a module
        let fqn3 = FullyQualifiedName::from(vec![RubyConstant::try_from("MyModule").unwrap()]);
        let entry3 = EntryBuilder::new()
            .fqn(fqn3)
            .location(Location {
                uri: uri.clone(),
                range: Default::default(),
            })
            .kind(EntryKind::new_module())
            .build()
            .unwrap();
        index.add_entry(entry3);

        // Test searching for constants
        let constant_results = index.search_by_prefix("MY_");
        assert_eq!(constant_results.len(), 1);
        
        // Test searching for classes and modules
        let my_results = index.search_by_prefix("My");
        assert_eq!(my_results.len(), 2); // MyClass and MyModule
        
        // Test searching with no matches
        let no_results = index.search_by_prefix("xyz");
        assert_eq!(no_results.len(), 0);
    }
}
