use lsp_types::{Position, Range};
use ruby_prism::{ConstantPathNode, ConstantReadNode, Location, Node};

use crate::indexer::types::{
    fully_qualified_name::FullyQualifiedName, ruby_constant::RubyConstant,
    ruby_namespace::RubyNamespace,
};

/// Convert an LSP position to a Prism location
/// LSP positions are 0-based, while Prism locations are byte offsets
pub fn lsp_pos_to_prism_loc(pos: Position, content: &str) -> usize {
    let mut byte_offset = 0;
    let mut line_count = 0;
    let mut char_count = 0;

    for c in content.chars() {
        if line_count == pos.line && char_count == pos.character {
            break;
        }

        if c == '\n' {
            line_count += 1;
            char_count = 0;
        } else {
            char_count += 1;
        }

        byte_offset += c.len_utf8();
    }

    byte_offset
}

/// Convert a Prism location back to an LSP position
pub fn prism_loc_to_lsp_pos(loc: Location, content: &str) -> Position {
    let mut line = 0;
    let mut character = 0;
    let mut current_offset = 0;

    for c in content.chars() {
        if current_offset >= loc.start_offset() {
            break;
        }

        if c == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }

        current_offset += c.len_utf8();
    }

    Position::new(line as u32, character as u32)
}

/// Convert a Prism node to a fully qualified name
/// This is used for goto definition and other features that need to identify nodes
pub fn node_to_fqn(node: &Node, namespace_stack: &[RubyNamespace]) -> Option<FullyQualifiedName> {
    match node {
        Node::ConstantPathNode { .. } => {
            // Get the rightmost constant name (the actual constant being referenced)
            let name = String::from_utf8_lossy(node.as_constant_path_node()?.name()?.as_slice());
            let constant = RubyConstant::new(&name).ok()?;

            // Build the namespace path from the parent nodes
            let mut parts = Vec::new();
            let mut current = node.as_constant_path_node()?.parent();
            while let Some(current_node) = current {
                match current_node {
                    Node::ConstantPathNode { .. } => {
                        if let Some(cpn) = current_node.as_constant_path_node() {
                            let name_str = String::from_utf8_lossy(cpn.name()?.as_slice());
                            if let Ok(ns) = RubyNamespace::new(&name_str) {
                                parts.push(ns);
                            }
                            current = cpn.parent(); // Move to the parent node
                        } else {
                            current = None; // Should not happen if match arm is correct, but safer
                        }
                    }
                    Node::ConstantReadNode { .. } => {
                        if let Some(crn) = current_node.as_constant_read_node() {
                            let name_str = String::from_utf8_lossy(crn.name().as_slice());
                            if let Ok(ns) = RubyNamespace::new(&name_str) {
                                parts.push(ns);
                            }
                        }
                        // ConstantReadNode doesn't have a `parent`, so we stop here
                        current = None;
                    }
                    Node::CallNode { .. } => {
                        // Handle cases like `Foo::Bar()` where the parent is a call node
                        // We might need to traverse up the receiver of the call node
                        // For now, we'll stop traversal if we hit a CallNode
                        // This might need refinement depending on desired behavior
                        current = None; // Stop traversal
                    }
                    // Add other node types if necessary, or break/continue
                    _ => {
                        // Stop if we encounter an unexpected node type in the path
                        current = None;
                    }
                }
            }
            parts.reverse();

            Some(FullyQualifiedName::constant(parts, constant))
        }
        Node::ConstantReadNode { .. } => {
            let name = String::from_utf8_lossy(node.as_constant_read_node()?.name().as_slice());
            let constant = RubyConstant::new(&name).ok()?;

            // For a constant read, we need to consider the current namespace stack
            let parts = namespace_stack.to_vec();

            Some(FullyQualifiedName::constant(parts, constant))
        }
        _ => None,
    }
}
