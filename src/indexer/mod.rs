use std::collections::HashMap;
use tower_lsp::lsp_types::{Location, Range, Url};

pub mod traverser;

#[derive(Clone)]
pub struct RubyIndex {
    // Main index mapping fully qualified names to entries
    pub entries: HashMap<String, Vec<Entry>>,

    // Map file URIs to their entries for efficient updates
    pub uri_to_entries: HashMap<String, Vec<Entry>>,

    // Maps for quick lookups by specific criteria
    pub methods_by_name: HashMap<String, Vec<Entry>>,
    pub constants_by_name: HashMap<String, Vec<Entry>>,

    // Namespace hierarchy
    pub namespace_tree: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EntryType {
    Class,
    Module,
    Method,
    Constant,
    ConstantAlias,
    UnresolvedAlias,
    LocalVariable,
}

/// Method visibility in Ruby
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Visibility {
    Public,
    Protected,
    Private,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,                      // The name of the entity
    pub fully_qualified_name: String,      // Complete namespace path
    pub location: EntryLocation,           // Where this entry is defined
    pub entry_type: EntryType,             // Type of entry
    pub visibility: Visibility,            // Public, protected, private
    pub metadata: HashMap<String, String>, // Additional information
}

#[derive(Debug, Clone)]
pub struct EntryLocation {
    pub uri: Url,     // File URI
    pub range: Range, // Position in the file
}

impl RubyIndex {
    pub fn new() -> Self {
        RubyIndex {
            entries: HashMap::new(),
            uri_to_entries: HashMap::new(),
            methods_by_name: HashMap::new(),
            constants_by_name: HashMap::new(),
            namespace_tree: HashMap::new(),
        }
    }

    pub fn add_entry(&mut self, entry: Entry) {
        // Add to the main entries map
        let entries = self
            .entries
            .entry(entry.fully_qualified_name.clone())
            .or_insert_with(Vec::new);
        entries.push(entry.clone());

        // Add to the uri_to_entries map for this file
        let uri_string = entry.location.uri.to_string();
        let uri_entries = self
            .uri_to_entries
            .entry(uri_string)
            .or_insert_with(Vec::new);
        uri_entries.push(entry.clone());

        // Add to the appropriate lookup map based on entry type
        match entry.entry_type {
            EntryType::Method => {
                let method_entries = self
                    .methods_by_name
                    .entry(entry.name.clone())
                    .or_insert_with(Vec::new);
                method_entries.push(entry);
            }
            EntryType::Class
            | EntryType::Module
            | EntryType::Constant
            | EntryType::ConstantAlias
            | EntryType::UnresolvedAlias => {
                let constant_entries = self
                    .constants_by_name
                    .entry(entry.name.clone())
                    .or_insert_with(Vec::new);
                constant_entries.push(entry);
            }
            EntryType::LocalVariable => {
                // Local variables are stored in the main entries map but don't need
                // special lookup maps since they are referenced by their fully qualified name
                // which includes the method scope they're defined in
            }
        }
    }

    pub fn remove_entries_for_uri(&mut self, uri: &Url) {
        let uri_string = uri.to_string();

        // If no entries for this URI, return early
        if !self.uri_to_entries.contains_key(&uri_string) {
            return;
        }

        // Get all entries for this URI
        let entries = self.uri_to_entries.remove(&uri_string).unwrap_or_default();

        // Remove each entry from the main map and lookup maps
        for entry in entries {
            // Remove from entries map
            if let Some(fqn_entries) = self.entries.get_mut(&entry.fully_qualified_name) {
                fqn_entries.retain(|e| e.location.uri != *uri);
                if fqn_entries.is_empty() {
                    self.entries.remove(&entry.fully_qualified_name);
                }
            }

            // Remove from lookup maps
            match entry.entry_type {
                EntryType::Method => {
                    if let Some(method_entries) = self.methods_by_name.get_mut(&entry.name) {
                        method_entries.retain(|e| e.location.uri != *uri);
                        if method_entries.is_empty() {
                            self.methods_by_name.remove(&entry.name);
                        }
                    }
                }
                EntryType::Class
                | EntryType::Module
                | EntryType::Constant
                | EntryType::ConstantAlias
                | EntryType::UnresolvedAlias => {
                    if let Some(constant_entries) = self.constants_by_name.get_mut(&entry.name) {
                        constant_entries.retain(|e| e.location.uri != *uri);
                        if constant_entries.is_empty() {
                            self.constants_by_name.remove(&entry.name);
                        }
                    }
                }
                EntryType::LocalVariable => {
                    // Local variables only exist in the main entries map
                    // No special lookup maps to update
                }
            }
        }
    }

    pub fn find_definition(&self, fully_qualified_name: &str) -> Option<&Entry> {
        // First try direct lookup - works for fully qualified names
        if let Some(entries) = self.entries.get(fully_qualified_name) {
            return entries.first();
        }

        // For local variables (those starting with $)
        if fully_qualified_name.starts_with('$') {
            // Extract the variable name without the $
            let var_name = &fully_qualified_name[1..];

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

        // If direct lookup fails, try to extract the method name and search by it
        // This handles cases where analyzer returns just "method_name" instead of "Class#method_name"
        if !fully_qualified_name.contains('#') && !fully_qualified_name.contains('$') {
            // It might be a method call without class context - check methods_by_name
            if let Some(method_entries) = self.methods_by_name.get(fully_qualified_name) {
                return method_entries.first();
            }
        }

        // For method calls inside a namespace (like "Namespace#method")
        if fully_qualified_name.contains('#') {
            let parts: Vec<&str> = fully_qualified_name.split('#').collect();
            if parts.len() == 2 {
                let method_name = parts[1];
                // Try to find the method by name
                if let Some(method_entries) = self.methods_by_name.get(method_name) {
                    return method_entries.first();
                }
            }
        }

        None
    }

    pub fn find_references(&self, fully_qualified_name: &str) -> Vec<Location> {
        // This is a placeholder implementation
        // A real implementation would need to track references during indexing
        self.entries
            .get(fully_qualified_name)
            .map(|entries| {
                entries
                    .iter()
                    .map(|e| Location {
                        uri: e.location.uri.clone(),
                        range: e.location.range,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

// Helper struct for building a new entry
pub struct EntryBuilder {
    name: String,
    fully_qualified_name: Option<String>,
    location: Option<EntryLocation>,
    entry_type: Option<EntryType>,
    visibility: Option<Visibility>,
    metadata: HashMap<String, String>,
}

impl EntryBuilder {
    pub fn new(name: &str) -> Self {
        EntryBuilder {
            name: name.to_string(),
            fully_qualified_name: None,
            location: None,
            entry_type: None,
            visibility: None,
            metadata: HashMap::new(),
        }
    }

    pub fn fully_qualified_name(mut self, fqn: &str) -> Self {
        self.fully_qualified_name = Some(fqn.to_string());
        self
    }

    pub fn location(mut self, uri: Url, range: Range) -> Self {
        self.location = Some(EntryLocation { uri, range });
        self
    }

    pub fn entry_type(mut self, entry_type: EntryType) -> Self {
        self.entry_type = Some(entry_type);
        self
    }

    pub fn visibility(mut self, visibility: Visibility) -> Self {
        self.visibility = Some(visibility);
        self
    }

    pub fn metadata(mut self, key: &str, value: &str) -> Self {
        self.metadata.insert(key.to_string(), value.to_string());
        self
    }

    pub fn build(self) -> Result<Entry, &'static str> {
        let fully_qualified_name = self
            .fully_qualified_name
            .ok_or("fully_qualified_name is required")?;
        let location = self.location.ok_or("location is required")?;
        let entry_type = self.entry_type.ok_or("entry_type is required")?;
        let visibility = self.visibility.unwrap_or(Visibility::Public);

        Ok(Entry {
            name: self.name,
            fully_qualified_name,
            location,
            entry_type,
            visibility,
            metadata: self.metadata,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::{Position, Range, Url};

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

        EntryBuilder::new(name)
            .fully_qualified_name(fqn)
            .location(uri, range)
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
        assert!(index.uri_to_entries.is_empty());
        assert!(index.methods_by_name.is_empty());
        assert!(index.constants_by_name.is_empty());
        assert!(index.namespace_tree.is_empty());
    }

    #[test]
    fn test_add_entry() {
        let mut index = RubyIndex::new();

        // Create a class entry
        let class_entry = create_test_entry(
            "User",
            "User",
            "file:///test.rb",
            EntryType::Class,
            Visibility::Public,
        );

        // Add the entry to the index
        index.add_entry(class_entry.clone());

        // Verify it was added correctly
        assert_eq!(index.entries.len(), 1);
        assert!(index.entries.contains_key("User"));
        assert_eq!(index.uri_to_entries.len(), 1);
        assert!(index.uri_to_entries.contains_key("file:///test.rb"));
        assert_eq!(
            index.uri_to_entries.get("file:///test.rb").unwrap().len(),
            1
        );
        assert_eq!(index.constants_by_name.len(), 1);
        assert!(index.constants_by_name.contains_key("User"));
        assert_eq!(index.constants_by_name.get("User").unwrap().len(), 1);

        // Create a method entry
        let method_entry = create_test_entry(
            "save",
            "User#save",
            "file:///test.rb",
            EntryType::Method,
            Visibility::Public,
        );

        // Add the method entry
        index.add_entry(method_entry.clone());

        // Verify it was added correctly
        assert_eq!(index.entries.len(), 2);
        assert!(index.entries.contains_key("User#save"));
        assert_eq!(
            index.uri_to_entries.get("file:///test.rb").unwrap().len(),
            2
        );
        assert_eq!(index.methods_by_name.len(), 1);
        assert!(index.methods_by_name.contains_key("save"));
        assert_eq!(index.methods_by_name.get("save").unwrap().len(), 1);
    }

    #[test]
    fn test_add_entries_same_name_different_files() {
        let mut index = RubyIndex::new();

        // Create two method entries with the same name but in different files
        let method_entry1 = create_test_entry(
            "process",
            "MyClass#process",
            "file:///test1.rb",
            EntryType::Method,
            Visibility::Public,
        );

        let method_entry2 = create_test_entry(
            "process",
            "OtherClass#process",
            "file:///test2.rb",
            EntryType::Method,
            Visibility::Public,
        );

        // Add both entries
        index.add_entry(method_entry1.clone());
        index.add_entry(method_entry2.clone());

        // Verify entries were added correctly
        assert_eq!(index.entries.len(), 2);
        assert!(index.entries.contains_key("MyClass#process"));
        assert!(index.entries.contains_key("OtherClass#process"));

        // Both URIs should be in the uri_to_entries map
        assert_eq!(index.uri_to_entries.len(), 2);
        assert_eq!(
            index.uri_to_entries.get("file:///test1.rb").unwrap().len(),
            1
        );
        assert_eq!(
            index.uri_to_entries.get("file:///test2.rb").unwrap().len(),
            1
        );

        // The "process" method should have 2 entries in methods_by_name
        assert_eq!(index.methods_by_name.len(), 1);
        assert_eq!(index.methods_by_name.get("process").unwrap().len(), 2);
    }

    #[test]
    fn test_remove_entries_for_uri() {
        let mut index = RubyIndex::new();
        let uri = Url::parse("file:///test.rb").unwrap();

        // Add two entries for the same URI
        let class_entry = create_test_entry(
            "User",
            "User",
            "file:///test.rb",
            EntryType::Class,
            Visibility::Public,
        );

        let method_entry = create_test_entry(
            "save",
            "User#save",
            "file:///test.rb",
            EntryType::Method,
            Visibility::Public,
        );

        index.add_entry(class_entry);
        index.add_entry(method_entry);

        // Verify entries were added
        assert_eq!(index.entries.len(), 2);
        assert_eq!(index.uri_to_entries.len(), 1);
        assert_eq!(index.methods_by_name.len(), 1);
        assert_eq!(index.constants_by_name.len(), 1);

        // Remove entries for the URI
        index.remove_entries_for_uri(&uri);

        // Verify all entries were removed
        assert!(index.entries.is_empty());
        assert!(index.uri_to_entries.is_empty());
        assert!(index.methods_by_name.is_empty());
        assert!(index.constants_by_name.is_empty());
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
        assert_eq!(def.name, "Product");
        assert_eq!(def.fully_qualified_name, "Product");
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
    fn test_entry_builder() {
        let uri = Url::parse("file:///test.rb").unwrap();
        let range = Range {
            start: Position::new(1, 0),
            end: Position::new(3, 2),
        };

        // Create an entry using the builder
        let entry = EntryBuilder::new("MyClass")
            .fully_qualified_name("MyClass")
            .location(uri.clone(), range)
            .entry_type(EntryType::Class)
            .visibility(Visibility::Public)
            .build()
            .unwrap(); // Unwrap the Result

        // Verify the entry was built correctly
        assert_eq!(entry.name, "MyClass");
        assert_eq!(entry.fully_qualified_name, "MyClass");
        assert_eq!(entry.location.uri, uri);
        assert_eq!(entry.location.range, range);
        assert_eq!(entry.entry_type, EntryType::Class);
        assert_eq!(entry.visibility, Visibility::Public);
        assert!(entry.metadata.is_empty());
    }

    #[test]
    fn test_entry_builder_defaults() {
        let uri = Url::parse("file:///test.rb").unwrap();
        let range = Range {
            start: Position::new(1, 0),
            end: Position::new(3, 2),
        };

        // Create an entry with only the required fields
        let entry = EntryBuilder::new("my_method")
            .fully_qualified_name("MyClass#my_method")
            .location(uri.clone(), range)
            .entry_type(EntryType::Method)
            .build()
            .unwrap(); // Unwrap the Result

        // Verify default values were used
        assert_eq!(entry.visibility, Visibility::Public); // Default visibility should be Public
        assert!(entry.metadata.is_empty()); // Metadata should be an empty HashMap
    }

    #[test]
    #[should_panic]
    fn test_entry_builder_missing_fully_qualified_name() {
        let uri = Url::parse("file:///test.rb").unwrap();
        let range = Range {
            start: Position::new(1, 0),
            end: Position::new(3, 2),
        };

        // Try to build without setting fully_qualified_name
        let _ = EntryBuilder::new("MyClass")
            .location(uri, range)
            .entry_type(EntryType::Class)
            .build()
            .unwrap(); // This should fail
    }

    #[test]
    #[should_panic]
    fn test_entry_builder_missing_location() {
        // Try to build without setting location
        let _ = EntryBuilder::new("MyClass")
            .fully_qualified_name("MyClass")
            .entry_type(EntryType::Class)
            .build()
            .unwrap(); // This should fail
    }

    #[test]
    #[should_panic]
    fn test_entry_builder_missing_entry_type() {
        let uri = Url::parse("file:///test.rb").unwrap();
        let range = Range {
            start: Position::new(1, 0),
            end: Position::new(3, 2),
        };

        // Try to build without setting entry_type
        let _ = EntryBuilder::new("MyClass")
            .fully_qualified_name("MyClass")
            .location(uri, range)
            .build()
            .unwrap(); // This should fail
    }

    #[test]
    fn test_find_definition_with_various_fqns() {
        let mut index = RubyIndex::new();

        // Set up some test entries with different FQN formats
        let class_entry = Entry {
            name: "TestClass".to_string(),
            fully_qualified_name: "TestClass".to_string(),
            location: EntryLocation {
                uri: Url::parse("file:///test/file.rb").unwrap(),
                range: Range {
                    start: Position::new(0, 0),
                    end: Position::new(5, 3),
                },
            },
            entry_type: EntryType::Class,
            visibility: Visibility::Public,
            metadata: HashMap::new(),
        };

        // Method inside a class
        let method_entry = Entry {
            name: "test_method".to_string(),
            fully_qualified_name: "TestClass#test_method".to_string(),
            location: EntryLocation {
                uri: Url::parse("file:///test/file.rb").unwrap(),
                range: Range {
                    start: Position::new(2, 2),
                    end: Position::new(4, 5),
                },
            },
            entry_type: EntryType::Method,
            visibility: Visibility::Public,
            metadata: HashMap::new(),
        };

        // Add entries to the index
        index.add_entry(class_entry);
        index.add_entry(method_entry);

        // Test lookup by exact FQN
        let found = index.find_definition("TestClass#test_method");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "test_method");

        // Test lookup by just method name (without class context)
        let found = index.find_definition("test_method");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "test_method");

        // Test lookup by method with wrong class
        let found = index.find_definition("WrongClass#test_method");
        assert!(found.is_some(), "Should still find method by name part");
        assert_eq!(found.unwrap().name, "test_method");

        // Test non-existent method
        let not_found = index.find_definition("nonexistent_method");
        assert!(not_found.is_none());
    }

    #[test]
    fn test_find_local_variable_definition() {
        let mut index = RubyIndex::new();

        // Create a local variable entry with the format used by the traverser
        let var_entry = Entry {
            name: "my_var".to_string(),
            fully_qualified_name: "TestClass#$my_var".to_string(),
            location: EntryLocation {
                uri: Url::parse("file:///test/file.rb").unwrap(),
                range: Range {
                    start: Position::new(3, 4),
                    end: Position::new(3, 10),
                },
            },
            entry_type: EntryType::LocalVariable,
            visibility: Visibility::Public,
            metadata: HashMap::new(),
        };

        // Local variable in a nested method
        let nested_var_entry = Entry {
            name: "nested_var".to_string(),
            fully_qualified_name: "Module::Class#method$nested_var".to_string(),
            location: EntryLocation {
                uri: Url::parse("file:///test/file.rb").unwrap(),
                range: Range {
                    start: Position::new(5, 6),
                    end: Position::new(5, 16),
                },
            },
            entry_type: EntryType::LocalVariable,
            visibility: Visibility::Public,
            metadata: HashMap::new(),
        };

        // Add entries to the index
        index.add_entry(var_entry);
        index.add_entry(nested_var_entry);

        // Test lookup by fully qualified name
        let found1 = index.find_definition("TestClass#$my_var");
        assert!(found1.is_some());
        assert_eq!(found1.unwrap().name, "my_var");

        // Test lookup by just variable name with $ prefix
        let found2 = index.find_definition("$my_var");
        assert!(found2.is_some());
        assert_eq!(found2.unwrap().name, "my_var");

        // Test lookup of nested variable
        let found3 = index.find_definition("$nested_var");
        assert!(found3.is_some());
        assert_eq!(found3.unwrap().name, "nested_var");

        // Test non-existent variable
        let not_found = index.find_definition("$nonexistent_var");
        assert!(not_found.is_none());
    }
}
