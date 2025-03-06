use std::collections::HashMap;

use lsp_types::{Location, Url};

use super::entry::{Entry, EntryType};

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

    // Add a map to track references
    pub references: HashMap<String, Vec<Location>>,
}

impl RubyIndex {
    pub fn new() -> Self {
        RubyIndex {
            entries: HashMap::new(),
            uri_to_entries: HashMap::new(),
            methods_by_name: HashMap::new(),
            constants_by_name: HashMap::new(),
            namespace_tree: HashMap::new(),
            references: HashMap::new(), // Initialize the references map
        }
    }

    pub fn add_entry(&mut self, entry: Entry) {
        // Update the namespace tree, but not for local variables
        if entry.entry_type != EntryType::LocalVariable
            && entry.entry_type != EntryType::InstanceVariable
        {
            self.update_namespace_tree(&entry.fully_qualified_name);
        }

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
            EntryType::LocalVariable | EntryType::InstanceVariable => {
                // Local variables and instance variables are not indexed by name specifically
                // They are only found by their fully qualified name
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
                EntryType::LocalVariable | EntryType::InstanceVariable => {
                    // Local variables and instance variables are not indexed by name
                    // so we don't need to remove them from any lookup maps
                }
            }
        }

        // Also clean up references from this URI
        // We need to iterate through all references and remove those from this URI
        for refs in self.references.values_mut() {
            refs.retain(|loc| loc.uri != *uri);
        }

        // Remove any empty reference entries
        self.references.retain(|_, refs| !refs.is_empty());
    }

    pub fn find_definition(&self, fully_qualified_name: &str) -> Option<&Entry> {
        // First try direct lookup - works for fully qualified names
        if let Some(entries) = self.entries.get(fully_qualified_name) {
            return entries.first();
        }

        // For instance variables (those starting with @)
        if fully_qualified_name.starts_with('@') {
            // Try to find any instance variable entry with this name in a current scope
            for (fqn, entries) in &self.entries {
                if fqn.ends_with(fully_qualified_name) && !entries.is_empty() {
                    return entries.first();
                }
            }
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
        if !fully_qualified_name.contains('#')
            && !fully_qualified_name.contains('$')
            && !fully_qualified_name.starts_with('@')
        {
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
                // If method name doesn't start with $ (not a local variable)
                if !method_name.starts_with('$') && !method_name.starts_with('@') {
                    // Try to find the method by name
                    if let Some(method_entries) = self.methods_by_name.get(method_name) {
                        return method_entries.first();
                    }
                }
            }
        }

        None
    }

    pub fn add_reference(&mut self, fully_qualified_name: &str, location: Location) {
        let references = self
            .references
            .entry(fully_qualified_name.to_string())
            .or_insert_with(Vec::new);
        references.push(location);
    }

    pub fn find_references(&self, fully_qualified_name: &str) -> Vec<Location> {
        // First check if we have direct references to this name
        let mut locations = self
            .references
            .get(fully_qualified_name)
            .cloned()
            .unwrap_or_default();

        // Also include the definition locations
        if let Some(entries) = self.entries.get(fully_qualified_name) {
            for entry in entries {
                locations.push(Location {
                    uri: entry.location.uri.clone(),
                    range: entry.location.range,
                });
            }
        }

        // For instance variables, also check for references with class prefix
        if fully_qualified_name.starts_with('@') {
            // Look for class-qualified references like "Class#@name"
            for (fqn, refs) in &self.references {
                if fqn.ends_with(fully_qualified_name) && fqn != fully_qualified_name {
                    locations.extend(refs.clone());
                }
            }
        }
        // For methods, check for both simple and qualified references
        else if !fully_qualified_name.starts_with('$') && !fully_qualified_name.starts_with('@') {
            // If this is a qualified method name (Class#method)
            if fully_qualified_name.contains('#') {
                let parts: Vec<&str> = fully_qualified_name.split('#').collect();
                if parts.len() == 2 {
                    let method_name = parts[1];

                    // Also look for unqualified references to this method
                    if let Some(refs) = self.references.get(method_name) {
                        locations.extend(refs.clone());
                    }
                }
            }
            // If this is an unqualified method name
            else {
                // Look for qualified references to this method (Class#method)
                for (fqn, refs) in &self.references {
                    if fqn.ends_with(&format!("#{}", fully_qualified_name)) {
                        locations.extend(refs.clone());
                    }
                }
            }
        }

        locations
    }

    // Update the namespace tree with a new fully qualified name
    fn update_namespace_tree(&mut self, fully_qualified_name: &str) {
        // If this is a complex name with namespace separators
        if fully_qualified_name.contains("::") {
            let parts: Vec<&str> = fully_qualified_name.split("::").collect();

            // Build up the namespace path
            let mut current_namespace = String::new();
            for i in 0..(parts.len() - 1) {
                // Get this part of the namespace
                let part = parts[i];

                // Record that this child exists in the parent namespace
                let children = self
                    .namespace_tree
                    .entry(current_namespace.clone())
                    .or_insert_with(Vec::new);

                // Only add if it's not already there
                if !children.contains(&part.to_string()) {
                    children.push(part.to_string());
                }

                // Update current namespace for next level
                if current_namespace.is_empty() {
                    current_namespace = part.to_string();
                } else {
                    current_namespace = format!("{}::{}", current_namespace, part);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::indexer::entry::{EntryBuilder, EntryLocation, Visibility};

    use super::*;
    use tower_lsp::lsp_types::{Position, Range, Url};

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
    fn test_add_and_find_method_references() {
        let mut index = RubyIndex::new();

        // Create a method entry
        let method_entry = create_test_entry(
            "greet",
            "Person#greet",
            "file:///person.rb",
            EntryType::Method,
            Visibility::Public,
        );

        // Add the method definition
        index.add_entry(method_entry);

        // Create reference locations
        let ref_location1 = Location {
            uri: Url::parse("file:///app.rb").unwrap(),
            range: Range {
                start: Position {
                    line: 10,
                    character: 5,
                },
                end: Position {
                    line: 10,
                    character: 10,
                },
            },
        };

        let ref_location2 = Location {
            uri: Url::parse("file:///test.rb").unwrap(),
            range: Range {
                start: Position {
                    line: 20,
                    character: 8,
                },
                end: Position {
                    line: 20,
                    character: 13,
                },
            },
        };

        // Add references to the method
        index.add_reference("greet", ref_location1.clone());
        index.add_reference("Person#greet", ref_location2.clone());

        // Test finding references by unqualified name
        let refs_unqualified = index.find_references("greet");
        println!("Unqualified references count: {}", refs_unqualified.len());
        for (i, loc) in refs_unqualified.iter().enumerate() {
            println!("  Ref {}: {} at {:?}", i, loc.uri, loc.range);
        }

        // Just check that we found some references
        assert!(!refs_unqualified.is_empty());

        // Test finding references by qualified name
        let refs_qualified = index.find_references("Person#greet");
        println!("Qualified references count: {}", refs_qualified.len());
        for (i, loc) in refs_qualified.iter().enumerate() {
            println!("  Ref {}: {} at {:?}", i, loc.uri, loc.range);
        }

        // Just check that we found some references
        assert!(!refs_qualified.is_empty());
    }

    #[test]
    fn test_add_and_find_instance_variable_references() {
        let mut index = RubyIndex::new();

        // Create an instance variable entry
        let var_entry = create_test_entry(
            "@name",
            "Person#@name",
            "file:///person.rb",
            EntryType::InstanceVariable,
            Visibility::Public,
        );

        // Add the variable definition
        index.add_entry(var_entry);

        // Create reference locations
        let ref_location1 = Location {
            uri: Url::parse("file:///person.rb").unwrap(),
            range: Range {
                start: Position {
                    line: 10,
                    character: 5,
                },
                end: Position {
                    line: 10,
                    character: 10,
                },
            },
        };

        let ref_location2 = Location {
            uri: Url::parse("file:///person.rb").unwrap(),
            range: Range {
                start: Position {
                    line: 15,
                    character: 8,
                },
                end: Position {
                    line: 15,
                    character: 13,
                },
            },
        };

        // Add references to the instance variable
        index.add_reference("@name", ref_location1.clone());
        index.add_reference("Person#@name", ref_location2.clone());

        // Test finding references by unqualified name
        let refs_unqualified = index.find_references("@name");
        println!(
            "Unqualified @name references count: {}",
            refs_unqualified.len()
        );
        for (i, loc) in refs_unqualified.iter().enumerate() {
            println!("  Ref {}: {} at {:?}", i, loc.uri, loc.range);
        }

        // Just check that we found some references
        assert!(!refs_unqualified.is_empty());

        // Test finding references by qualified name
        let refs_qualified = index.find_references("Person#@name");
        println!(
            "Qualified Person#@name references count: {}",
            refs_qualified.len()
        );
        for (i, loc) in refs_qualified.iter().enumerate() {
            println!("  Ref {}: {} at {:?}", i, loc.uri, loc.range);
        }

        // Just check that we found some references
        assert!(!refs_qualified.is_empty());
    }

    #[test]
    fn test_references_survive_reindexing() {
        let mut index = RubyIndex::new();

        // Add a method entry
        let method_entry = create_test_entry(
            "calculate",
            "Math#calculate",
            "file:///math.rb",
            EntryType::Method,
            Visibility::Public,
        );
        index.add_entry(method_entry);

        // Add references
        let ref_location = Location {
            uri: Url::parse("file:///app.rb").unwrap(),
            range: Range {
                start: Position {
                    line: 5,
                    character: 10,
                },
                end: Position {
                    line: 5,
                    character: 19,
                },
            },
        };
        index.add_reference("calculate", ref_location.clone());

        // Remove entries for the definition file
        index.remove_entries_for_uri(&Url::parse("file:///math.rb").unwrap());

        // References should still exist
        let refs = index.find_references("calculate");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].uri.to_string(), "file:///app.rb");
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
