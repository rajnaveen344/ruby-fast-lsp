//! AST utility functions shared across the codebase.
//!
//! This module contains common AST traversal and search functions
//! to avoid duplication across hover, inlay_hints, and type inference.

use ruby_prism::{DefNode, Node};

/// Find a DefNode at the given line in the AST.
///
/// This recursively searches the AST for a method definition (`def`) node
/// whose start position matches the target line.
///
/// # Arguments
/// * `node` - The AST node to search within
/// * `target_line` - The line number to search for (0-indexed)
/// * `content` - The source file content (used to calculate line numbers)
///
/// # Returns
/// * `Some(DefNode)` if a matching method definition is found
/// * `None` if no match is found
pub fn find_def_node_at_line<'a>(
    node: &Node<'a>,
    target_line: u32,
    content: &str,
) -> Option<DefNode<'a>> {
    // Try to match DefNode
    if let Some(def_node) = node.as_def_node() {
        let offset = def_node.location().start_offset();
        // Calculate line from byte offset (count newlines before this offset)
        let line = content.as_bytes()[..offset]
            .iter()
            .filter(|&&b| b == b'\n')
            .count() as u32;
        if line == target_line {
            return Some(def_node);
        }
    }

    // Recurse into child nodes (Program, Class, Module, Statements)
    if let Some(program) = node.as_program_node() {
        for stmt in program.statements().body().iter() {
            if let Some(found) = find_def_node_at_line(&stmt, target_line, content) {
                return Some(found);
            }
        }
    }

    if let Some(class_node) = node.as_class_node() {
        if let Some(body) = class_node.body() {
            if let Some(stmts) = body.as_statements_node() {
                for stmt in stmts.body().iter() {
                    if let Some(found) = find_def_node_at_line(&stmt, target_line, content) {
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
                    if let Some(found) = find_def_node_at_line(&stmt, target_line, content) {
                        return Some(found);
                    }
                }
            }
        }
    }

    if let Some(stmts) = node.as_statements_node() {
        for stmt in stmts.body().iter() {
            if let Some(found) = find_def_node_at_line(&stmt, target_line, content) {
                return Some(found);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_def_node_at_line() {
        let content = "def foo\n  42\nend";
        let parse_result = ruby_prism::parse(content.as_bytes());
        let root = parse_result.node();

        let def_node = find_def_node_at_line(&root, 0, content);
        assert!(def_node.is_some());
        assert_eq!(
            String::from_utf8_lossy(def_node.unwrap().name().as_slice()),
            "foo"
        );
    }

    #[test]
    fn test_find_def_node_in_class() {
        let content = "class Foo\n  def bar\n    123\n  end\nend";
        let parse_result = ruby_prism::parse(content.as_bytes());
        let root = parse_result.node();

        let def_node = find_def_node_at_line(&root, 1, content);
        assert!(def_node.is_some());
        assert_eq!(
            String::from_utf8_lossy(def_node.unwrap().name().as_slice()),
            "bar"
        );
    }

    #[test]
    fn test_find_def_node_not_found() {
        let content = "x = 42";
        let parse_result = ruby_prism::parse(content.as_bytes());
        let root = parse_result.node();

        let def_node = find_def_node_at_line(&root, 0, content);
        assert!(def_node.is_none());
    }
}
