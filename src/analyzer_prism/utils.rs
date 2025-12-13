use crate::indexer::entry::MixinRef;
use crate::indexer::index::RubyIndex;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_namespace::RubyConstant;
use ruby_prism::{ConstantPathNode, ConstantReadNode, Node};

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

/// Resolve a constant (ConstantReadNode or ConstantPathNode) to a FullyQualifiedName
/// by searching the index according to Ruby's constant lookup rules.
/// This centralizes the constant resolution logic used throughout the codebase.
pub fn resolve_constant_fqn(
    index: &RubyIndex,
    node: &Node,
    current_fqn: &FullyQualifiedName,
) -> Option<FullyQualifiedName> {
    if let Some(constant_read) = node.as_constant_read_node() {
        resolve_constant_read_fqn(index, &constant_read, current_fqn)
    } else if let Some(constant_path) = node.as_constant_path_node() {
        resolve_constant_path_fqn(index, &constant_path, current_fqn)
    } else {
        None
    }
}

/// Resolve a ConstantReadNode to a FullyQualifiedName using Ruby's constant lookup rules.
fn resolve_constant_read_fqn(
    index: &RubyIndex,
    node: &ConstantReadNode,
    current_fqn: &FullyQualifiedName,
) -> Option<FullyQualifiedName> {
    let name = String::from_utf8_lossy(node.name().as_slice()).to_string();
    let constant = RubyConstant::new(&name).ok()?;

    resolve_constant_fqn_from_parts(index, &[constant], false, current_fqn)
}

/// Resolve a ConstantPathNode to a FullyQualifiedName using Ruby's constant lookup rules.
fn resolve_constant_path_fqn(
    index: &RubyIndex,
    node: &ConstantPathNode,
    current_fqn: &FullyQualifiedName,
) -> Option<FullyQualifiedName> {
    let mut parts = vec![];
    collect_namespaces(node, &mut parts);

    // A ConstantPathNode is absolute if its parent is None (starts with ::)
    let absolute = node.parent().is_none();

    resolve_constant_fqn_from_parts(index, &parts, absolute, current_fqn)
}

/// Core constant resolution logic that follows Ruby's constant lookup rules.
/// For absolute constants (::Foo::Bar), searches from root.
/// For relative constants (Foo::Bar), searches through lexical scope hierarchy.
pub fn resolve_constant_fqn_from_parts(
    index: &RubyIndex,
    parts: &[RubyConstant],
    absolute: bool,
    current_fqn: &FullyQualifiedName,
) -> Option<FullyQualifiedName> {
    let mut search_paths: Vec<Vec<RubyConstant>> = vec![];

    if absolute {
        // For `::Foo::Bar`, we check `Foo::Bar`, then `Bar`
        let mut remaining_parts = parts.to_vec();
        while !remaining_parts.is_empty() {
            search_paths.push(remaining_parts.clone());
            remaining_parts.remove(0);
        }
    } else {
        // For relative paths like `C` inside `module A; module B;`,
        // search order is `A::B::C`, `A::C`, `C`.
        let mut lexical_scope = current_fqn.namespace_parts().to_vec();
        loop {
            let mut candidate_parts = lexical_scope.clone();
            candidate_parts.extend(parts.iter().cloned());
            search_paths.push(candidate_parts);

            if lexical_scope.is_empty() {
                break;
            }
            lexical_scope.pop();
        }
    }

    // Search through all candidate paths and return the first match
    for candidate_parts in search_paths {
        let candidate_fqn = FullyQualifiedName::Constant(candidate_parts);
        if index.contains_fqn(&candidate_fqn) {
            return Some(candidate_fqn);
        }
    }

    None
}
