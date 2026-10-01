use super::underscore;
use crate::simulation::graph::{ConstantRefShape, MethodTarget};

pub(super) fn constant_ref_text(
    lexical_scope: &str,
    constant: &super::graph::ConstantRefSpec,
) -> String {
    match &constant.shape {
        ConstantRefShape::Auto => {
            let prefix = format!("{lexical_scope}::");
            if let Some(local) = constant.fqn.strip_prefix(&prefix) {
                return local.to_string();
            }
            constant.fqn.clone()
        }
        ConstantRefShape::Absolute => format!("::{}", constant.fqn),
        ConstantRefShape::ConstGet => {
            let (receiver, leaf) = const_get_receiver(&constant.fqn);
            format!("{receiver}.const_get(:{leaf})")
        }
        ConstantRefShape::ConstDefined => {
            let (receiver, leaf) = const_get_receiver(&constant.fqn);
            format!("{receiver}.const_defined?(:{leaf})")
        }
        ConstantRefShape::RelativeName { name } => name.clone(),
        ConstantRefShape::Qualified { path } => path.clone(),
    }
}

pub(super) fn constant_ref_cursor_offset(shape: &ConstantRefShape, text: &str) -> usize {
    match shape {
        ConstantRefShape::ConstGet | ConstantRefShape::ConstDefined => text
            .rfind(':')
            .map(|idx| idx + ':'.len_utf8())
            .unwrap_or_else(|| leaf_segment_offset(text)),
        ConstantRefShape::Auto
        | ConstantRefShape::Absolute
        | ConstantRefShape::RelativeName { .. }
        | ConstantRefShape::Qualified { .. } => leaf_segment_offset(text),
    }
}

pub(super) fn leaf_segment_offset(fqn: &str) -> usize {
    fqn.rfind("::").map(|idx| idx + "::".len()).unwrap_or(0)
}

pub(super) fn const_get_receiver(fqn: &str) -> (String, String) {
    let Some(index) = fqn.rfind("::") else {
        return ("Object".to_string(), fqn.to_string());
    };
    (
        fqn[..index].to_string(),
        fqn[index + "::".len()..].to_string(),
    )
}

pub(super) fn yield_helper_name(target: &MethodTarget) -> String {
    let owner = target
        .owner
        .split("::")
        .map(underscore)
        .collect::<Vec<_>>()
        .join("_");
    format!("__sim_yield_{}_{}", owner, method_slug(&target.name))
}

pub(super) fn method_slug(name: &str) -> String {
    name.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect()
}

pub(super) fn return_expression(return_type: &str) -> String {
    match return_type {
        "String" => "\"value\"".to_string(),
        "Integer" => "1".to_string(),
        "Float" => "1.0".to_string(),
        "Symbol" => ":value".to_string(),
        "NilClass" => "nil".to_string(),
        // Model collection values directly. An unrelated navigation scenario
        // must not depend on guessing the type of an unindexed constructor.
        "Array" => "[]".to_string(),
        class_name => format!("{}.new", class_name),
    }
}

pub(super) fn indent_len(depth: usize) -> usize {
    depth * 2
}
