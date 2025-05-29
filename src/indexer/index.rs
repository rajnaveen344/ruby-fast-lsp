use std::collections::HashMap;

use lsp_types::{Location, Url};

use crate::indexer::entry::{entry_kind::EntryKind, Entry, Mixin};
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

    // Mixins to support include, extend, and prepend helpers
    pub mixin_relationships: HashMap<FullyQualifiedName, Vec<Mixin>>,

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
            mixin_relationships: HashMap::new(),
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

    pub fn remove_entries_for_uri(&mut self, uri: &Url) {
        // If no entries for this URI, return early
        if !self.file_entries.contains_key(uri) {
            return;
        }

        // Collect all entries
        let entries = self.file_entries.remove(uri).unwrap();

        // Remove each entry from the definitions map
        for entry in entries {
            if let Some(fqn_entries) = self.definitions.get_mut(&entry.fqn) {
                fqn_entries.retain(|e| e.location.uri != *uri);

                if fqn_entries.is_empty() {
                    self.definitions.remove(&entry.fqn);
                }
            }
        }

        // Remove all references mapped to this URI
        self.references
            .retain(|_, refs| refs.iter().all(|loc| loc.uri != *uri));
    }

    // Add a reference to a symbol
    pub fn add_reference(&mut self, fully_qualified_name: FullyQualifiedName, location: Location) {
        self.references
            .entry(fully_qualified_name)
            .or_insert_with(Vec::new)
            .push(location);
    }
}
