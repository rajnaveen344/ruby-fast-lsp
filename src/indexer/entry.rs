use std::collections::HashMap;

use lsp_types::Location;

#[derive(Debug, Clone, PartialEq)]
pub enum EntryType {
    Class,
    Module,
    Method,
    Constant,
    ConstantAlias,
    UnresolvedAlias,
    LocalVariable,
    InstanceVariable,
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
    pub location: Location,                // Where this entry is defined
    pub entry_type: EntryType,             // Type of entry
    pub visibility: Visibility,            // Public, protected, private
    pub metadata: HashMap<String, String>, // Additional information
}

// Helper struct for building a new entry
pub struct EntryBuilder {
    name: String,
    fully_qualified_name: Option<String>,
    location: Option<Location>,
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

    pub fn location(mut self, location: Location) -> Self {
        self.location = Some(location);
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
    use crate::indexer::index::RubyIndex;

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
            .location(Location { uri, range })
            .entry_type(entry_type)
            .visibility(visibility)
            .build()
            .expect("Valid entry")
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
    fn test_entry_builder() {
        let uri = Url::parse("file:///test.rb").unwrap();
        let range = Range {
            start: Position::new(1, 0),
            end: Position::new(3, 2),
        };

        // Create an entry using the builder
        let entry = EntryBuilder::new("MyClass")
            .fully_qualified_name("MyClass")
            .location(Location {
                uri: uri.clone(),
                range,
            })
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
            .location(Location { uri, range })
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
            .location(Location { uri, range })
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
            .location(Location { uri, range })
            .build()
            .unwrap(); // This should fail
    }
}
