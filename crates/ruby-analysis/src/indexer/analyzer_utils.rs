use crate::core::NamespaceKind;
use ruby_prism::{ConstantPathNode, Location as PrismLocation, Node};

use crate::core::RubyConstant;

/// Recursively collect all namespaces from a ConstantPathNode
/// Eg: `Core::Platform::API::Users` will return
/// `vec![
///     RubyConstant("Core"),
///     RubyConstant("Platform"),
///     RubyConstant("API"),
///     RubyConstant("Users")
/// ]`
pub fn collect_namespaces(node: &ConstantPathNode, acc: &mut Vec<RubyConstant>) {
    if let Some(parent) = node.parent() {
        if let Some(parent_const_path) = parent.as_constant_path_node() {
            collect_namespaces(&parent_const_path, acc);
        } else if let Some(parent_const_read) = parent.as_constant_read_node() {
            let parent_name = utf8_str(parent_const_read.name().as_slice());
            if let Ok(constant) = RubyConstant::new(parent_name) {
                acc.push(constant);
            }
        }
    }

    if let Some(name_node) = node.name() {
        let name = utf8_str(name_node.as_slice());
        if let Ok(constant) = RubyConstant::new(name) {
            acc.push(constant);
        }
    }
}

/// Get the body location for a node that has an optional body.
/// If the body exists, returns the body's location; otherwise returns the node's location.
/// This pattern is used consistently across ClassNode, ModuleNode, and DefNode visitors.
///
/// # Arguments
/// * `body_location` - Optional location from node.body().map(|b| b.location())
/// * `node_location` - The fallback location from node.location()
pub fn get_body_offsets(
    body_location: Option<PrismLocation<'_>>,
    node_location: &PrismLocation<'_>,
) -> (usize, usize) {
    match body_location {
        Some(body) => (body.start_offset(), body.end_offset()),
        None => (node_location.start_offset(), node_location.end_offset()),
    }
}

/// Classify a method receiver for cursor-target discovery.
/// Constant receivers and `self` select the singleton namespace.
pub fn get_method_namespace_kind_simple(receiver: Option<&Node>) -> NamespaceKind {
    if let Some(receiver) = receiver {
        if receiver.as_self_node().is_some()
            || receiver.as_constant_path_node().is_some()
            || receiver.as_constant_read_node().is_some()
        {
            NamespaceKind::Singleton
        } else {
            NamespaceKind::Instance
        }
    } else {
        NamespaceKind::Instance
    }
}

/// Zero-alloc view of a prism byte slice as &str. Prism identifiers are
/// expected to be valid UTF-8; any invalid bytes yield "".
pub(crate) fn utf8_str(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).unwrap_or("")
}
