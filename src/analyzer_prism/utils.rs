use crate::types::ruby_namespace::RubyNamespace;
use ruby_prism::ConstantPathNode;

/// Recursively collect all namespaces from a ConstantPathNode
/// Eg: `Core::Platform::API::Users` will return
/// `vec![
///     RubyNamespace("Core"),
///     RubyNamespace("Platform"),
///     RubyNamespace("API"),
///     RubyNamespace("Users")
/// ]`
pub fn collect_namespaces(node: &ConstantPathNode, acc: &mut Vec<RubyNamespace>) {
    let name = String::from_utf8_lossy(node.name().unwrap().as_slice());

    if let Some(parent) = node.parent() {
        if let Some(parent_const) = parent.as_constant_path_node() {
            collect_namespaces(&parent_const, acc);
        }

        if let Some(parent_const_read) = parent.as_constant_read_node() {
            let parent_name =
                String::from_utf8_lossy(parent_const_read.name().as_slice()).to_string();
            acc.push(RubyNamespace::new(&parent_name).unwrap());
        }
    }

    acc.push(RubyNamespace::new(&name.to_string()).unwrap());
}
