//! Catalog-independent Ruby syntax helpers for dotted Java calls, constant
//! paths, static names, and Java package identifiers.

use ruby_prism::{CallNode, ConstantPathNode, Node};

pub(super) fn dotted_call_name(call: &CallNode<'_>) -> Option<String> {
    if call.arguments().is_some() || call.block().is_some() {
        return None;
    }
    let name = std::str::from_utf8(call.name().as_slice()).ok()?;
    if let Some(receiver) = call.receiver() {
        if receiver
            .as_constant_read_node()
            .is_some_and(|constant| constant.name().as_slice() == b"Java")
        {
            return Some(name.to_string());
        }
        let receiver = receiver.as_call_node()?;
        let prefix = dotted_call_name(&receiver)?;
        return Some(format!("{prefix}.{name}"));
    }
    Some(name.to_string())
}

pub(super) fn dotted_call_root<'a>(call: &CallNode<'a>) -> Option<&'a [u8]> {
    if call.arguments().is_some() || call.block().is_some() {
        return None;
    }
    match call.receiver() {
        None => Some(call.name().as_slice()),
        Some(receiver) => {
            if receiver
                .as_constant_read_node()
                .is_some_and(|constant| constant.name().as_slice() == b"Java")
            {
                return Some(b"Java");
            }
            dotted_call_root(&receiver.as_call_node()?)
        }
    }
}

pub(super) fn is_java_class_name(name: &str) -> bool {
    name.rsplit(['.', '/'])
        .next()
        .and_then(|class| class.split('$').next())
        .and_then(|class| class.chars().next())
        .is_some_and(char::is_uppercase)
}

pub(super) fn java_package_prefix(package: &str) -> Option<String> {
    let mut components = package.split('.');
    let first = components.next()?;
    if !valid_java_identifier(first) {
        return None;
    }
    let mut normalized = first.to_string();
    for component in components {
        if !valid_java_identifier(component) {
            return None;
        }
        normalized.push('/');
        normalized.push_str(component);
    }
    normalized.push('/');
    Some(normalized)
}

pub(super) fn static_symbol_or_string(node: &Node<'_>) -> Option<String> {
    if let Some(symbol) = node.as_symbol_node() {
        return Some(String::from_utf8_lossy(symbol.unescaped()).to_string());
    }
    node.as_string_node()
        .map(|string| String::from_utf8_lossy(string.unescaped()).to_string())
}

fn valid_java_identifier(component: &str) -> bool {
    let mut chars = component.chars();
    chars
        .next()
        .is_some_and(|first| first.is_alphabetic() || first == '_' || first == '$')
        && chars
            .all(|character| character.is_alphanumeric() || character == '_' || character == '$')
}

pub(super) fn canonical_java_constant_path(node: &ConstantPathNode<'_>) -> Option<String> {
    let mut parts = Vec::new();
    collect_ruby_constant_path(node, &mut parts)?;
    (parts.first().is_some_and(|part| part == "Java") && parts.len() >= 3).then(|| parts.join("::"))
}

pub(super) fn collect_ruby_constant_path(
    node: &ConstantPathNode<'_>,
    parts: &mut Vec<String>,
) -> Option<()> {
    if let Some(parent) = node.parent() {
        if let Some(path) = parent.as_constant_path_node() {
            collect_ruby_constant_path(&path, parts)?;
        } else if let Some(read) = parent.as_constant_read_node() {
            parts.push(String::from_utf8_lossy(read.name().as_slice()).to_string());
        } else {
            return None;
        }
    }
    let name = node.name()?;
    parts.push(String::from_utf8_lossy(name.as_slice()).to_string());
    Some(())
}
