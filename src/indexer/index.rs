use std::collections::HashMap;

use lsp_types::{Location, Url};

use super::{
    entry::{Entry, EntryType},
    types::{constant::Constant, fully_qualified_constant::FullyQualifiedName, method::Method},
};

#[derive(Debug)]
pub struct RubyIndex {
    pub file_entries: HashMap<Url, Vec<Entry>>,

    // Namespace ancestors are the ancestors of a namespace.
    // For example, if we have a namespace Foo::Bar, its ancestors are [Foo].
    // Eg. Foo::Bar.ancestors = [Foo], Foo.ancestors = [], Foo::Bar::Baz.ancestors = [Foo::Bar, Foo]
    pub namespace_ancestors: HashMap<Constant, Vec<Constant>>,

    // Definitions are the definitions of a fully qualified name.
    // For example, if we have a method Foo#bar, its definition is the method definition.
    pub definitions: HashMap<FullyQualifiedName, Vec<Entry>>,

    // References are the references to a fully qualified name.
    pub references: HashMap<FullyQualifiedName, Vec<Location>>,
}

impl RubyIndex {
    pub fn new() -> Self {
        RubyIndex {
            file_entries: HashMap::new(),
            namespace_ancestors: HashMap::new(),
            definitions: HashMap::new(),
            references: HashMap::new(),
        }
    }

    pub fn add_entry(&mut self, entry: Entry) {
        // Add to the definitions map
        let entries = self
            .definitions
            .entry(entry.fully_qualified_name.clone())
            .or_insert_with(Vec::new);
        entries.push(entry.clone());

        // Add to the file_entries map for this file
        let file_entries = self
            .file_entries
            .entry(entry.location.uri.clone())
            .or_insert_with(Vec::new);
        file_entries.push(entry);
    }

    pub fn remove_entries_for_uri(&mut self, uri: &Url) {
        // If no entries for this URI, return early
        if !self.file_entries.contains_key(uri) {
            return;
        }

        // Get all entries for this URI
        if let Some(entries) = self.file_entries.remove(uri) {
            // Remove each entry from the definitions map
            for entry in entries {
                if let Some(fqn_entries) = self.definitions.get_mut(&entry.fully_qualified_name) {
                    fqn_entries.retain(|e| e.location.uri != *uri);

                    if fqn_entries.is_empty() {
                        self.definitions.remove(&entry.fully_qualified_name);
                    }
                }
            }
        }

        // Clean up references from this URI
        self.remove_references_for_uri(uri);
    }

    // Add a reference to a symbol
    pub fn add_reference(&mut self, fully_qualified_name: FullyQualifiedName, location: Location) {
        self.references
            .entry(fully_qualified_name)
            .or_insert_with(Vec::new)
            .push(location);
    }

    // Remove all references from a specific URI
    pub fn remove_references_for_uri(&mut self, uri: &Url) {
        for refs in self.references.values_mut() {
            refs.retain(|loc| loc.uri != *uri);
        }

        // Remove any empty reference entries
        self.references.retain(|_, refs| !refs.is_empty());
    }

    // Find all references to a symbol
    pub fn find_references(&self, fully_qualified_name: &FullyQualifiedName) -> Vec<Location> {
        match self.references.get(fully_qualified_name) {
            Some(locations) => locations.clone(),
            None => Vec::new(),
        }
    }

    // Find definitions for a fully qualified name
    pub fn find_definition(&self, fully_qualified_name: &FullyQualifiedName) -> Option<&Entry> {
        // Look up entries by fully qualified name
        if let Some(entries) = self.definitions.get(fully_qualified_name) {
            return entries.first();
        }

        None
    }

    // Add a namespace ancestor relationship
    pub fn add_namespace_ancestor(&mut self, namespace: Constant, ancestor: Constant) {
        let ancestors = self
            .namespace_ancestors
            .entry(namespace.clone())
            .or_insert_with(Vec::new);
        if !ancestors.contains(&ancestor) {
            ancestors.push(ancestor);
        }
    }

    // Get the ancestor chain for a namespace
    pub fn get_namespace_ancestors(&self, namespace: &Constant) -> Vec<Constant> {
        if let Some(direct_ancestors) = self.namespace_ancestors.get(namespace) {
            let mut all_ancestors = direct_ancestors.clone();

            // Recursively gather ancestors of ancestors
            for ancestor in direct_ancestors {
                let ancestor_ancestors = self.get_namespace_ancestors(ancestor);
                for aa in ancestor_ancestors {
                    if !all_ancestors.contains(&aa) {
                        all_ancestors.push(aa);
                    }
                }
            }

            all_ancestors
        } else {
            Vec::new()
        }
    }

    // Find definition by name in a namespace and its ancestors
    pub fn find_definition_in_namespace(
        &self,
        method_name: &Method,
        namespace: Option<&Constant>,
    ) -> Option<&Entry> {
        // First, try to find in exact namespace
        if let Some(ns) = namespace {
            let fqn = FullyQualifiedName::new(vec![ns.clone()], Some(method_name.clone()));
            if let Some(entries) = self.definitions.get(&fqn) {
                if !entries.is_empty() {
                    return Some(&entries[0]);
                }
            }

            // Then, try to find in ancestor namespaces
            for ancestor in self.get_namespace_ancestors(ns) {
                let ancestor_fqn =
                    FullyQualifiedName::new(vec![ancestor], Some(method_name.clone()));
                if let Some(entries) = self.definitions.get(&ancestor_fqn) {
                    if !entries.is_empty() {
                        return Some(&entries[0]);
                    }
                }
            }
        }

        // Finally, look for top-level methods as fallback
        let top_level_fqn = FullyQualifiedName::new(vec![], Some(method_name.clone()));
        if let Some(entries) = self.definitions.get(&top_level_fqn) {
            if !entries.is_empty() {
                return Some(&entries[0]);
            }
        }

        None
    }

    // Find all definitions of a method by name in all namespaces
    pub fn find_all_definitions_by_method(&self, method_name: &Method) -> Vec<&Entry> {
        let mut results = Vec::new();

        for (fqn, entries) in &self.definitions {
            // Check if this FQN's string representation ends with the method name
            let fqn_str = fqn.to_string();
            if fqn_str.ends_with(&format!("#{}", method_name)) {
                // Add all matching entries
                for entry in entries {
                    if entry.entry_type == EntryType::Method {
                        results.push(entry);
                    }
                }
            }
        }

        results
    }
}

#[cfg(test)]
mod tests {
    use super::super::entry::Visibility;
    use super::*;
    use crate::indexer::{entry::EntryType, types::method::Method};
    use lsp_types::{Position, Range, Url};

    // Create a helper function to build a test entry
    fn create_test_entry(
        name: Constant,
        fqn: FullyQualifiedName,
        uri_str: &str,
        entry_type: EntryType,
        visibility: Visibility,
    ) -> Entry {
        let uri = Url::parse(uri_str).expect("Valid URL");
        let range = Range {
            start: Position {
                line: 0,
                character: 0,
            },
            end: Position {
                line: 1,
                character: 0,
            },
        };

        super::super::entry::EntryBuilder::new(name)
            .fully_qualified_name(fqn)
            .location(Location { uri, range })
            .entry_type(entry_type)
            .visibility(visibility)
            .build()
            .expect("Valid entry")
    }

    #[test]
    fn test_new_ruby_index() {
        let index = RubyIndex::new();

        // Verify empty collections
        assert!(index.definitions.is_empty());
        assert!(index.file_entries.is_empty());
        assert!(index.namespace_ancestors.is_empty());
        assert!(index.references.is_empty());
    }

    #[test]
    fn test_find_definition() {
        let mut index = RubyIndex::new();

        // Create a class entry
        let name = Constant::from("Product");
        let fqn = FullyQualifiedName::new(vec![name.clone()], None);

        let class_entry = create_test_entry(
            name,
            fqn.clone(),
            "file:///test.rb",
            EntryType::Class,
            Visibility::Public,
        );

        // Add the entry
        index.add_entry(class_entry.clone());

        // Find the definition
        let definition = index.find_definition(&fqn);

        // Verify the correct definition was found
        assert!(definition.is_some());
        let def = definition.unwrap();
        assert_eq!(def.constant_name.to_string(), "Product");
        assert_eq!(def.fully_qualified_name.to_string(), "Product");
        assert_eq!(def.entry_type, EntryType::Class);

        // Test finding a non-existent definition
        let not_found_fqn = FullyQualifiedName::new(vec![Constant::from("NonExistent")], None);
        let not_found = index.find_definition(&not_found_fqn);
        assert!(not_found.is_none());
    }

    #[test]
    fn test_find_references() {
        let mut index = RubyIndex::new();

        // Create method entries
        let method_name1 = Method::from(String::from("validate"));
        let class_name1 = Constant::from("User");
        let fqn1 = FullyQualifiedName::new(vec![class_name1], Some(method_name1));

        let method_name2 = Method::from(String::from("validate"));
        let class_name2 = Constant::from("Product");
        let fqn2 = FullyQualifiedName::new(vec![class_name2], Some(method_name2));

        let method_entry1 = create_test_entry(
            Constant::from("validate"),
            fqn1.clone(),
            "file:///test1.rb",
            EntryType::Method,
            Visibility::Public,
        );

        let method_entry2 = create_test_entry(
            Constant::from("validate"),
            fqn2.clone(),
            "file:///test2.rb",
            EntryType::Method,
            Visibility::Public,
        );

        // Add the entries
        index.add_entry(method_entry1.clone());
        index.add_entry(method_entry2.clone());

        // Add references
        index.add_reference(fqn1.clone(), method_entry1.location.clone());
        index.add_reference(fqn2.clone(), method_entry2.location.clone());

        // Find references to specific fully qualified name
        let references = index.find_references(&fqn1);

        // Verify only one entry was found
        assert_eq!(references.len(), 1);
        assert_eq!(references[0].uri.to_string(), "file:///test1.rb");

        // Find references to the other fully qualified name
        let references2 = index.find_references(&fqn2);
        assert_eq!(references2.len(), 1);
        assert_eq!(references2[0].uri.to_string(), "file:///test2.rb");

        // Test finding references for a non-existent name
        let nonexistent_fqn = FullyQualifiedName::new(vec![Constant::from("nonexistent")], None);
        let no_refs = index.find_references(&nonexistent_fqn);
        assert!(no_refs.is_empty());
    }

    #[test]
    fn test_remove_entries_for_uri() {
        let mut index = RubyIndex::new();

        // Create entries
        let user_name = Constant::from("User");
        let user_fqn = FullyQualifiedName::new(vec![user_name.clone()], None);

        let save_name = Constant::from("save");
        let save_method = Method::from(String::from("save"));
        let save_fqn = FullyQualifiedName::new(vec![user_name.clone()], Some(save_method));

        let product_name = Constant::from("Product");
        let product_fqn = FullyQualifiedName::new(vec![product_name.clone()], None);

        let entry1 = create_test_entry(
            user_name,
            user_fqn.clone(),
            "file:///models/user.rb",
            EntryType::Class,
            Visibility::Public,
        );

        let entry2 = create_test_entry(
            save_name,
            save_fqn.clone(),
            "file:///models/user.rb",
            EntryType::Method,
            Visibility::Public,
        );

        let entry3 = create_test_entry(
            product_name,
            product_fqn.clone(),
            "file:///models/product.rb",
            EntryType::Class,
            Visibility::Public,
        );

        index.add_entry(entry1.clone());
        index.add_entry(entry2.clone());
        index.add_entry(entry3.clone());

        // Add references
        index.add_reference(
            user_fqn.clone(),
            Location {
                uri: Url::parse("file:///app.rb").unwrap(),
                range: Range::default(),
            },
        );

        // Verify entries were added
        assert_eq!(index.definitions.len(), 3);
        assert_eq!(index.file_entries.len(), 2);

        // Remove entries for the first URI
        index.remove_entries_for_uri(&Url::parse("file:///models/user.rb").unwrap());

        // Verify only entries from the first URI were removed
        assert_eq!(index.definitions.len(), 1);
        assert_eq!(index.file_entries.len(), 1);
        assert!(index.definitions.contains_key(&product_fqn));
        assert!(!index.definitions.contains_key(&user_fqn));
        assert!(!index.definitions.contains_key(&save_fqn));

        // References should still exist though
        let refs = index.find_references(&user_fqn);
        assert_eq!(refs.len(), 1);
    }
}
