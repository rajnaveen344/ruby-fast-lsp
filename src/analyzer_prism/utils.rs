use crate::indexer::entry::MixinRef;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_namespace::RubyConstant;
use ruby_prism::{ConstantPathNode, Node};

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
            let parent_name =
                String::from_utf8_lossy(parent_const_read.name().as_slice()).to_string();
            if let Ok(constant) = RubyConstant::new(&parent_name) {
                acc.push(constant);
            }
        }
    }

    if let Some(name_node) = node.name() {
        let name = String::from_utf8_lossy(name_node.as_slice()).to_string();
        if let Ok(constant) = RubyConstant::new(&name) {
            acc.push(constant);
        }
    }
}

/// Create a MixinRef from a ConstantReadNode or ConstantPathNode.
/// This captures the textual representation of the constant without trying to resolve it,
/// which is deferred until a capability requests the ancestor chain.
pub fn mixin_ref_from_node(node: &Node) -> Option<MixinRef> {
    if let Some(n) = node.as_constant_read_node() {
        let name = String::from_utf8_lossy(n.name().as_slice()).to_string();
        if let Ok(constant) = RubyConstant::new(&name) {
            Some(MixinRef {
                parts: vec![constant],
                absolute: false,
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
        Some(MixinRef { parts, absolute })
    } else {
        None
    }
}

pub fn fqn_from_node(
    node: &Node,
    current_namespace: &[RubyConstant],
) -> Option<FullyQualifiedName> {
    if let Some(n) = node.as_constant_read_node() {
        let name = String::from_utf8_lossy(n.name().as_slice()).to_string();
        let mut fqn_parts = current_namespace.to_vec();
        if let Ok(constant) = RubyConstant::new(&name) {
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
