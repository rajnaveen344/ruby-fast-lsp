use std::cmp::PartialEq;
use std::collections::HashMap;
use std::fmt::Display;

use lsp_types::Location;

use super::types::constant::Constant;
use super::types::fully_qualified_constant::FullyQualifiedName;

#[derive(Debug, Clone, PartialEq)]
pub enum EntryType {
    Class,
    SingletonClass,
    Module,
    Method,
    Constant,
    ConstantAlias,
    UnresolvedAlias,
    LocalVariable,
    InstanceVariable,
}

impl Display for EntryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
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
    pub constant_name: Constant,                  // The name of the entity
    pub fully_qualified_name: FullyQualifiedName, // Complete namespace path
    pub location: Location,                       // Where this entry is defined
    pub entry_type: EntryType,                    // Type of entry
    pub visibility: Visibility,                   // Public, protected, private
    pub metadata: HashMap<String, String>,        // Additional information
}

// Helper struct for building a new entry
pub struct EntryBuilder {
    constant_name: Constant,
    fully_qualified_name: Option<FullyQualifiedName>,
    location: Option<Location>,
    entry_type: Option<EntryType>,
    visibility: Option<Visibility>,
    metadata: HashMap<String, String>,
}

impl EntryBuilder {
    pub fn new(constant_name: Constant) -> Self {
        EntryBuilder {
            constant_name,
            fully_qualified_name: None,
            location: None,
            entry_type: None,
            visibility: None,
            metadata: HashMap::new(),
        }
    }

    pub fn fully_qualified_name(mut self, fqn: FullyQualifiedName) -> Self {
        self.fully_qualified_name = Some(fqn);
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

    pub fn build(self) -> Result<Entry, &'static str> {
        let fully_qualified_name = self
            .fully_qualified_name
            .ok_or("fully_qualified_name is required")?;
        let location = self.location.ok_or("location is required")?;
        let entry_type = self.entry_type.ok_or("entry_type is required")?;
        let visibility = self.visibility.unwrap_or(Visibility::Public);

        Ok(Entry {
            constant_name: self.constant_name,
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
    use crate::indexer::{index::RubyIndex, types::method::Method};

    use super::*;
    use tower_lsp::lsp_types::{Position, Range, Url};

    // Create a helper function to build a test entry
    fn create_test_entry(
        name: &str,
        fqn: &FullyQualifiedName,
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

        EntryBuilder::new(name.to_string().into())
            .fully_qualified_name(fqn.clone())
            .location(Location { uri, range })
            .entry_type(entry_type)
            .visibility(visibility)
            .build()
            .expect("Valid entry")
    }

    #[test]
    fn test_add_entry() {
        let mut index = RubyIndex::new();

        let fqn = FullyQualifiedName::new(vec![], Some(Method::from(String::from("User"))));

        // Create a class entry
        let class_entry = create_test_entry(
            "User",
            &fqn,
            "file:///test.rb",
            EntryType::Class,
            Visibility::Public,
        );

        // Add the entry to the index
        index.add_entry(class_entry.clone());

        // Verify it was added correctly
        assert_eq!(index.definitions.len(), 1);
        assert!(index.definitions.contains_key(&fqn.clone().into()));
        assert_eq!(index.file_entries.len(), 1);
        assert!(index
            .file_entries
            .contains_key(&Url::parse("file:///test.rb").unwrap()));
        assert_eq!(
            index
                .file_entries
                .get(&Url::parse("file:///test.rb").unwrap())
                .unwrap()
                .len(),
            1
        );
        assert_eq!(index.definitions.len(), 1);
        assert!(index.definitions.contains_key(&fqn.clone().into()));
        assert_eq!(index.definitions.get(&fqn.clone().into()).unwrap().len(), 1);

        // Create a method entry
        let method_entry = create_test_entry(
            "save",
            &FullyQualifiedName::new(vec![], Some(Method::from(String::from("save")))),
            "file:///test.rb",
            EntryType::Method,
            Visibility::Public,
        );

        // Add the method entry
        index.add_entry(method_entry.clone());

        // Verify it was added correctly
        assert_eq!(index.definitions.len(), 2);
        assert!(index.definitions.contains_key(&fqn.clone().into()));
        assert_eq!(
            index
                .file_entries
                .get(&Url::parse("file:///test.rb").unwrap())
                .unwrap()
                .len(),
            2
        );
        assert_eq!(index.definitions.len(), 2);
        assert!(index.definitions.contains_key(&fqn.clone().into()));
        assert_eq!(index.definitions.get(&fqn.clone().into()).unwrap().len(), 1);
    }

    #[test]
    fn test_add_entries_same_name_different_files() {
        let mut index = RubyIndex::new();
        let fqn1 = FullyQualifiedName::new(vec![], Some(Method::from(String::from("process"))));
        let fqn2 = FullyQualifiedName::new(
            vec![Constant::from("AnotherClass")],
            Some(Method::from(String::from("process"))),
        );

        // Create two method entries with the same name but in different files
        let method_entry1 = create_test_entry(
            "process",
            &fqn1,
            "file:///test1.rb",
            EntryType::Method,
            Visibility::Public,
        );

        let method_entry2 = create_test_entry(
            "process",
            &fqn2,
            "file:///test2.rb",
            EntryType::Method,
            Visibility::Public,
        );

        // Add both entries
        index.add_entry(method_entry1.clone());
        index.add_entry(method_entry2.clone());

        // Verify entries were added correctly
        assert_eq!(index.definitions.len(), 2);
        assert!(index.definitions.contains_key(&fqn1.clone().into()));
        assert!(index.definitions.contains_key(&fqn2.clone().into()));

        // Both URIs should be in the uri_to_entries map
        assert_eq!(index.file_entries.len(), 2);
        assert_eq!(
            index
                .file_entries
                .get(&Url::parse("file:///test1.rb").unwrap())
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            index
                .file_entries
                .get(&Url::parse("file:///test2.rb").unwrap())
                .unwrap()
                .len(),
            1
        );

        // Both method entries should be present
        assert_eq!(
            index.definitions.get(&fqn1.clone().into()).unwrap().len(),
            1
        );
        assert_eq!(
            index.definitions.get(&fqn2.clone().into()).unwrap().len(),
            1
        );
    }

    #[test]
    fn test_remove_entries_for_uri() {
        let mut index = RubyIndex::new();
        let uri = Url::parse("file:///test.rb").unwrap();
        let user_fqn = FullyQualifiedName::new(vec![], Some(Method::from(String::from("User"))));
        let save_fqn = FullyQualifiedName::new(
            vec![Constant::from("User")],
            Some(Method::from(String::from("save"))),
        );

        // Add two entries for the same URI
        let class_entry = create_test_entry(
            "User",
            &user_fqn,
            "file:///test.rb",
            EntryType::Class,
            Visibility::Public,
        );

        let method_entry = create_test_entry(
            "save",
            &save_fqn,
            "file:///test.rb",
            EntryType::Method,
            Visibility::Public,
        );

        index.add_entry(class_entry);
        index.add_entry(method_entry);

        // Verify entries were added
        assert_eq!(index.definitions.len(), 2);
        assert_eq!(index.file_entries.len(), 1);
        assert_eq!(
            index
                .definitions
                .get(&user_fqn.clone().into())
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            index
                .definitions
                .get(&save_fqn.clone().into())
                .unwrap()
                .len(),
            1
        );

        // Remove entries for the URI
        index.remove_entries_for_uri(&uri);

        // Verify all entries were removed
        assert!(index.definitions.is_empty());
        assert!(index.file_entries.is_empty());
    }

    #[test]
    fn test_entry_builder() {
        let uri = Url::parse("file:///test.rb").unwrap();
        let range = Range {
            start: Position::new(1, 0),
            end: Position::new(3, 2),
        };
        let fqn = FullyQualifiedName::new(vec![], Some(Method::from(String::from("MyClass"))));

        // Create an entry using the builder
        let entry = EntryBuilder::new("MyClass".into())
            .fully_qualified_name(fqn.clone())
            .location(Location {
                uri: uri.clone(),
                range,
            })
            .entry_type(EntryType::Class)
            .visibility(Visibility::Public)
            .build()
            .unwrap(); // Unwrap the Result

        // Verify the entry was built correctly
        assert_eq!(entry.constant_name, "MyClass".into());
        assert_eq!(entry.fully_qualified_name, fqn);
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
        let fqn = FullyQualifiedName::new(vec![], Some(Method::from(String::from("my_method"))));

        // Create an entry with only the required fields
        let entry = EntryBuilder::new("my_method".into())
            .fully_qualified_name(fqn.clone())
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
        let _ = EntryBuilder::new("MyClass".into())
            .location(Location { uri, range })
            .entry_type(EntryType::Class)
            .build()
            .unwrap(); // This should fail
    }

    #[test]
    #[should_panic]
    fn test_entry_builder_missing_location() {
        // Try to build without setting location
        let fqn = FullyQualifiedName::new(vec![], Some(Method::from(String::from("MyClass"))));
        let _ = EntryBuilder::new("MyClass".into())
            .fully_qualified_name(fqn.clone())
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
        let fqn = FullyQualifiedName::new(vec![], Some(Method::from(String::from("MyClass"))));

        // Try to build without setting entry_type
        let _ = EntryBuilder::new("MyClass".into())
            .fully_qualified_name(fqn.clone())
            .location(Location { uri, range })
            .build()
            .unwrap(); // This should fail
    }
}
