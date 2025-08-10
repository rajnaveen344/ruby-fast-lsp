use std::collections::HashMap;

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
/// See https://en.wikipedia.org/wiki/Trie for more information
#[derive(Debug)]
pub struct PrefixTree {
    root: Node,
}



impl PrefixTree {
    pub fn new() -> Self {
        PrefixTree {
            root: Node::new(None),
        }
    }

    /// Search the PrefixTree based on a given `prefix`. If `foo` is an entry in the tree, then searching for `fo` will
    /// return it as a result. The result is always an array of Entry references.
    pub fn search(&self, prefix: &str) -> Vec<&Entry> {
        if let Some(node) = self.find_node(prefix) {
            node.collect()
        } else {
            Vec::new()
        }
    }

    /// Inserts an `entry` using the given `key`
    pub fn insert(&mut self, key: &str, entry: Entry) {
        let mut node = &mut self.root;

        for char in key.chars() {
            let char_str = char.to_string();
            if !node.children.contains_key(&char_str) {
                let new_node = Node::new(None);
                node.children.insert(char_str.clone(), new_node);
            }
            node = node.children.get_mut(&char_str).unwrap();
        }

        // This line is to allow a value to be overridden. When we are indexing files, we want to be able to update entries
        // for a given fully qualified name if we find more occurrences of it. Without being able to override, that would
        // not be possible
        node.value = Some(entry);
        node.is_leaf = true;
    }

    /// Deletes the entry identified by `key` from the tree. Notice that a partial match will still delete all entries
    /// that match it. For example, if the tree contains `foo` and we ask to delete `fo`, then `foo` will be deleted
    pub fn delete(&mut self, key: &str) {
        if let Some(node) = self.find_node_mut(key) {
            node.value = None;
            node.is_leaf = false;

            // TODO: Implement cleanup of empty parent nodes
            // This is more complex in Rust due to ownership, so we'll keep it simple for now
        }
    }

    /// Find a node that matches the given `key`
    fn find_node(&self, key: &str) -> Option<&Node> {
        let mut node = &self.root;

        for char in key.chars() {
            let char_str = char.to_string();
            if let Some(child_node) = node.children.get(&char_str) {
                node = child_node;
            } else {
                return None;
            }
        }

        Some(node)
    }

    /// Find a mutable node that matches the given `key`
    fn find_node_mut(&mut self, key: &str) -> Option<&mut Node> {
        let mut node = &mut self.root;

        for char in key.chars() {
            let char_str = char.to_string();
            if node.children.contains_key(&char_str) {
                node = node.children.get_mut(&char_str).unwrap();
            } else {
                return None;
            }
        }

        Some(node)
    }
}

impl Default for PrefixTree {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
struct Node {
    value: Option<Entry>,
    children: HashMap<String, Node>,
    is_leaf: bool,
}

impl Node {
    fn new(value: Option<Entry>) -> Self {
        Node {
            value,
            children: HashMap::new(),
            is_leaf: false,
        }
    }

    /// Collect all entries in this subtree using depth-first traversal
    fn collect(&self) -> Vec<&Entry> {
        let mut result = Vec::new();
        let mut stack = vec![self];

        while let Some(node) = stack.pop() {
            if node.is_leaf {
                if let Some(ref entry) = node.value {
                    result.push(entry);
                }
            }

            // Add children to stack for traversal
            for child in node.children.values() {
                stack.push(child);
            }
        }

        result
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