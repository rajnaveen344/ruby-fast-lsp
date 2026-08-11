//! Unified type query API for Ruby code.
//!
//! This module provides a single entry point for all type queries, abstracting away
//! the complexity of checking caches, triggering inference, and storing results.
//!
//! Handlers (hover, inlay hints, completion) should use this API instead of
//! directly interacting with inference internals.

use crate::core::FullyQualifiedName;
use crate::core::{SourceFileId, TypeResolution, TypeStore, TypeSubject};
use crate::inference::RubyType;

/// Unified type query interface.
///
/// Provides methods to query types for various constructs, automatically
/// handling inference and caching.
pub struct TypeQuery<'a> {
    type_store: Option<&'a TypeStore>,
    source_file_id: SourceFileId,
}

impl<'a> TypeQuery<'a> {
    /// Create a query without a semantic snapshot.
    pub fn new() -> Self {
        Self {
            type_store: None,
            source_file_id: SourceFileId(0),
        }
    }

    pub fn with_type_store(type_store: &'a TypeStore) -> Self {
        Self::with_type_store_for_file(type_store, SourceFileId(0))
    }

    pub fn with_type_store_for_file(
        type_store: &'a TypeStore,
        source_file_id: SourceFileId,
    ) -> Self {
        Self {
            type_store: Some(type_store),
            source_file_id,
        }
    }

    /// Get the value type for a constant assignment.
    ///
    /// Class/module constants still fall back to ClassReference/ModuleReference when
    /// there is no value-constant entry, but `A = 1` returns `Integer`.
    pub fn get_constant_type(&self, fqn: &FullyQualifiedName) -> Option<RubyType> {
        if let Some(type_store) = self.type_store {
            return type_store
                .facts_for(&TypeSubject::Constant(fqn.clone()))
                .iter()
                .filter(|fact| fact.range.file_id == self.source_file_id)
                .next_back()
                .map(|fact| fact.ruby_type.clone());
        }

        None
    }

    pub fn get_constant_type_at(
        &self,
        fqn: &FullyQualifiedName,
        byte_offset: u32,
    ) -> Option<RubyType> {
        if let Some(type_store) = self.type_store {
            match type_store.type_at(
                &TypeSubject::Constant(fqn.clone()),
                self.source_file_id,
                byte_offset,
            ) {
                TypeResolution::Resolved(fact) => return Some(fact.ruby_type),
                TypeResolution::Ambiguous(_) => return None,
                TypeResolution::Unresolved => return None,
            }
        }

        None
    }

    /// Get type for a local variable by name at a position.
    /// Checks method parameters first, then falls back to assignment inference.
    pub fn get_local_variable_type(&self, _name: &str, _byte_offset: u32) -> Option<RubyType> {
        None
    }

    pub fn get_local_variable_type_at(
        &self,
        name: &str,
        scope_id: u32,
        byte_offset: u32,
    ) -> Option<RubyType> {
        if let Some(type_store) = self.type_store {
            match type_store.type_at(
                &TypeSubject::Local {
                    scope_id,
                    name: name.to_string(),
                },
                self.source_file_id,
                byte_offset,
            ) {
                TypeResolution::Resolved(fact) => return Some(fact.ruby_type),
                TypeResolution::Ambiguous(_) => return None,
                TypeResolution::Unresolved => {}
            }
            return type_store
                .facts_in_file(self.source_file_id)
                .into_iter()
                .filter(|fact| fact.range.start_byte <= byte_offset)
                .filter_map(|fact| match &fact.subject {
                    TypeSubject::Parameter {
                        name: fact_name, ..
                    } if fact_name == name && fact.ruby_type != RubyType::Unknown => Some(fact),
                    TypeSubject::Constant(_)
                    | TypeSubject::Local { .. }
                    | TypeSubject::InstanceVariable { .. }
                    | TypeSubject::ClassVariable { .. }
                    | TypeSubject::GlobalVariable(_)
                    | TypeSubject::MethodReturn(_)
                    | TypeSubject::Parameter { .. }
                    | TypeSubject::Expression(_) => None,
                })
                .max_by_key(|fact| fact.range.start_byte)
                .map(|fact| fact.ruby_type);
        }

        self.get_local_variable_type(name, byte_offset)
    }

    pub fn get_method_return_type_at(
        &self,
        fqn: &FullyQualifiedName,
        byte_offset: u32,
    ) -> Option<RubyType> {
        if let Some(type_store) = self.type_store {
            match type_store.type_at(
                &TypeSubject::MethodReturn(fqn.clone()),
                self.source_file_id,
                byte_offset,
            ) {
                TypeResolution::Resolved(fact) => return Some(fact.ruby_type),
                TypeResolution::Ambiguous(_) => return None,
                TypeResolution::Unresolved => {}
            }
        }

        None
    }
}

impl Default for TypeQuery<'_> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{TextRange, TypeFact, TypeProvenance};

    #[test]
    fn query_uses_domain_byte_offsets_without_source_or_protocol_coordinates() {
        let file_id = SourceFileId(7);
        let range = TextRange::new(file_id, 4, 9);
        let fqn = FullyQualifiedName::constant(vec![crate::RubyConstant::new("VALUE").unwrap()]);
        let mut store = TypeStore::new();
        store.add(TypeFact::new(
            TypeSubject::Constant(fqn.clone()),
            RubyType::string(),
            range,
            TypeProvenance::Inferred,
        ));

        let query = TypeQuery::with_type_store_for_file(&store, file_id);
        assert_eq!(
            query.get_constant_type_at(&fqn, 6),
            Some(RubyType::string())
        );
        assert_eq!(query.get_constant_type_at(&fqn, 2), None);
    }
}
