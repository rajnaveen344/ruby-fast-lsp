//! Unified type query API for Ruby code.
//!
//! This module provides a single entry point for all type queries, abstracting away
//! the complexity of checking caches, triggering inference, and storing results.
//!
//! Handlers (hover, inlay hints, completion) should use this API instead of
//! directly interacting with the inferrer or index.

use crate::indexer::entry::entry_kind::EntryKind;
use crate::indexer::index::EntryId;
use crate::indexer::index_ref::{Index, Unlocked};
use crate::inferrer::method::resolver::MethodResolver;
use crate::inferrer::r#type::ruby::RubyType;
use crate::inferrer::return_type::ReturnTypeInferrer;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_namespace::RubyConstant;
use tower_lsp::lsp_types::{Position, Range, Url};

/// A type hint to display (for inlay hints, hover, etc.)
#[derive(Debug, Clone)]
pub struct TypeHint {
    /// Where to show the hint (end of the identifier)
    pub position: Position,
    /// The inferred or declared type
    pub ruby_type: RubyType,
    /// What kind of construct this type is for
    pub kind: TypeHintKind,
    /// Optional tooltip text (e.g., YARD description)
    pub tooltip: Option<String>,
}

/// The kind of construct a type hint is for
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeHintKind {
    /// Method return type: `def foo -> String`
    MethodReturn,
    /// Local variable: `x = "hello"` → `x: String`
    LocalVariable,
    /// Instance variable: `@name = "bob"` → `@name: String`
    InstanceVariable,
    /// Class variable: `@@count = 0` → `@@count: Integer`
    ClassVariable,
    /// Global variable: `$debug = true` → `$debug: TrueClass`
    GlobalVariable,
    /// Method parameter from YARD: `def foo(x)` → `x: Integer`
    MethodParameter,
}

/// Unified type query interface.
///
/// Provides methods to query types for various constructs, automatically
/// handling inference and caching.
pub struct TypeQuery<'a> {
    index: Index<Unlocked>,
    uri: &'a Url,
    content: &'a [u8],
}

impl<'a> TypeQuery<'a> {
    /// Create a new TypeQuery for a specific file.
    pub fn new(index: Index<Unlocked>, uri: &'a Url, content: &'a [u8]) -> Self {
        Self {
            index,
            uri,
            content,
        }
    }

    /// Get all type hints in a range (for inlay hints).
    ///
    /// Returns types for:
    /// - Method return types
    /// - Local variables
    /// - Instance/class/global variables
    /// - Method parameters (from YARD)
    pub fn get_types_in_range(&mut self, range: &Range) -> Vec<TypeHint> {
        let mut hints = Vec::new();

        // Collect method hints (with inference)
        hints.extend(self.get_method_hints_in_range(range));

        // Collect variable hints from index
        hints.extend(self.get_variable_hints_in_range(range));

        // Collect local variable hints (from document lvars - handled separately)
        // Note: Local variables are stored in the Document, not the Index,
        // so they need to be passed in or queried separately

        hints
    }

    /// Get type at a specific position (for hover).
    ///
    /// Returns the type of whatever construct is at the given position.
    pub fn get_type_at(&mut self, position: Position) -> Option<RubyType> {
        // First check if there's a method at this position
        if let Some(ty) = self.get_method_type_at(position) {
            return Some(ty);
        }

        // Check for variables at this position
        if let Some(ty) = self.get_variable_type_at(position) {
            return Some(ty);
        }

        None
    }

    /// Get method return type hints in a range.
    fn get_method_hints_in_range(&mut self, range: &Range) -> Vec<TypeHint> {
        let mut hints = Vec::new();

        // Collect methods needing inference
        let methods_to_process: Vec<(
            EntryId,
            Position,
            Option<RubyType>,
            Option<String>,
            Option<String>,
        )> = {
            let index = self.index.lock();
            index
                .get_entry_ids_for_uri(self.uri)
                .iter()
                .filter_map(|&entry_id| {
                    let entry = index.get_entry(entry_id)?;
                    if !Self::is_in_range(&entry.location.range.start, range) {
                        return None;
                    }

                    if let EntryKind::Method(data) = &entry.kind {
                        let return_type_pos = data.return_type_position?;
                        let yard_return =
                            data.yard_doc.as_ref().and_then(|d| d.format_return_type());
                        let yard_desc = data
                            .yard_doc
                            .as_ref()
                            .and_then(|d| d.get_return_description().cloned());
                        Some((
                            entry_id,
                            return_type_pos,
                            data.return_type.clone(),
                            yard_return,
                            yard_desc,
                        ))
                    } else {
                        None
                    }
                })
                .collect()
        };

        // Process each method
        for (entry_id, pos, existing_type, yard_return, yard_desc) in methods_to_process {
            let ruby_type = if let Some(ty) = existing_type {
                // Already have a type (from RBS, YARD, or previous inference)
                ty
            } else {
                // Need to infer - try to get the type
                if let Some(ty) = self.infer_method_return_type(entry_id, pos.line) {
                    // Cache the result
                    let mut index = self.index.lock();
                    index.update_method_return_type(entry_id, ty.clone());
                    ty
                } else if yard_return.is_some() {
                    // Fall back to YARD string - but we can't parse it as RubyType yet
                    // Skip for now
                    continue;
                } else {
                    continue;
                }
            };

            hints.push(TypeHint {
                position: pos,
                ruby_type,
                kind: TypeHintKind::MethodReturn,
                tooltip: yard_desc,
            });
        }

        hints
    }

    /// Get variable type hints in a range.
    fn get_variable_hints_in_range(&mut self, range: &Range) -> Vec<TypeHint> {
        let mut hints = Vec::new();

        let index = self.index.lock();
        let entries = index.file_entries(self.uri);

        for entry in entries {
            if !Self::is_in_range(&entry.location.range.start, range) {
                continue;
            }

            let (ruby_type, kind) = match &entry.kind {
                EntryKind::InstanceVariable(data) if data.r#type != RubyType::Unknown => {
                    (data.r#type.clone(), TypeHintKind::InstanceVariable)
                }
                EntryKind::ClassVariable(data) if data.r#type != RubyType::Unknown => {
                    (data.r#type.clone(), TypeHintKind::ClassVariable)
                }
                EntryKind::GlobalVariable(data) if data.r#type != RubyType::Unknown => {
                    (data.r#type.clone(), TypeHintKind::GlobalVariable)
                }
                _ => continue,
            };

            hints.push(TypeHint {
                position: entry.location.range.end,
                ruby_type,
                kind,
                tooltip: None,
            });
        }

        hints
    }

    /// Get method type at a specific position.
    fn get_method_type_at(&mut self, position: Position) -> Option<RubyType> {
        let (entry_id, existing_type, line) = {
            let index = self.index.lock();
            let entry_id = index
                .get_entry_ids_for_uri(self.uri)
                .iter()
                .find(|&&eid| {
                    if let Some(entry) = index.get_entry(eid) {
                        Self::position_in_entry_range(position, &entry.location.range)
                    } else {
                        false
                    }
                })
                .copied()?;

            let entry = index.get_entry(entry_id)?;
            if let EntryKind::Method(data) = &entry.kind {
                (
                    entry_id,
                    data.return_type.clone(),
                    entry.location.range.start.line,
                )
            } else {
                return None;
            }
        };

        if let Some(ty) = existing_type {
            return Some(ty);
        }

        // Need to infer
        if let Some(ty) = self.infer_method_return_type(entry_id, line) {
            let mut index = self.index.lock();
            index.update_method_return_type(entry_id, ty.clone());
            Some(ty)
        } else {
            None
        }
    }

    /// Get variable type at a specific position.
    fn get_variable_type_at(&self, position: Position) -> Option<RubyType> {
        let index = self.index.lock();
        let entries = index.file_entries(self.uri);

        for entry in entries {
            if !Self::position_in_entry_range(position, &entry.location.range) {
                continue;
            }

            let ruby_type = match &entry.kind {
                EntryKind::InstanceVariable(data) => Some(data.r#type.clone()),
                EntryKind::ClassVariable(data) => Some(data.r#type.clone()),
                EntryKind::GlobalVariable(data) => Some(data.r#type.clone()),
                _ => None,
            };

            if let Some(ty) = ruby_type {
                if ty != RubyType::Unknown {
                    return Some(ty);
                }
            }
        }

        None
    }

    /// Infer return type for a method at the given line.
    /// Parses the file and finds the DefNode, then infers its return type.
    fn infer_method_return_type(&self, _entry_id: EntryId, line: u32) -> Option<RubyType> {
        // Parse the file and find the DefNode at this line
        let parse_result = ruby_prism::parse(self.content);
        let node = parse_result.node();
        let def_node = find_def_node_recursive(&node, line, self.content)?;

        // Create inferrer and infer the return type
        let inferrer =
            ReturnTypeInferrer::new_with_content(self.index.clone(), self.content, self.uri);
        inferrer.infer_return_type(self.content, &def_node)
    }

    /// Check if a position is within a range.
    #[inline]
    pub fn is_in_range(pos: &Position, range: &Range) -> bool {
        (pos.line > range.start.line
            || (pos.line == range.start.line && pos.character >= range.start.character))
            && (pos.line < range.end.line
                || (pos.line == range.end.line && pos.character <= range.end.character))
    }

    /// Check if a position is within an entry's range.
    #[inline]
    fn position_in_entry_range(pos: Position, range: &Range) -> bool {
        pos.line >= range.start.line && pos.line <= range.end.line
    }
}

/// Find a DefNode at the given line in the AST.
fn find_def_node_recursive<'a>(
    node: &ruby_prism::Node<'a>,
    target_line: u32,
    content: &[u8],
) -> Option<ruby_prism::DefNode<'a>> {
    if let Some(def_node) = node.as_def_node() {
        let offset = def_node.location().start_offset();
        let line = content[..offset].iter().filter(|&&b| b == b'\n').count() as u32;
        if line == target_line {
            return Some(def_node);
        }
    }

    // Recurse into child nodes
    if let Some(program) = node.as_program_node() {
        for stmt in program.statements().body().iter() {
            if let Some(found) = find_def_node_recursive(&stmt, target_line, content) {
                return Some(found);
            }
        }
    }

    if let Some(class_node) = node.as_class_node() {
        if let Some(body) = class_node.body() {
            if let Some(stmts) = body.as_statements_node() {
                for stmt in stmts.body().iter() {
                    if let Some(found) = find_def_node_recursive(&stmt, target_line, content) {
                        return Some(found);
                    }
                }
            }
        }
    }

    if let Some(module_node) = node.as_module_node() {
        if let Some(body) = module_node.body() {
            if let Some(stmts) = body.as_statements_node() {
                for stmt in stmts.body().iter() {
                    if let Some(found) = find_def_node_recursive(&stmt, target_line, content) {
                        return Some(found);
                    }
                }
            }
        }
    }

    if let Some(stmts) = node.as_statements_node() {
        for stmt in stmts.body().iter() {
            if let Some(found) = find_def_node_recursive(&stmt, target_line, content) {
                return Some(found);
            }
        }
    }

    if let Some(sclass) = node.as_singleton_class_node() {
        if let Some(body) = sclass.body() {
            if let Some(stmts) = body.as_statements_node() {
                for stmt in stmts.body().iter() {
                    if let Some(found) = find_def_node_recursive(&stmt, target_line, content) {
                        return Some(found);
                    }
                }
            }
        }
    }

    None
}

/// Infer type from assignment patterns like `var = Class.new.method`.
/// This is a standalone function that can be used without a full TypeQuery.
pub fn infer_type_from_assignment(
    content: &str,
    var_name: &str,
    index: &crate::indexer::index::RubyIndex,
) -> Option<RubyType> {
    // Look for assignment pattern: `var_name = ...`
    for line in content.lines() {
        let trimmed = line.trim();

        // Look for assignment pattern: `var = ...`
        if let Some(rest) = trimmed.strip_prefix(var_name) {
            // Make sure we matched the whole variable name (not just a prefix)
            let next_char = rest.chars().next();
            if !matches!(next_char, Some(' ') | Some('\t') | Some('=')) {
                continue;
            }

            let rest = rest.trim();
            if let Some(rest) = rest.strip_prefix('=') {
                let rhs = rest.trim();

                // Look for .new somewhere in the chain
                if let Some(new_pos) = rhs.find(".new") {
                    // Extract the class name before .new
                    let class_part = rhs[..new_pos].trim();

                    // Validate it's a constant (starts with uppercase)
                    if !class_part
                        .chars()
                        .next()
                        .map(|c| c.is_uppercase())
                        .unwrap_or(false)
                    {
                        continue;
                    }

                    // Parse the constant path
                    let parts: Vec<_> = class_part
                        .split("::")
                        .filter_map(|s| RubyConstant::new(s.trim()).ok())
                        .collect();

                    if parts.is_empty() {
                        continue;
                    }

                    let class_fqn = FullyQualifiedName::Constant(parts.into());
                    let mut current_type = RubyType::Class(class_fqn);

                    // Check for method chain after .new
                    let after_new = &rhs[new_pos + 4..]; // Skip ".new"

                    // Skip any arguments after .new
                    let after_new = if after_new.starts_with('(') {
                        if let Some(close_paren) = after_new.find(')') {
                            &after_new[close_paren + 1..]
                        } else {
                            after_new
                        }
                    } else {
                        after_new
                    };

                    // Parse method chain: .method1.method2.method3
                    for method_call in after_new.split('.') {
                        let method_name = method_call
                            .split(|c: char| c == '(' || c.is_whitespace())
                            .next()
                            .unwrap_or("")
                            .trim();

                        if method_name.is_empty() {
                            continue;
                        }

                        // Look up the method's return type
                        if let Some(return_type) = MethodResolver::resolve_method_return_type(
                            index,
                            &current_type,
                            method_name,
                        ) {
                            current_type = return_type;
                        } else {
                            // Can't resolve this method, stop the chain
                            break;
                        }
                    }

                    return Some(current_type);
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_hint_kind_equality() {
        assert_eq!(TypeHintKind::MethodReturn, TypeHintKind::MethodReturn);
        assert_ne!(TypeHintKind::MethodReturn, TypeHintKind::LocalVariable);
    }

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
