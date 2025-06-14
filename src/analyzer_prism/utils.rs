use crate::types::ruby_namespace::RubyConstant;
use ruby_prism::ConstantPathNode;

/// Recursively collect all namespaces from a ConstantPathNode
/// Eg: `Core::Platform::API::Users` will return
/// `vec![
///     RubyConstant("Core"),
///     RubyConstant("Platform"),
///     RubyConstant("API"),
///     RubyConstant("Users")
/// ]`
pub fn collect_namespaces(node: &ConstantPathNode, acc: &mut Vec<RubyConstant>) {
    let name = String::from_utf8_lossy(node.name().unwrap().as_slice());

    if let Some(parent) = node.parent() {
        if let Some(parent_const) = parent.as_constant_path_node() {
            collect_namespaces(&parent_const, acc);
        }

        if let Some(parent_const_read) = parent.as_constant_read_node() {
            let parent_name =
                String::from_utf8_lossy(parent_const_read.name().as_slice()).to_string();
            acc.push(RubyConstant::new(&parent_name).unwrap());
        }
    }

    acc.push(RubyConstant::new(&name.to_string()).unwrap());
}
