//! Unified type query API for Ruby code.
//!
//! This module provides a single entry point for all type queries, abstracting away
//! the complexity of checking caches, triggering inference, and storing results.
//!
//! Handlers (hover, inlay hints, completion) should use this API instead of
//! directly interacting with the inferrer or index.

use crate::indexer::index_ref::{Index, Unlocked};
use crate::inferrer::r#type::ruby::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;
use ruby_analysis_core::{SourceFileId, TypeResolution, TypeStore, TypeSubject};
use tower_lsp::lsp_types::{Position, Range, Url};

/// Unified type query interface.
///
/// Provides methods to query types for various constructs, automatically
/// handling inference and caching.
pub struct TypeQuery<'a> {
    content: &'a [u8],
    type_store: Option<&'a TypeStore>,
    source_file_id: SourceFileId,
}

impl<'a> TypeQuery<'a> {
    /// Create a new TypeQuery for a specific file.
    pub fn new(_index: Index<Unlocked>, _uri: &'a Url, content: &'a [u8]) -> Self {
        Self {
            content,
            type_store: None,
            source_file_id: SourceFileId(0),
        }
    }

    pub fn with_type_store(
        index: Index<Unlocked>,
        uri: &'a Url,
        content: &'a [u8],
        type_store: &'a TypeStore,
    ) -> Self {
        Self::with_type_store_for_file(index, uri, content, type_store, SourceFileId(0))
    }

    pub fn with_type_store_for_file(
        _index: Index<Unlocked>,
        _uri: &'a Url,
        content: &'a [u8],
        type_store: &'a TypeStore,
        source_file_id: SourceFileId,
    ) -> Self {
        Self::with_type_store_snapshot(content, type_store, source_file_id)
    }

    pub fn with_type_store_snapshot(
        content: &'a [u8],
        type_store: &'a TypeStore,
        source_file_id: SourceFileId,
    ) -> Self {
        Self {
            content,
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
        position: Position,
    ) -> Option<RubyType> {
        if let Some(type_store) = self.type_store {
            let byte_offset = position_to_byte_offset(self.content, position)?;
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
    pub fn get_local_variable_type(&self, _name: &str, _position: Position) -> Option<RubyType> {
        None
    }

    pub fn get_local_variable_type_at(
        &self,
        name: &str,
        scope_id: u32,
        position: Position,
    ) -> Option<RubyType> {
        if let Some(type_store) = self.type_store {
            let byte_offset = position_to_byte_offset(self.content, position)?;
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

        self.get_local_variable_type(name, position)
    }

    pub fn get_method_return_type_at(
        &self,
        fqn: &FullyQualifiedName,
        position: Position,
    ) -> Option<RubyType> {
        if let Some(type_store) = self.type_store {
            let byte_offset = position_to_byte_offset(self.content, position)?;
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

    /// Check if a position is within a range.
    #[inline]
    pub fn is_in_range(pos: &Position, range: &Range) -> bool {
        (pos.line > range.start.line
            || (pos.line == range.start.line && pos.character >= range.start.character))
            && (pos.line < range.end.line
                || (pos.line == range.end.line && pos.character <= range.end.character))
    }
}

fn position_to_byte_offset(content: &[u8], position: Position) -> Option<u32> {
    let content = std::str::from_utf8(content).ok()?;
    let mut line = 0u32;
    let mut character = 0u32;

    for (byte_offset, ch) in content.char_indices() {
        if line == position.line && character == position.character {
            return u32::try_from(byte_offset).ok();
        }

        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }
    }

    if line == position.line && character == position.character {
        return u32::try_from(content.len()).ok();
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_in_range() {
        let range = Range {
            start: Position {
                line: 5,
                character: 0,
            },
            end: Position {
                line: 10,
                character: 0,
            },
        };

        // Inside range
        assert!(TypeQuery::is_in_range(
            &Position {
                line: 7,
                character: 5
            },
            &range
        ));

        // At start
        assert!(TypeQuery::is_in_range(
            &Position {
                line: 5,
                character: 0
            },
            &range
        ));

        // At end
        assert!(TypeQuery::is_in_range(
            &Position {
                line: 10,
                character: 0
            },
            &range
        ));

        // Before range
        assert!(!TypeQuery::is_in_range(
            &Position {
                line: 4,
                character: 0
            },
            &range
        ));

        // After range
        assert!(!TypeQuery::is_in_range(
            &Position {
                line: 11,
                character: 0
            },
            &range
        ));
    }
}
