//! Definition Query - Find where symbols are defined
//!
//! Consolidates definition logic from `capabilities/definitions/`.

use crate::analyzer_prism::{Identifier, RubyPrismAnalyzer};
use crate::indexer::entry::EntryKind;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_namespace::RubyConstant;
use crate::yard::YardParser;
use log::info;
use tower_lsp::lsp_types::{Location, Position, Url};

use super::IndexQuery;

impl IndexQuery {
    /// Find definitions for an identifier at the given position.
    ///
    /// This handles all identifier types:
    /// - Constants (classes, modules)
    /// - Methods (instance and class methods)
    /// - Variables (local, instance, class, global)
    /// - YARD type references
    pub fn find_definitions_at_position(
        &self,
        uri: &Url,
        position: Position,
        content: &str,
    ) -> Option<Vec<Location>> {
        // First check if we're in a YARD comment type reference
        if let Some(yard_type) = YardParser::find_type_at_position(content, position) {
            info!("Found YARD type at position: {}", yard_type.type_name);
            return self.find_yard_type_definitions(&yard_type.type_name);
        }

        let analyzer = RubyPrismAnalyzer::new(uri.clone(), content.to_string());
        let (identifier, _, ancestors, _scope_stack) = analyzer.get_identifier(position);

        let identifier = match identifier {
            Some(id) => id,
            None => {
                info!("No identifier found at position {:?}", position);
                return None;
            }
        };

        info!(
            "Looking for definition of: {}->{}",
            FullyQualifiedName::from(ancestors.clone()),
            identifier,
        );

        self.find_definitions_for_identifier(
            &identifier,
            &ancestors,
            position,
            uri,
            content,
        )
    }

    /// Find definitions for a local variable using document.lvars (file-local storage)
    fn find_local_variable_definitions_at_position(
        &self,
        name: &str,
        scope_id: crate::types::scope::LVScopeId,
        position: Position,
    ) -> Option<Vec<Location>> {
        let doc_arc = self.doc.as_ref()?;
        let document = doc_arc.read();

        // Try exact scope ID match with position filter
        if let Some(entries) = document.get_local_var_entries(scope_id) {
            for entry in entries {
                if let EntryKind::LocalVariable(data) = &entry.kind {
                    // Ensure we find the definition that is BEFORE the usage
                    if &data.name == name && entry.location.range.start < position {
                        let loc = Location {
                            uri: document.uri.clone(),
                            range: entry.location.range,
                        };
                        return Some(vec![loc]);
                    }
                }
            }
        }

        // Fallback: search all scopes in the document for this variable name
        if let Some(location) = document.find_local_var_by_name(name) {
            if location.range.start < position {
                return Some(vec![location]);
            }
        }

        None
    }

    /// Find definitions for a global variable.
    fn find_global_variable_definitions(&self, name: &str) -> Option<Vec<Location>> {
        if let Ok(fqn) = FullyQualifiedName::global_variable(name.to_string()) {
            return self.find_variable_definitions(&fqn);
        }
        None
    }

    /// Find definitions for a constant (class or module) by path.
    fn find_constant_definitions_by_path(
        &self,
        constant_path: &[RubyConstant],
        ancestors: &[RubyConstant],
    ) -> Option<Vec<Location>> {
        let fqn = self.resolve_constant_fqn(constant_path, ancestors);
        info!("Resolved constant FQN: {}", fqn);

        let index = self.index.lock();
        let entries = index.get(&fqn)?;

        // Filter for definition entries (classes or modules)
        let locations: Vec<Location> = entries
            .iter()
            .filter(|e| {
                matches!(
                    e.kind,
                    EntryKind::Class(_) | EntryKind::Module(_) | EntryKind::Constant(_)
                )
            })
            .filter_map(|e| index.to_lsp_location(&e.location))
            .collect();

        if locations.is_empty() {
            None
        } else {
            Some(locations)
        }
    }
}

// Private helpers
impl IndexQuery {
    /// Find definitions for a given identifier.
    fn find_definitions_for_identifier(
        &self,
        identifier: &Identifier,
        ancestors: &[RubyConstant],
        position: Position,
        uri: &Url,
        content: &str,
    ) -> Option<Vec<Location>> {
        match identifier {
            Identifier::RubyConstant { namespace: _, iden } => {
                // iden is Vec<RubyConstant> - the full constant path being referenced
                self.find_constant_definitions_by_path(iden, ancestors)
            }
            Identifier::RubyMethod {
                namespace: _,
                receiver,
                iden,
            } => {
                self.find_method_definitions(
                    receiver, iden, ancestors, uri, position, content,
                )
            }
            Identifier::RubyInstanceVariable { name, .. } => {
                self.find_instance_variable_definitions(name)
            }
            Identifier::RubyClassVariable { name, .. } => {
                self.find_class_variable_definitions(name)
            }
            Identifier::RubyGlobalVariable { name, .. } => {
                self.find_global_variable_definitions(name)
            }
            Identifier::RubyLocalVariable { name, scope, .. } => {
                self.find_local_variable_definitions_at_position(name, *scope, position)
            }
            Identifier::YardType { type_name, .. } => self.find_yard_type_definitions(type_name),
        }
    }

    /// Find definitions for a YARD type reference string (e.g., "String", "Foo::Bar").
    fn find_yard_type_definitions(&self, type_name: &str) -> Option<Vec<Location>> {
        // Handle built-in types
        let builtins = ["nil", "true", "false", "void", "Boolean", "bool"];
        if builtins.iter().any(|b| b.eq_ignore_ascii_case(type_name)) {
            return None;
        }

        // Parse path
        let parts: Vec<&str> = type_name.split("::").collect();
        let mut namespace = Vec::new();
        for part in parts {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Ok(constant) = RubyConstant::try_from(trimmed) {
                namespace.push(constant);
            } else {
                return None;
            }
        }

        if namespace.is_empty() {
            return None;
        }

        // Find definition
        let fqn = FullyQualifiedName::Constant(namespace);
        let index = self.index.lock();
        index.get(&fqn).map(|entries| {
            entries
                .iter()
                .filter(|e| matches!(e.kind, EntryKind::Class(_) | EntryKind::Module(_)))
                .filter_map(|e| index.to_lsp_location(&e.location))
                .collect()
        })
    }

    /// Find method definitions by name.
    #[allow(dead_code)]
    fn find_method_definitions_by_name(
        &self,
        method_name: &crate::types::ruby_method::RubyMethod,
        ancestors: &[RubyConstant],
    ) -> Option<Vec<Location>> {
        let index = self.index.lock();

        // First, look for methods in the current class/module ancestry
        let context_fqn = FullyQualifiedName::from(ancestors.to_vec());
        let ancestor_chain = index.get_ancestor_chain(&context_fqn, false);

        for ancestor_fqn in &ancestor_chain {
            if let Some(entries) = index.get_methods_by_name(method_name) {
                let locations: Vec<Location> = entries
                    .iter()
                    .filter(|e| {
                        // Check if method belongs to this ancestor
                        if let Some(fqn) = index.get_fqn(e.fqn_id) {
                            fqn.namespace_parts() == ancestor_fqn.namespace_parts()
                        } else {
                            false
                        }
                    })
                    .filter_map(|e| index.to_lsp_location(&e.location))
                    .collect();

                if !locations.is_empty() {
                    return Some(locations);
                }
            }
        }

        // Fallback: return any method with this name
        index.get_methods_by_name(method_name).map(|entries| {
            entries
                .iter()
                .filter_map(|e| index.to_lsp_location(&e.location))
                .collect()
        })
    }

    /// Find instance variable definitions.
    fn find_instance_variable_definitions(&self, name: &str) -> Option<Vec<Location>> {
        // Instance variables are stored with just their name (e.g., "@foo")
        if let Ok(fqn) = FullyQualifiedName::instance_variable(name.to_string()) {
            let index = self.index.lock();
            return index.get(&fqn).map(|entries| {
                entries
                    .iter()
                    .filter_map(|e| index.to_lsp_location(&e.location))
                    .collect()
            });
        }
        None
    }

    /// Find class variable definitions.
    fn find_class_variable_definitions(&self, name: &str) -> Option<Vec<Location>> {
        // Class variables are stored with just their name (e.g., "@@foo")
        if let Ok(fqn) = FullyQualifiedName::class_variable(name.to_string()) {
            let index = self.index.lock();
            return index.get(&fqn).map(|entries| {
                entries
                    .iter()
                    .filter_map(|e| index.to_lsp_location(&e.location))
                    .collect()
            });
        }
        None
    }

    /// Resolve constant FQN from path.
    fn resolve_constant_fqn(
        &self,
        constant_path: &[RubyConstant],
        ancestors: &[RubyConstant],
    ) -> FullyQualifiedName {
        let index = self.index.lock();
        let mut current_context = ancestors.to_vec();

        // 1. Iteratively check scopes from most specific to least specific
        loop {
            let mut probe_ns = current_context.clone();
            probe_ns.extend(constant_path.iter().cloned());
            let probe_fqn = FullyQualifiedName::Constant(probe_ns);

            // If found in index (and is a Class/Module/Constant?), return it
            if index.get(&probe_fqn).is_some() {
                return probe_fqn;
            }

            if current_context.is_empty() {
                break;
            }
            current_context.pop();
        }

        // 2. Default to just the path (absolute/toplevel)
        // This acts as the final fallback if not found in any scope (or if defined at toplevel but not indexed yet?)
        FullyQualifiedName::Constant(constant_path.to_vec())
    }

    /// Find variable definitions by FQN.
    fn find_variable_definitions(&self, fqn: &FullyQualifiedName) -> Option<Vec<Location>> {
        let index = self.index.lock();
        index.get(fqn).map(|entries| {
            entries
                .iter()
                .filter_map(|e| index.to_lsp_location(&e.location))
                .collect()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::index::RubyIndex;
    use crate::indexer::index_ref::Index;
    use parking_lot::Mutex;
    use std::sync::Arc;

    fn create_test_query() -> IndexQuery {
        let index = RubyIndex::new();
        let index_ref = Index::new(Arc::new(Mutex::new(index)));
        IndexQuery::new(index_ref)
    }

    #[test]
    fn test_builtin_types_return_none() {
        let query = create_test_query();
        assert!(query.find_yard_type_definitions("nil").is_none());
        assert!(query.find_yard_type_definitions("true").is_none());
        assert!(query.find_yard_type_definitions("false").is_none());
        assert!(query.find_yard_type_definitions("void").is_none());
        assert!(query.find_yard_type_definitions("Boolean").is_none());
    }

    #[test]
    fn test_empty_type_returns_none() {
        let query = create_test_query();
        assert!(query.find_yard_type_definitions("").is_none());
        assert!(query.find_yard_type_definitions("  ").is_none());
    }

    #[test]
    fn test_invalid_constant_returns_none() {
        let query = create_test_query();
        // lowercase names are not valid constants
        assert!(query.find_yard_type_definitions("lowercase").is_none());
    }
}
