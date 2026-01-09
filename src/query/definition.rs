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
use crate::inferrer::TypeNarrowingEngine;

impl IndexQuery<'_> {
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
        type_narrowing: Option<&TypeNarrowingEngine>,
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
            type_narrowing,
            uri,
            content,
        )
    }

    /// Find definitions for a given identifier.
    fn find_definitions_for_identifier(
        &self,
        identifier: &Identifier,
        ancestors: &[RubyConstant],
        position: Position,
        type_narrowing: Option<&TypeNarrowingEngine>,
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
                if let Some(narrowing) = type_narrowing {
                    self.find_method_definitions(
                        receiver, iden, ancestors, narrowing, uri, position, content,
                    )
                } else {
                    // Fallback if no narrowing engine provided
                    self.find_method_definitions_by_name(iden, ancestors)
                }
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
                if let Some(doc) = self.doc {
                    self.find_local_variable_definitions_at_position(name, *scope, doc, position)
                } else {
                    None
                }
            }
            Identifier::YardType { type_name, .. } => self.find_yard_type_definitions(type_name),
        }
    }

    /// Find definitions for a local variable using document.lvars (file-local storage)
    pub fn find_local_variable_definitions_at_position(
        &self,
        name: &str,
        scope_id: crate::types::scope::LVScopeId,
        document: &crate::types::ruby_document::RubyDocument,
        position: Position,
    ) -> Option<Vec<Location>> {
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

    /// Find constant definitions by path within ancestry scope.
    ///
    /// The `constant_path` is the path being referenced (e.g., `["Inner", "CONST_A"]` for `Inner::CONST_A`).
    /// The `ancestors` is the current namespace context (e.g., `["Outer"]` when inside `module Outer`).
    pub fn find_constant_definitions_by_path(
        &self,
        constant_path: &[RubyConstant],
        ancestors: &[RubyConstant],
    ) -> Option<Vec<Location>> {
        // Try combining ancestors with the constant path
        // Start with full scope: ancestors + constant_path
        let mut search_parts = ancestors.to_vec();
        search_parts.extend(constant_path.iter().cloned());

        loop {
            let fqn = FullyQualifiedName::Constant(search_parts.clone());
            if let Some(entries) = self.index.get(&fqn) {
                let locations: Vec<Location> = entries
                    .iter()
                    .filter_map(|e| self.index.to_lsp_location(&e.location))
                    .collect();
                if !locations.is_empty() {
                    return Some(locations);
                }
            }

            // Try without the first ancestor (scope resolution - walk up)
            if search_parts.len() > constant_path.len() {
                search_parts.remove(0);
            } else {
                break;
            }
        }

        // Try just the constant path itself (absolute reference)
        let fqn = FullyQualifiedName::Constant(constant_path.to_vec());
        if let Some(entries) = self.index.get(&fqn) {
            let locations: Vec<Location> = entries
                .iter()
                .filter_map(|e| self.index.to_lsp_location(&e.location))
                .collect();
            if !locations.is_empty() {
                return Some(locations);
            }
        }

        None
    }

    /// Find definitions for a YARD type reference string (e.g., "String", "Foo::Bar").
    pub fn find_yard_type_definitions(&self, type_name: &str) -> Option<Vec<Location>> {
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
        self.index.get(&fqn).map(|entries| {
            entries
                .iter()
                .filter(|e| matches!(e.kind, EntryKind::Class(_) | EntryKind::Module(_)))
                .filter_map(|e| self.index.to_lsp_location(&e.location))
                .collect()
        })
    }

    /// Find method definitions by name.
    pub fn find_method_definitions_by_name(
        &self,
        method_name: &crate::types::ruby_method::RubyMethod,
        ancestors: &[RubyConstant],
    ) -> Option<Vec<Location>> {
        // First, look for methods in the current class/module ancestry
        let context_fqn = FullyQualifiedName::from(ancestors.to_vec());
        let ancestor_chain = self.index.get_ancestor_chain(&context_fqn, false);

        for ancestor_fqn in &ancestor_chain {
            if let Some(entries) = self.index.get_methods_by_name(method_name) {
                let locations: Vec<Location> = entries
                    .iter()
                    .filter(|e| {
                        // Check if method belongs to this ancestor
                        if let Some(fqn) = self.index.get_fqn(e.fqn_id) {
                            fqn.namespace_parts() == ancestor_fqn.namespace_parts()
                        } else {
                            false
                        }
                    })
                    .filter_map(|e| self.index.to_lsp_location(&e.location))
                    .collect();

                if !locations.is_empty() {
                    return Some(locations);
                }
            }
        }

        // Fallback: return any method with this name
        self.index.get_methods_by_name(method_name).map(|entries| {
            entries
                .iter()
                .filter_map(|e| self.index.to_lsp_location(&e.location))
                .collect()
        })
    }

    /// Find instance variable definitions.
    pub fn find_instance_variable_definitions(&self, name: &str) -> Option<Vec<Location>> {
        // Instance variables are stored with just their name (e.g., "@foo")
        if let Ok(fqn) = FullyQualifiedName::instance_variable(name.to_string()) {
            return self.index.get(&fqn).map(|entries| {
                entries
                    .iter()
                    .filter_map(|e| self.index.to_lsp_location(&e.location))
                    .collect()
            });
        }
        None
    }

    /// Find class variable definitions.
    pub fn find_class_variable_definitions(&self, name: &str) -> Option<Vec<Location>> {
        // Class variables are stored with just their name (e.g., "@@foo")
        if let Ok(fqn) = FullyQualifiedName::class_variable(name.to_string()) {
            return self.index.get(&fqn).map(|entries| {
                entries
                    .iter()
                    .filter_map(|e| self.index.to_lsp_location(&e.location))
                    .collect()
            });
        }
        None
    }

    /// Find global variable definitions.
    pub fn find_global_variable_definitions(&self, name: &str) -> Option<Vec<Location>> {
        // Global variables are stored with just their name (e.g., "$foo")
        if let Ok(fqn) = FullyQualifiedName::global_variable(name.to_string()) {
            return self.index.get(&fqn).map(|entries| {
                entries
                    .iter()
                    .filter_map(|e| self.index.to_lsp_location(&e.location))
                    .collect()
            });
        }
        None
    }

    /// Find definition by FQN directly.
    pub fn find_definition_by_fqn(&self, fqn: &FullyQualifiedName) -> Option<Location> {
        self.index
            .get(fqn)?
            .first()
            .and_then(|e| self.index.to_lsp_location(&e.location))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexer::index::RubyIndex;

    #[test]
    fn test_builtin_types_return_none() {
        let index = RubyIndex::new();
        let query = IndexQuery::new(&index);
        assert!(query.find_yard_type_definitions("nil").is_none());
        assert!(query.find_yard_type_definitions("true").is_none());
        assert!(query.find_yard_type_definitions("false").is_none());
        assert!(query.find_yard_type_definitions("void").is_none());
        assert!(query.find_yard_type_definitions("Boolean").is_none());
    }

    #[test]
    fn test_empty_type_returns_none() {
        let index = RubyIndex::new();
        let query = IndexQuery::new(&index);
        assert!(query.find_yard_type_definitions("").is_none());
        assert!(query.find_yard_type_definitions("  ").is_none());
    }

    #[test]
    fn test_invalid_constant_returns_none() {
        let index = RubyIndex::new();
        let query = IndexQuery::new(&index);
        // lowercase names are not valid constants
        assert!(query.find_yard_type_definitions("lowercase").is_none());
    }
}
