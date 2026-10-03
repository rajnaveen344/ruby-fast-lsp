//! Local captures bound by a `case`/`in` pattern.
//!
//! Both flow walks type a capture the same way: a capture in a literal Hash
//! or Array pattern takes the type of the matching element of a literal
//! value, and a Hash pattern over a known shape takes the field type.

use crate::core::RubyType;
use crate::inference::r#type::literal::literal_key;
use crate::inference::r#type::shape as shape_reads;
use ruby_prism::*;
use std::collections::HashMap;

/// Type the captures of `pattern` against a literal `value` node; each
/// captured element is typed by `infer`.
pub(crate) fn capture_types_from_value(
    pattern: &Node<'_>,
    value: &Node<'_>,
    captures: &mut HashMap<String, RubyType>,
    infer: &mut impl FnMut(&Node<'_>) -> RubyType,
) {
    if let Some(target) = pattern.as_local_variable_target_node() {
        let name = String::from_utf8_lossy(target.name().as_slice()).to_string();
        captures.insert(name, infer(value));
        return;
    }

    if let Some(pattern_hash) = pattern.as_hash_pattern_node() {
        let Some(value_hash) = value.as_hash_node() else {
            return;
        };
        let value_elements = value_hash
            .elements()
            .iter()
            .filter_map(|element| {
                let assoc = element.as_assoc_node()?;
                Some((symbol_key(&assoc.key())?, assoc.value()))
            })
            .collect::<Vec<_>>();

        for element in pattern_hash.elements().iter() {
            let Some(assoc) = element.as_assoc_node() else {
                continue;
            };
            let Some(key) = symbol_key(&assoc.key()) else {
                continue;
            };
            let Some((_, value_node)) = value_elements
                .iter()
                .find(|(value_key, _)| value_key == &key)
            else {
                continue;
            };
            capture_types_from_value(&assoc.value(), value_node, captures, infer);
        }
        return;
    }

    if let Some(pattern_array) = pattern.as_array_pattern_node() {
        let Some(value_array) = value.as_array_node() else {
            return;
        };
        let value_elements = value_array.elements().iter().collect::<Vec<_>>();
        for (index, required) in pattern_array.requireds().iter().enumerate() {
            let Some(value_node) = value_elements.get(index) else {
                continue;
            };
            capture_types_from_value(&required, value_node, captures, infer);
        }
    }
}

/// Type the captures of `pattern` against a value of known type. Hash
/// pattern fields read the shape field of the same key.
pub(crate) fn capture_types_from_type(
    pattern: &Node<'_>,
    value_type: &RubyType,
    captures: &mut HashMap<String, RubyType>,
) {
    if let Some(implicit) = pattern.as_implicit_node() {
        capture_types_from_type(&implicit.value(), value_type, captures);
        return;
    }
    if let Some(target) = pattern.as_local_variable_target_node() {
        let name = String::from_utf8_lossy(target.name().as_slice()).to_string();
        captures.insert(name, value_type.clone());
        return;
    }
    let Some(pattern_hash) = pattern.as_hash_pattern_node() else {
        return;
    };
    for element in pattern_hash.elements().iter() {
        let Some(assoc) = element.as_assoc_node() else {
            continue;
        };
        let Some(key) = literal_key(&assoc.key()) else {
            continue;
        };
        let Ok(field_type) = shape_reads::indexed_read(value_type, Some(&key)) else {
            continue;
        };
        capture_types_from_type(&assoc.value(), &field_type, captures);
    }
}

fn symbol_key(node: &Node<'_>) -> Option<String> {
    node.as_symbol_node()
        .map(|symbol| String::from_utf8_lossy(symbol.unescaped()).to_string())
}
