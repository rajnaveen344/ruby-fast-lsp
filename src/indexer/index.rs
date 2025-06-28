use std::collections::HashMap;

use log::debug;
use lsp_types::{Location, Url};

use crate::indexer::entry::{entry_kind::EntryKind, Entry};
use crate::types::{fully_qualified_name::FullyQualifiedName, ruby_method::RubyMethod};

#[derive(Debug)]
pub struct RubyIndex {
    // File to entries map
    // Eg. file:///test.rb => [Entry1, Entry2, ...]
    pub file_entries: HashMap<Url, Vec<Entry>>,

    // Namespace ancestors are the ancestors of a namespace.
    // For example, if we have a namespace Foo::Bar, its ancestors are [Foo].
    // Eg. Foo::Bar.ancestors = [Foo], Foo.ancestors = [], Foo::Bar::Baz.ancestors = [Foo::Bar, Foo]
    pub namespace_ancestors: HashMap<FullyQualifiedName, Vec<FullyQualifiedName>>,

    // Definitions are the definitions of a fully qualified name.
    // For example, if we have a method Foo#bar, its definition is the method definition.
    pub definitions: HashMap<FullyQualifiedName, Vec<Entry>>,

    // References are the references to a fully qualified name.
    pub references: HashMap<FullyQualifiedName, Vec<Location>>,

    // Temporarily used to find definitions by name until we have logic to determine the type of the receiver
    // For example, if we have a method Foo#bar, its method by name is bar.
    pub methods_by_name: HashMap<RubyMethod, Vec<Entry>>,
}

impl RubyIndex {
    pub fn new() -> Self {
        RubyIndex {
            file_entries: HashMap::new(),
            namespace_ancestors: HashMap::new(),
            definitions: HashMap::new(),
            references: HashMap::new(),
            methods_by_name: HashMap::new(),
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
}
