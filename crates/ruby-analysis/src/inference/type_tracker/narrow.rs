//! Guard narrowing: refine the type environment based on a predicate's truth value.
//!
//! When control flow proves a predicate must have evaluated to a specific
//! boolean (e.g. `return if x.nil?` — code below proves the predicate was
//! false), we can narrow the types of variables tested by that predicate.
//!
//! Patterns supported (V1):
//! - `x.nil?`               truth=true → x = NilClass; false → x = remove_nil(x)
//! - `x.is_a?(C)`           truth=true → x = Class(C); false → x = subtract(Class(C))
//! - `x.kind_of?(C)`        same as is_a?
//! - `x.instance_of?(C)`    same as is_a?
//! - `!P`                   recurse into P with inverted truth
//!
//! Anything else is a no-op (we don't pretend to narrow what we can't prove).

use crate::core::FullyQualifiedName;
use crate::core::RubyConstant;
use crate::r#type::ruby::RubyType;
use ruby_prism::Node;
use std::collections::HashMap;

/// Narrow `env` in-place using `predicate` known to have evaluated to `truth`.
pub fn narrow(env: &mut HashMap<String, RubyType>, predicate: &Node, truth: bool) {
    let Some(call) = predicate.as_call_node() else {
        return;
    };
    let name = String::from_utf8_lossy(call.name().as_slice()).to_string();

    // Boolean negation: `!P` — recurse into receiver with inverted truth.
    if name == "!" && call.arguments().is_none() {
        if let Some(recv) = call.receiver() {
            narrow(env, &recv, !truth);
        }
        return;
    }

    // Receiver must be a local variable read for narrowing to apply.
    let Some(receiver) = call.receiver() else {
        return;
    };
    let Some(var_read) = receiver.as_local_variable_read_node() else {
        return;
    };
    let var_name = String::from_utf8_lossy(var_read.name().as_slice()).to_string();

    match name.as_str() {
        "nil?" => {
            if call.arguments().is_some() {
                return;
            }
            let nil_ty = RubyType::nil_class();
            if truth {
                env.insert(var_name, nil_ty);
            } else if let Some(current) = env.get(&var_name) {
                let narrowed = current.clone().remove_nil();
                env.insert(var_name, narrowed);
            }
        }
        "is_a?" | "kind_of?" | "instance_of?" => {
            let Some(args) = call.arguments() else {
                return;
            };
            let arg_nodes: Vec<_> = args.arguments().iter().collect();
            if arg_nodes.len() != 1 {
                return;
            }
            let Some(class_ty) = node_to_class_type(&arg_nodes[0]) else {
                return;
            };
            if truth {
                env.insert(var_name, class_ty);
            } else if let Some(current) = env.get(&var_name) {
                let narrowed = current.subtract(&class_ty);
                env.insert(var_name, narrowed);
            }
        }
        _ => {}
    }
}

/// Convert a constant-reference node (e.g. `String`, `Foo::Bar`) into a Class type.
fn node_to_class_type(node: &Node) -> Option<RubyType> {
    if let Some(const_read) = node.as_constant_read_node() {
        let name = String::from_utf8_lossy(const_read.name().as_slice()).to_string();
        let constant = RubyConstant::new(&name).ok()?;
        return Some(RubyType::Class(FullyQualifiedName::Constant(vec![
            constant,
        ])));
    }
    if let Some(const_path) = node.as_constant_path_node() {
        let parts = collect_constant_path(&const_path)?;
        return Some(RubyType::Class(FullyQualifiedName::Constant(parts)));
    }
    None
}

fn collect_constant_path(path: &ruby_prism::ConstantPathNode<'_>) -> Option<Vec<RubyConstant>> {
    let mut out = Vec::new();
    if let Some(parent) = path.parent() {
        if let Some(parent_const) = parent.as_constant_read_node() {
            let name = String::from_utf8_lossy(parent_const.name().as_slice()).to_string();
            out.push(RubyConstant::new(&name).ok()?);
        } else if let Some(parent_path) = parent.as_constant_path_node() {
            out.extend(collect_constant_path(&parent_path)?);
        } else {
            return None;
        }
    }
    let name_id = path.name()?;
    let name = String::from_utf8_lossy(name_id.as_slice()).to_string();
    out.push(RubyConstant::new(&name).ok()?);
    Some(out)
}
