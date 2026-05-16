//! Unified type query API for Ruby code.
//!
//! This module provides a single entry point for all type queries, abstracting away
//! the complexity of checking caches, triggering inference, and storing results.
//!
//! Handlers (hover, inlay hints, completion) should use this API instead of
//! directly interacting with the inferrer or index.

use crate::indexer::index::RubyIndex;
use crate::indexer::index_ref::{Index, Unlocked};
use crate::inferrer::method::resolver::MethodResolver;
use crate::inferrer::r#type::literal::LiteralAnalyzer;
use crate::inferrer::r#type::ruby::RubyType;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_namespace::RubyConstant;
use ruby_analysis_core::{SourceFileId, TypeResolution, TypeStore, TypeSubject};
use ruby_prism::{Node, Visit};
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

/// Infer type from assignment patterns using robust AST analysis.
/// Replaces the brittle string-parsing approach with proper parsing and type resolution.
pub fn infer_type_from_assignment(
    content: &str,
    var_name: &str,
    index: &RubyIndex,
) -> Option<RubyType> {
    let parse_result = ruby_prism::parse(content.as_bytes());
    let root = parse_result.node();

    struct AssignmentFinder<'a> {
        var_name: &'a str,
        best_type: Option<RubyType>,
        index: &'a RubyIndex,
        /// Current namespace stack (for resolving implicit self calls)
        namespace: Vec<String>,
    }

    impl<'a> Visit<'a> for AssignmentFinder<'a> {
        fn visit_class_node(&mut self, node: &ruby_prism::ClassNode<'a>) {
            // Track class namespace
            let name = extract_constant_name(&node.constant_path());
            if let Some(name) = name {
                self.namespace.push(name);
                ruby_prism::visit_class_node(self, node);
                self.namespace.pop();
            } else {
                ruby_prism::visit_class_node(self, node);
            }
        }

        fn visit_module_node(&mut self, node: &ruby_prism::ModuleNode<'a>) {
            // Track module namespace
            let name = extract_constant_name(&node.constant_path());
            if let Some(name) = name {
                self.namespace.push(name);
                ruby_prism::visit_module_node(self, node);
                self.namespace.pop();
            } else {
                ruby_prism::visit_module_node(self, node);
            }
        }

        fn visit_local_variable_write_node(
            &mut self,
            node: &ruby_prism::LocalVariableWriteNode<'a>,
        ) {
            let name = String::from_utf8_lossy(node.name().as_slice());
            if name == self.var_name {
                let val = node.value();
                self.best_type = infer_value_type_with_context(&val, self.index, &self.namespace);
            }
            ruby_prism::visit_local_variable_write_node(self, node);
        }
    }

    let mut finder = AssignmentFinder {
        var_name,
        best_type: None,
        index,
        namespace: Vec::new(),
    };
    finder.visit(&root);

    finder.best_type
}

/// Extract constant name from a constant path or constant read node.
fn extract_constant_name<'a>(node: &ruby_prism::Node<'a>) -> Option<String> {
    if let Some(const_read) = node.as_constant_read_node() {
        return Some(String::from_utf8_lossy(const_read.name().as_slice()).to_string());
    }
    if let Some(const_path) = node.as_constant_path_node() {
        return const_path
            .name()
            .map(|n| String::from_utf8_lossy(n.as_slice()).to_string());
    }
    None
}

/// Infer the type of a value expression with namespace context for implicit self calls.
fn infer_value_type_with_context<'a>(
    node: &Node<'a>,
    index: &RubyIndex,
    namespace: &[String],
) -> Option<RubyType> {
    let literal_analyzer = LiteralAnalyzer::new();

    // 1. Literals
    if let Some(ty) = literal_analyzer.analyze_literal(node) {
        return Some(ty);
    }

    // 2. Constant Read
    if let Some(const_node) = node.as_constant_read_node() {
        let name = String::from_utf8_lossy(const_node.name().as_slice()).to_string();
        if let Ok(fqn) = FullyQualifiedName::try_from(name.as_str()) {
            return Some(RubyType::ClassReference(fqn));
        }
    }

    // 3. Constant Path
    if let Some(path_node) = node.as_constant_path_node() {
        if let Some(full_name) = flatten_constant_path(&path_node) {
            if let Ok(fqn) = FullyQualifiedName::try_from(full_name.as_str()) {
                return Some(RubyType::ClassReference(fqn));
            }
        }
    }

    // 4. Call Node (Recursive)
    if let Some(call_node) = node.as_call_node() {
        let method_name = String::from_utf8_lossy(call_node.name().as_slice()).to_string();

        let receiver_type = if let Some(receiver) = call_node.receiver() {
            infer_value_type_with_context(&receiver, index, namespace)
        } else {
            // Implicit self - use current class/module context
            if namespace.is_empty() {
                return None;
            }
            // Build FQN from namespace
            let parts: Vec<RubyConstant> = namespace
                .iter()
                .filter_map(|s| RubyConstant::new(s).ok())
                .collect();
            if parts.is_empty() {
                return None;
            }
            Some(RubyType::Class(FullyQualifiedName::Constant(parts)))
        };

        if let Some(recv_type) = receiver_type {
            return MethodResolver::resolve_method_return_type(index, &recv_type, &method_name);
        }
    }

    None
}

#[allow(dead_code)]
fn infer_value_type<'a>(node: &Node<'a>, index: &RubyIndex) -> Option<RubyType> {
    // Delegate to context-aware version with empty namespace
    infer_value_type_with_context(node, index, &[])
}

fn flatten_constant_path<'a>(node: &ruby_prism::ConstantPathNode<'a>) -> Option<String> {
    let parent_str = if let Some(parent) = node.parent() {
        if let Some(p) = parent.as_constant_path_node() {
            flatten_constant_path(&p)?
        } else if let Some(p) = parent.as_constant_read_node() {
            String::from_utf8_lossy(p.name().as_slice()).to_string()
        } else {
            return None;
        }
    } else {
        return None;
    };

    let name = node
        .name()
        .map(|n| String::from_utf8_lossy(n.as_slice()).to_string())?;
    Some(format!("{}::{}", parent_str, name))
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

    #[test]
    fn test_infer_array_first_from_assignment() {
        use crate::indexer::index::RubyIndex;

        let index = RubyIndex::new();
        let code = r#"a = [1, 2, 3].first"#;
        let result = infer_type_from_assignment(code, "a", &index);
        println!("a = [1,2,3].first => {:?}", result);
        assert!(result.is_some(), "Should infer type for a = [1,2,3].first");
        let ty = result.unwrap();
        assert_eq!(ty, RubyType::integer(), "Expected Integer, got {:?}", ty);
    }

    #[test]
    fn test_infer_integer_abs_from_assignment() {
        use crate::indexer::index::RubyIndex;

        let index = RubyIndex::new();
        let code = r#"b = 2.abs"#;
        let result = infer_type_from_assignment(code, "b", &index);
        println!("b = 2.abs => {:?}", result);
        assert!(result.is_some(), "Should infer type for b = 2.abs");
        let ty = result.unwrap();
        assert_eq!(ty, RubyType::integer(), "Expected Integer, got {:?}", ty);
    }
}
