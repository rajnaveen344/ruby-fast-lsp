use ruby_analysis_core::NamespaceKind;
use ruby_prism::{ConstantPathNode, Location as PrismLocation, Node};
use tower_lsp::lsp_types::Location as LspLocation;

use crate::types::compact_location::CompactLocation;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_document::RubyDocument;
use crate::types::ruby_namespace::RubyConstant;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MixinRef {
    pub parts: Vec<RubyConstant>,
    pub absolute: bool,
    pub location: CompactLocation,
}

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

/// Create a MixinRef from a ConstantReadNode or ConstantPathNode.
/// This captures the textual representation of the constant without trying to resolve it,
/// which is deferred until a capability requests the ancestor chain.
///
/// The `location` parameter should be the CompactLocation of where the include/extend/prepend
/// call was made (for CodeLens and other features that need to show the call site).
pub fn mixin_ref_from_node(node: &Node, location: CompactLocation) -> Option<MixinRef> {
    if let Some(n) = node.as_constant_read_node() {
        let name = utf8_str(n.name().as_slice());
        if let Ok(constant) = RubyConstant::new(name) {
            Some(MixinRef {
                parts: vec![constant],
                absolute: false,
                location,
            })
        } else {
            None
        }
    } else if let Some(n) = node.as_constant_path_node() {
        let mut parts = vec![];
        collect_namespaces(&n, &mut parts);
        // A ConstantPathNode is absolute if its `parent` is `None`,
        // which corresponds to a `::` at the beginning.
        let absolute = n.parent().is_none();
        Some(MixinRef {
            parts,
            absolute,
            location,
        })
    } else {
        None
    }
}

pub fn fqn_from_node(
    node: &Node,
    current_namespace: &[RubyConstant],
) -> Option<FullyQualifiedName> {
    if let Some(n) = node.as_constant_read_node() {
        let name = utf8_str(n.name().as_slice());
        let mut fqn_parts = current_namespace.to_vec();
        if let Ok(constant) = RubyConstant::new(name) {
            fqn_parts.push(constant);
            Some(FullyQualifiedName::Constant(fqn_parts))
        } else {
            None
        }
    } else if let Some(n) = node.as_constant_path_node() {
        let mut collected_parts = vec![];
        collect_namespaces(&n, &mut collected_parts);

        let absolute = n.parent().is_none();
        let final_parts = if absolute {
            collected_parts
        } else {
            let mut parts = current_namespace.to_vec();
            parts.extend(collected_parts);
            parts
        };
        Some(FullyQualifiedName::Constant(final_parts))
    } else {
        None
    }
}

/// Get the body location for a node that has an optional body.
/// If the body exists, returns the body's location; otherwise returns the node's location.
/// This pattern is used consistently across ClassNode, ModuleNode, and DefNode visitors.
///
/// # Arguments
/// * `body_location` - Optional location from node.body().map(|b| b.location())
/// * `node_location` - The fallback location from node.location()
/// * `document` - The RubyDocument for converting prism locations to LSP locations
pub fn get_body_location(
    body_location: Option<PrismLocation>,
    node_location: &PrismLocation,
    document: &RubyDocument,
) -> LspLocation {
    if let Some(body_loc) = body_location {
        document.prism_location_to_lsp_location(&body_loc)
    } else {
        document.prism_location_to_lsp_location(node_location)
    }
}

/// Determine the NamespaceKind for a method based on its receiver.
/// Returns (NamespaceKind, should_skip_method) where:
/// - NamespaceKind::Instance for instance methods (no receiver or in singleton context)
/// - NamespaceKind::Singleton for class methods (self receiver or matching constant receiver)
/// - should_skip_method is true if the receiver type is unsupported
///
/// # Arguments
/// * `receiver` - Optional receiver node from DefNode.receiver()
/// * `current_namespace` - The current namespace stack for validating constant receivers
/// * `in_singleton` - Whether we're currently in a singleton context (class << self)
pub fn get_method_namespace_kind(
    receiver: Option<Node>,
    current_namespace: &[RubyConstant],
    in_singleton: bool,
) -> (NamespaceKind, bool) {
    let mut namespace_kind = NamespaceKind::Instance;
    let mut skip_method = false;

    if let Some(receiver) = receiver {
        if receiver.as_self_node().is_some() {
            namespace_kind = NamespaceKind::Singleton;
        } else if let Some(read_node) = receiver.as_constant_read_node() {
            let recv_name = utf8_str(read_node.name().as_slice());
            // Current namespace last element (if any) should match receiver constant
            let last_ns = current_namespace.last();
            if let Some(last) = last_ns {
                if last.as_str() == recv_name {
                    namespace_kind = NamespaceKind::Singleton;
                } else {
                    skip_method = true;
                }
            } else {
                // No enclosing namespace -> unsupported
                skip_method = true;
            }
        } else if receiver.as_constant_path_node().is_some() {
            // For reference/identifier visitors, any constant receiver = Singleton
            namespace_kind = NamespaceKind::Singleton;
        } else {
            // Other receiver types not supported
            skip_method = true;
        }
    } else if in_singleton {
        namespace_kind = NamespaceKind::Singleton;
    }

    (namespace_kind, skip_method)
}

/// Simplified version of get_method_namespace_kind for visitors that don't need
/// to validate constant receivers (fact_collector, identifier_visitor).
/// Returns NamespaceKind based on presence of receiver.
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

/// Build the full constant path name as a string (e.g., "Foo::Bar::Baz")
pub(crate) fn build_constant_path_name(node: &Node) -> String {
    let mut parts = Vec::new();
    collect_constant_path_parts_for_name(node, &mut parts);
    parts.join("::")
}

/// Recursively collect constant path parts for building the name
pub(crate) fn collect_constant_path_parts_for_name(node: &Node, parts: &mut Vec<String>) {
    if let Some(constant_path) = node.as_constant_path_node() {
        // Process parent first (left side)
        if let Some(parent) = constant_path.parent() {
            collect_constant_path_parts_for_name(&parent, parts);
        }
        // Then add the name (right side)
        if let Some(name_bytes) = constant_path.name() {
            parts.push(utf8_str(name_bytes.as_slice()).to_string());
        }
    } else if let Some(constant_read) = node.as_constant_read_node() {
        parts.push(utf8_str(constant_read.name().as_slice()).to_string());
    }
}
