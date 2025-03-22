use std::collections::HashMap;

use lsp_types::{Location, Url};

use super::{
    entry::{Entry, EntryType},
    types::{constant::Constant, fully_qualified_constant::FullyQualifiedName},
};

#[derive(Debug)]
pub struct RubyIndex {
    pub file_entries: HashMap<Url, Vec<Entry>>,
    pub namespace_ancestors: HashMap<Constant, Vec<Constant>>,
    pub definitions: HashMap<FullyQualifiedName, Vec<Entry>>,
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
        let a = "".bytes();
        let fully_qualified_name = entry.fully_qualified_name.to_string();

        // Update the namespace tree, but not for local variables
        if entry.entry_type != EntryType::LocalVariable
            && entry.entry_type != EntryType::InstanceVariable
        {
            self.update_namespace_tree(&fully_qualified_name);
        }

        // Add to the main entries map
        let entries = self
            .definitions
            .entry(fully_qualified_name.clone())
            .or_insert_with(Vec::new);
        entries.push(entry.clone());

        // Add to the uri_to_entries map for this file
        let uri_string = entry.location.uri.to_string();
        let uri_entries = self.file_entries.entry(uri_string).or_insert_with(Vec::new);
        uri_entries.push(entry.clone());

        // Add to the appropriate lookup map based on entry type
        match entry.entry_type {
            EntryType::Method => {
                let method_entries = self
                    .methods_by_name
                    .entry(entry.short_name.to_string())
                    .or_insert_with(Vec::new);
                method_entries.push(entry);
            }
            EntryType::Class
            | EntryType::Module
            | EntryType::SingletonClass
            | EntryType::Constant
            | EntryType::ConstantAlias
            | EntryType::UnresolvedAlias => {
                let constant_entries = self
                    .constants_by_name
                    .entry(entry.short_name.to_string())
                    .or_insert_with(Vec::new);
                constant_entries.push(entry);
            }
            EntryType::LocalVariable | EntryType::InstanceVariable => {
                // Don't index these in by_name maps
            }
        }
    }

    pub fn remove_entries_for_uri(&mut self, uri: &Url) {
        let uri_string = uri.to_string();

        // If no entries for this URI, return early
        if !self.file_entries.contains_key(&uri_string) {
            return;
        }

        // Get all entries for this URI
        let entries = self.file_entries.remove(&uri_string).unwrap_or_default();

        // Remove each entry from the main map and lookup maps
        for entry in entries {
            let fqn = entry.fully_qualified_name.to_string();

            // Remove from entries map
            if let Some(fqn_entries) = self.entries.get_mut(&fqn) {
                fqn_entries.retain(|e| e.location.uri != *uri);

                if fqn_entries.is_empty() {
                    self.entries.remove(&fqn);
                }
            }

            // Remove from lookup maps
            match entry.entry_type {
                EntryType::Method => {
                    if let Some(method_entries) =
                        self.methods_by_name.get_mut(&entry.short_name.to_string())
                    {
                        method_entries.retain(|e| e.location.uri != *uri);
                        if method_entries.is_empty() {
                            self.methods_by_name.remove(&entry.short_name.to_string());
                        }
                    }
                }
                EntryType::Class
                | EntryType::Module
                | EntryType::SingletonClass
                | EntryType::Constant
                | EntryType::ConstantAlias
                | EntryType::UnresolvedAlias => {
                    if let Some(constant_entries) = self
                        .constants_by_name
                        .get_mut(&entry.short_name.to_string())
                    {
                        constant_entries.retain(|e| e.location.uri != *uri);
                        if constant_entries.is_empty() {
                            self.constants_by_name.remove(&entry.short_name.to_string());
                        }
                    }
                }
                EntryType::LocalVariable | EntryType::InstanceVariable => {
                    // Local variables and instance variables are not indexed by name
                }
            }
        }

        // Also remove the require path for this URI if it exists
        if let Some(require_path) = get_require_path(uri) {
            self.require_paths.remove(&require_path);
        }

        // Clean up references from this URI
        self.remove_references_for_uri(uri);
    }

    // Register an included hook that will be executed when module_name is included into any namespace
    pub fn register_included_hook<F>(&mut self, module_name: &str, hook: F)
    where
        F: Fn(&mut RubyIndex, &Entry) + 'static + Send + Sync,
    {
        self.included_hooks
            .entry(module_name.to_string())
            .or_insert_with(Vec::new)
            .push(DebugableFn(Box::new(hook)));
    }

    // Add a file's require path to the index for require autocompletion
    pub fn add_require_path(&mut self, uri: &Url) {
        if let Some(require_path) = get_require_path(uri) {
            self.require_paths.insert(require_path, uri.clone());
        }
    }

    // Search for require paths that match the given prefix
    pub fn search_require_paths(&self, query: &str) -> Vec<Url> {
        self.require_paths
            .iter()
            .filter(|(path, _)| path.starts_with(query))
            .map(|(_, url)| url.clone())
            .collect()
    }

    // Search for entries that match the given prefix
    pub fn prefix_search(&self, query: &str) -> Vec<Vec<Entry>> {
        self.entries
            .iter()
            .filter(|(name, _)| name.starts_with(query))
            .map(|(_, entries)| entries.clone())
            .collect()
    }

    // Add the linearized ancestors for a namespace
    pub fn add_ancestors(&mut self, fully_qualified_name: &str, ancestors: Vec<String>) {
        self.ancestors
            .insert(fully_qualified_name.to_string(), ancestors);
    }

    // Get the linearized ancestors for a namespace
    pub fn linearized_ancestors_of(&self, fully_qualified_name: &str) -> Option<&Vec<String>> {
        self.ancestors.get(fully_qualified_name)
    }

    // Check if the index is empty
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    // Get a list of all fully qualified names in the index
    pub fn names(&self) -> Vec<String> {
        self.entries.keys().cloned().collect()
    }

    // Check if a name is indexed
    pub fn indexed(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    // Get the number of entries in the index
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    // Get entries by their fully qualified name
    pub fn get_entries(&self, fully_qualified_name: &str) -> Option<&Vec<Entry>> {
        self.entries.get(fully_qualified_name)
    }

    // Get all entries for a given URI
    pub fn entries_for(&self, uri: &str) -> Option<&Vec<Entry>> {
        self.file_entries.get(uri)
    }

    // Clear the ancestors cache, typically done when there are changes that affect ancestor relationships
    pub fn clear_ancestors_cache(&mut self) {
        self.ancestors.clear();
    }

    // Add a reference to a symbol
    pub fn add_reference(&mut self, fully_qualified_name: &str, location: Location) {
        self.references
            .entry(fully_qualified_name.to_string())
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
    pub fn find_references(&self, fully_qualified_name: &str) -> Vec<Location> {
        match self.references.get(fully_qualified_name) {
            Some(locations) => locations.clone(),
            None => Vec::new(),
        }
    }

    // Find definitions for a name
    pub fn find_definition(&self, name: &str) -> Option<&Entry> {
        // First try direct lookup - works for fully qualified names
        if let Some(entries) = self.entries.get(name) {
            return entries.first();
        }

        // For instance variables (those starting with @)
        if name.starts_with('@') {
            // Try to find any instance variable entry with this name in a current scope
            for (fqn, entries) in &self.entries {
                if fqn.ends_with(name) && !entries.is_empty() {
                    return entries.first();
                }
            }
        }

        // For local variables (those starting with $)
        if name.starts_with('$') {
            // Extract the variable name without the $
            let var_name = &name[1..];

            // Try to find any variable entry with this name in a current scope
            for (fqn, entries) in &self.entries {
                if fqn.ends_with(&format!("${}", var_name)) && !entries.is_empty() {
                    if let Some(entry) = entries.first() {
                        if entry.entry_type == EntryType::LocalVariable {
                            return Some(entry);
                        }
                    }
                }
            }
        }

        // For plain local variables without any $ prefix
        if !name.contains('#')
            && !name.contains("::")
            && !name.contains('$')
            && !name.starts_with('@')
        {
            // First try to find a local variable with this name
            for (fqn, entries) in &self.entries {
                let var_pattern = format!("${}", name);
                if fqn.ends_with(&var_pattern) && !entries.is_empty() {
                    if let Some(entry) = entries.first() {
                        if entry.entry_type == EntryType::LocalVariable {
                            return Some(entry);
                        }
                    }
                }
            }

            // Search for constants whose name could match
            if let Some(constants) = self.constants_by_name.get(name) {
                return constants.first();
            }
        }

        // For methods
        if name.contains('#') {
            // This is a method reference in the format Class#method
            let parts: Vec<&str> = name.split('#').collect();
            if parts.len() == 2 {
                let class_name = parts[0];
                let method_name = parts[1];

                // First look for an exact match with the fully qualified name
                if let Some(entries) = self.entries.get(name) {
                    return entries.first();
                }

                // If no exact match, try to find the method in the class's methods
                if let Some(method_entries) = self.methods_by_name.get(method_name) {
                    for entry in method_entries {
                        let fqn = entry.fully_qualified_name.to_string();
                        if fqn == name
                            || fqn.ends_with(&format!("::{}#{}", class_name, method_name))
                        {
                            return Some(entry);
                        }
                    }
                }
            }
        }

        None
    }

    // Update the namespace tree when an entry is added
    fn update_namespace_tree(&mut self, fully_qualified_name: &str) {
        // Split the name by :: to get the parts
        let parts: Vec<&str> = fully_qualified_name.split("::").collect();
        if parts.is_empty() {
            return;
        }

        // Add each level of nesting to the tree
        let mut current_namespace = String::new();
        for (i, part) in parts.iter().enumerate() {
            // If this is the last part, it's the name itself and not a parent namespace
            if i == parts.len() - 1 {
                break;
            }

            // Build the namespace path
            if current_namespace.is_empty() {
                current_namespace = part.to_string();
            } else {
                current_namespace = format!("{}::{}", current_namespace, part);
            }

            // Add this namespace to the tree
            let namespace_children = self
                .namespace_tree
                .entry(current_namespace.clone())
                .or_insert_with(Vec::new);

            // Add the next part as a child if it doesn't already exist
            let next_part = parts[i + 1];
            if !namespace_children.contains(&next_part.to_string()) {
                namespace_children.push(next_part.to_string());
            }
        }

        // Also add the top-level parts to the root namespace
        let root_children = self
            .namespace_tree
            .entry(String::new())
            .or_insert_with(Vec::new);
        if !parts.is_empty() && !root_children.contains(&parts[0].to_string()) {
            root_children.push(parts[0].to_string());
        }
    }
}

// Helper function to convert a URI to a require path
fn get_require_path(uri: &Url) -> Option<String> {
    if uri.scheme() != "file" {
        return None;
    }

    let path = uri.path();

    // Extract the relative path that would be used in a require statement
    // This is a simplified implementation, in a real-world scenario you would:
    // 1. Identify the project root
    // 2. Extract the relative path from the root
    // 3. Remove the file extension

    // For now, just use the file name without extension as a simple approximation
    if let Some(file_name) = path.rsplit('/').next() {
        if let Some(name) = file_name.rsplit('.').next() {
            return Some(name.to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::super::entry::Visibility;
    use super::*;
    use lsp_types::{Position, Range, Url};

    // Create a helper function to build a test entry
    fn create_test_entry(
        name: &str,
        fqn: &str,
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

        super::super::entry::EntryBuilder::new(ShortName::from(name))
            .fully_qualified_name(fqn.into())
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
        assert!(index.entries.is_empty());
        assert!(index.file_entries.is_empty());
        assert!(index.methods_by_name.is_empty());
        assert!(index.constants_by_name.is_empty());
        assert!(index.namespace_tree.is_empty());
        assert!(index.require_paths.is_empty());
        assert!(index.ancestors.is_empty());
    }

    #[test]
    fn test_find_definition() {
        let mut index = RubyIndex::new();

        // Create a class entry
        let class_entry = create_test_entry(
            "Product",
            "Product",
            "file:///test.rb",
            EntryType::Class,
            Visibility::Public,
        );

        // Add the entry
        index.add_entry(class_entry.clone());

        // Find the definition
        let definition = index.find_definition("Product");

        // Verify the correct definition was found
        assert!(definition.is_some());
        let def = definition.unwrap();
        assert_eq!(def.short_name.to_string(), "Product");
        assert_eq!(def.fully_qualified_name.to_string(), "Product");
        assert_eq!(def.entry_type, EntryType::Class);

        // Test finding a non-existent definition
        let not_found = index.find_definition("NonExistent");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_find_references() {
        let mut index = RubyIndex::new();

        // Create two method entries with the same name
        let method_entry1 = create_test_entry(
            "validate",
            "User#validate",
            "file:///test1.rb",
            EntryType::Method,
            Visibility::Public,
        );

        let method_entry2 = create_test_entry(
            "validate",
            "Product#validate",
            "file:///test2.rb",
            EntryType::Method,
            Visibility::Public,
        );

        // Add the entries
        index.add_entry(method_entry1.clone());
        index.add_entry(method_entry2.clone());

        // Add references
        index.add_reference("User#validate", method_entry1.location.clone());
        index.add_reference("Product#validate", method_entry2.location.clone());

        // Find references to specific fully qualified name
        let references = index.find_references("User#validate");

        // Verify only one entry was found
        assert_eq!(references.len(), 1);
        assert_eq!(references[0].uri.to_string(), "file:///test1.rb");

        // Find references to the other fully qualified name
        let references2 = index.find_references("Product#validate");
        assert_eq!(references2.len(), 1);
        assert_eq!(references2[0].uri.to_string(), "file:///test2.rb");

        // Test finding references for a non-existent name
        let no_refs = index.find_references("nonexistent");
        assert!(no_refs.is_empty());
    }

    #[test]
    fn test_prefix_search() {
        let mut index = RubyIndex::new();

        // Add some entries with different namespace prefixes
        let entry1 = create_test_entry(
            "User",
            "User",
            "file:///user.rb",
            EntryType::Class,
            Visibility::Public,
        );

        let entry2 = create_test_entry(
            "UserProfile",
            "UserProfile",
            "file:///user_profile.rb",
            EntryType::Class,
            Visibility::Public,
        );

        let entry3 = create_test_entry(
            "Post",
            "Post",
            "file:///post.rb",
            EntryType::Class,
            Visibility::Public,
        );

        index.add_entry(entry1);
        index.add_entry(entry2);
        index.add_entry(entry3);

        // Search with prefix "User"
        let results = index.prefix_search("User");

        // Should find both User and UserProfile
        assert_eq!(results.len(), 2);

        // Verify entries were found
        let mut found_user = false;
        let mut found_user_profile = false;

        for entries in &results {
            for entry in entries {
                let name = entry.fully_qualified_name.to_string();
                if name == "User" {
                    found_user = true;
                } else if name == "UserProfile" {
                    found_user_profile = true;
                }
            }
        }

        assert!(found_user, "Should find User entry");
        assert!(found_user_profile, "Should find UserProfile entry");

        // Search with prefix "P"
        let results = index.prefix_search("P");

        // Should find only Post
        assert_eq!(results.len(), 1);
        let entry = &results[0][0];
        assert_eq!(entry.fully_qualified_name.to_string(), "Post");
    }

    #[test]
    fn test_require_paths() {
        let mut index = RubyIndex::new();

        // Create test URIs
        let uri1 = Url::parse("file:///app/models/user.rb").unwrap();
        let uri2 = Url::parse("file:///app/models/user_profile.rb").unwrap();
        let uri3 = Url::parse("file:///app/controllers/posts_controller.rb").unwrap();

        // Add require paths
        index.add_require_path(&uri1);
        index.add_require_path(&uri2);
        index.add_require_path(&uri3);

        // Search for paths starting with "user"
        let results = index.search_require_paths("user");

        // Should find both user.rb and user_profile.rb
        assert_eq!(results.len(), 2);

        // Verify URIs were found
        let found_uris: Vec<String> = results.iter().map(|url| url.to_string()).collect();
        assert!(
            found_uris.contains(&uri1.to_string()),
            "Should find user.rb URI"
        );
        assert!(
            found_uris.contains(&uri2.to_string()),
            "Should find user_profile.rb URI"
        );

        // Search for "posts"
        let results = index.search_require_paths("posts");

        // Should find only posts_controller.rb
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].to_string(), uri3.to_string());
    }

    #[test]
    fn test_ancestors() {
        let mut index = RubyIndex::new();

        // Define ancestor relationships
        let ancestors_for_user = vec![
            "User".to_string(),
            "ActiveRecord::Base".to_string(),
            "Object".to_string(),
            "BasicObject".to_string(),
        ];

        index.add_ancestors("User", ancestors_for_user.clone());

        // Test getting ancestors
        let retrieved_ancestors = index.linearized_ancestors_of("User");
        assert!(retrieved_ancestors.is_some());
        assert_eq!(*retrieved_ancestors.unwrap(), ancestors_for_user);

        // Test non-existent ancestors
        let no_ancestors = index.linearized_ancestors_of("NonExistent");
        assert!(no_ancestors.is_none());

        // Test clearing ancestors
        index.clear_ancestors_cache();
        assert!(index.ancestors.is_empty());
    }

    #[test]
    fn test_index_properties() {
        let mut index = RubyIndex::new();

        // Test initially empty
        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
        assert!(index.names().is_empty());
        assert!(!index.indexed("Test"));

        // Add an entry
        let entry = create_test_entry(
            "Test",
            "Test",
            "file:///test.rb",
            EntryType::Class,
            Visibility::Public,
        );

        index.add_entry(entry);

        // Test not empty now
        assert!(!index.is_empty());
        assert_eq!(index.len(), 1);
        assert_eq!(index.names(), vec!["Test"]);
        assert!(index.indexed("Test"));
        assert!(!index.indexed("NonExistent"));

        // Test getting entries
        let entries = index.get_entries("Test");
        assert!(entries.is_some());
        assert_eq!(entries.unwrap().len(), 1);

        // Test entries for URI
        let uri_entries = index.entries_for("file:///test.rb");
        assert!(uri_entries.is_some());
        assert_eq!(uri_entries.unwrap().len(), 1);
    }

    #[test]
    fn test_remove_entries_for_uri() {
        let mut index = RubyIndex::new();

        // Create and add entries for two different URIs
        let entry1 = create_test_entry(
            "User",
            "User",
            "file:///models/user.rb",
            EntryType::Class,
            Visibility::Public,
        );

        let entry2 = create_test_entry(
            "save",
            "User#save",
            "file:///models/user.rb",
            EntryType::Method,
            Visibility::Public,
        );

        let entry3 = create_test_entry(
            "Product",
            "Product",
            "file:///models/product.rb",
            EntryType::Class,
            Visibility::Public,
        );

        index.add_entry(entry1);
        index.add_entry(entry2);
        index.add_entry(entry3);

        // Add references
        index.add_reference(
            "User",
            Location {
                uri: Url::parse("file:///app.rb").unwrap(),
                range: Range::default(),
            },
        );

        // Verify entries were added
        assert_eq!(index.entries.len(), 3);
        assert_eq!(index.file_entries.len(), 2);

        // Remove entries for the first URI
        index.remove_entries_for_uri(&Url::parse("file:///models/user.rb").unwrap());

        // Verify only entries from the first URI were removed
        assert_eq!(index.entries.len(), 1);
        assert_eq!(index.file_entries.len(), 1);
        assert!(index.entries.contains_key("Product"));
        assert!(!index.entries.contains_key("User"));
        assert!(!index.entries.contains_key("User#save"));

        // References should still exist though
        let refs = index.find_references("User");
        assert_eq!(refs.len(), 1);
    }
}
