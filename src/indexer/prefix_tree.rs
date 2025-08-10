use std::collections::HashMap;
use std::cell::RefCell;
use trie_rs::map::{Trie, TrieBuilder};

use crate::indexer::entry::Entry;

/// A PrefixTree is a data structure that allows searching for partial strings fast. The tree is similar to a nested
/// hash structure, where the keys are the characters of the inserted strings.
///
/// ## Example
/// ```rust
/// let mut tree = PrefixTree::new();
/// // Insert entries using the same key and value
/// tree.insert("bar", entry1);
/// tree.insert("baz", entry2);
/// // When we search it, it finds all possible values based on partial (or complete matches):
/// tree.search(""); // => [entry1, entry2]
/// tree.search("b"); // => [entry1, entry2]
/// tree.search("ba"); // => [entry1, entry2]
/// tree.search("bar"); // => [entry1]
/// ```
///
/// A PrefixTree is useful for autocomplete, since we always want to find all alternatives while the developer hasn't
/// finished typing yet. This PrefixTree implementation allows for string keys and Entry values.
///
/// This implementation uses the `trie-rs` crate for efficient memory usage and fast lookups.
/// See https://en.wikipedia.org/wiki/Trie for more information
#[derive(Debug)]
pub struct PrefixTree {
    // We maintain both a built trie for fast searches and a map for modifications
    trie: RefCell<Option<Trie<u8, Entry>>>,
    entries: RefCell<HashMap<String, Entry>>,
    needs_rebuild: RefCell<bool>,
}

impl PrefixTree {
    pub fn new() -> Self {
        PrefixTree {
            trie: RefCell::new(None),
            entries: RefCell::new(HashMap::new()),
            needs_rebuild: RefCell::new(false),
        }
    }

    /// Search the PrefixTree based on a given `prefix`. If `foo` is an entry in the tree, then searching for `fo` will
    /// return it as a result. The result is always an array of Entry references.
    pub fn search(&self, prefix: &str) -> Vec<Entry> {
        // Ensure trie is built
        self.ensure_trie_built();
        
        if prefix.is_empty() {
            // Return all entries for empty prefix
            self.entries.borrow().values().cloned().collect()
        } else {
            // Use trie's predictive search to find all entries with the given prefix
            let trie_ref = self.trie.borrow();
            let trie = trie_ref.as_ref().unwrap();
            let results: Vec<(String, &Entry)> = trie.predictive_search(prefix).collect();
            results.into_iter().map(|(_, entry)| entry.clone()).collect()
        }
    }

    /// Inserts an `entry` using the given `key`
    pub fn insert(&mut self, key: &str, entry: Entry) {
        // This line is to allow a value to be overridden. When we are indexing files, we want to be able to update entries
        // for a given fully qualified name if we find more occurrences of it. Without being able to override, that would
        // not be possible
        self.entries.borrow_mut().insert(key.to_string(), entry);
        *self.needs_rebuild.borrow_mut() = true;
    }

    /// Deletes the entry identified by `key` from the tree. Notice that a partial match will still delete all entries
    /// that match it. For example, if the tree contains `foo` and we ask to delete `fo`, then `foo` will be deleted
    pub fn delete(&mut self, key: &str) {
        if self.entries.borrow_mut().remove(key).is_some() {
            *self.needs_rebuild.borrow_mut() = true;
        }
    }

    /// Ensure the trie is built if it needs rebuilding
    fn ensure_trie_built(&self) {
        let needs_rebuild = *self.needs_rebuild.borrow();
        let trie_is_none = self.trie.borrow().is_none();
        
        if needs_rebuild || trie_is_none {
            self.rebuild_trie();
        }
    }

    /// Rebuild the trie from the current entries
    fn rebuild_trie(&self) {
        let mut builder = TrieBuilder::new();
        
        for (key, entry) in self.entries.borrow().iter() {
            builder.push(key, entry.clone());
        }
        
        *self.trie.borrow_mut() = Some(builder.build());
        *self.needs_rebuild.borrow_mut() = false;
    }
}

impl Default for PrefixTree {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::entry::{entry_builder::EntryBuilder, entry_kind::EntryKind};
    use crate::types::{fully_qualified_name::FullyQualifiedName, ruby_namespace::RubyConstant};
    use tower_lsp::lsp_types::{Location, Url};

    #[test]
    fn test_prefix_tree_insert_and_search() {
        let mut tree = PrefixTree::new();
        let uri = Url::parse("file://test.rb").unwrap();

        // Create test entries
        let fqn1 = FullyQualifiedName::from(vec![RubyConstant::try_from("Foo").unwrap()]);
        let entry1 = EntryBuilder::new()
            .fqn(fqn1)
            .location(Location {
                uri: uri.clone(),
                range: Default::default(),
            })
            .kind(EntryKind::new_class(None))
            .build()
            .unwrap();

        let fqn2 = FullyQualifiedName::from(vec![RubyConstant::try_from("FooBar").unwrap()]);
        let entry2 = EntryBuilder::new()
            .fqn(fqn2)
            .location(Location {
                uri: uri.clone(),
                range: Default::default(),
            })
            .kind(EntryKind::new_class(None))
            .build()
            .unwrap();

        // Insert entries
        tree.insert("Foo", entry1);
        tree.insert("FooBar", entry2);

        // Test searches
        let results = tree.search("");
        assert_eq!(results.len(), 2);

        let results = tree.search("F");
        assert_eq!(results.len(), 2);

        let results = tree.search("Foo");
        assert_eq!(results.len(), 2); // Both "Foo" and "FooBar" match

        let results = tree.search("FooB");
        assert_eq!(results.len(), 1); // Only "FooBar" matches

        let results = tree.search("FooBar");
        assert_eq!(results.len(), 1); // Only "FooBar" matches

        let results = tree.search("Baz");
        assert_eq!(results.len(), 0); // No matches
    }

    #[test]
    fn test_prefix_tree_delete() {
        let mut tree = PrefixTree::new();
        let uri = Url::parse("file://test.rb").unwrap();

        let fqn = FullyQualifiedName::from(vec![RubyConstant::try_from("Foo").unwrap()]);
        let entry = EntryBuilder::new()
            .fqn(fqn)
            .location(Location {
                uri: uri.clone(),
                range: Default::default(),
            })
            .kind(EntryKind::new_class(None))
            .build()
            .unwrap();

        tree.insert("Foo", entry);
        assert_eq!(tree.search("Foo").len(), 1);

        tree.delete("Foo");
        assert_eq!(tree.search("Foo").len(), 0);
    }
}