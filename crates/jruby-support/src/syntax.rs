//! Catalog-independent Ruby syntax helpers for dotted Java calls, constant
//! paths, static names, Java package identifiers, and static import aliases.

use ruby_prism::{CallNode, ConstantPathNode, Node};

const MAX_STATIC_IMPORT_ALIAS_BYTES: usize = 256;

pub fn dotted_call_name(call: &CallNode<'_>) -> Option<String> {
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

pub fn dotted_call_root<'a>(call: &CallNode<'a>) -> Option<&'a [u8]> {
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

pub fn is_java_class_name(name: &str) -> bool {
    name.rsplit(['.', '/'])
        .next()
        .and_then(|class| class.split('$').next())
        .and_then(|class| class.chars().next())
        .is_some_and(char::is_uppercase)
}

pub fn java_package_prefix(package: &str) -> Option<String> {
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

pub fn static_symbol_or_string(node: &Node<'_>) -> Option<String> {
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

pub fn canonical_java_constant_path(node: &ConstantPathNode<'_>) -> Option<String> {
    let mut parts = Vec::new();
    collect_ruby_constant_path(node, &mut parts)?;
    (parts.first().is_some_and(|part| part == "Java") && parts.len() >= 3).then(|| parts.join("::"))
}

pub fn collect_ruby_constant_path(
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

/// Evaluate a `java_import` alias block `{ |package, name| ... }` whose body is
/// a string built only from its two parameters, bounded to
/// `MAX_STATIC_IMPORT_ALIAS_BYTES`; `None` for anything dynamic.
pub fn evaluate_static_import_alias(
    block: &ruby_prism::BlockNode<'_>,
    package: &str,
    class_name: &str,
) -> Option<String> {
    let parameters = block
        .parameters()?
        .as_block_parameters_node()?
        .parameters()?;
    if parameters.requireds().iter().count() != 2
        || parameters.optionals().iter().next().is_some()
        || parameters.rest().is_some()
        || parameters.posts().iter().next().is_some()
        || parameters.keywords().iter().next().is_some()
        || parameters.keyword_rest().is_some()
        || parameters.block().is_some()
    {
        return None;
    }
    let names = parameters
        .requireds()
        .iter()
        .map(|parameter| {
            parameter
                .as_required_parameter_node()
                .map(|parameter| String::from_utf8_lossy(parameter.name().as_slice()).to_string())
        })
        .collect::<Option<Vec<_>>>()?;
    let expression = single_expression(block.body()?)?;
    let alias = evaluate_static_alias_expression(
        expression,
        (&names[0], package),
        (&names[1], class_name),
    )?;
    (alias.len() <= MAX_STATIC_IMPORT_ALIAS_BYTES).then_some(alias)
}

fn single_expression(node: Node<'_>) -> Option<Node<'_>> {
    if let Some(statements) = node.as_statements_node() {
        let mut body = statements.body().iter();
        let expression = body.next()?;
        if body.next().is_some() {
            return None;
        }
        return Some(expression);
    }
    if let Some(embedded) = node.as_embedded_statements_node() {
        let statements = embedded.statements()?;
        let mut body = statements.body().iter();
        let expression = body.next()?;
        if body.next().is_some() {
            return None;
        }
        return Some(expression);
    }
    Some(node)
}

fn evaluate_static_alias_expression(
    node: Node<'_>,
    first: (&str, &str),
    second: (&str, &str),
) -> Option<String> {
    if let Some(string) = node.as_string_node() {
        return Some(String::from_utf8_lossy(string.unescaped()).to_string());
    }
    if let Some(local) = node.as_local_variable_read_node() {
        let name = String::from_utf8_lossy(local.name().as_slice());
        return match name.as_ref() {
            name if name == first.0 => Some(first.1.to_string()),
            name if name == second.0 => Some(second.1.to_string()),
            _ => None,
        };
    }
    let interpolated = node.as_interpolated_string_node()?;
    let mut output = String::new();
    for part in interpolated.parts().iter() {
        let value = if let Some(string) = part.as_string_node() {
            String::from_utf8_lossy(string.unescaped()).to_string()
        } else {
            evaluate_static_alias_expression(single_expression(part)?, first, second)?
        };
        if output.len().saturating_add(value.len()) > MAX_STATIC_IMPORT_ALIAS_BYTES {
            return None;
        }
        output.push_str(&value);
    }
    Some(output)
}
