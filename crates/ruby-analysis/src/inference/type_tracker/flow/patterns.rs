use crate::core::RubyType;
use crate::inference::r#type::literal::literal_key;
use crate::inference::r#type::shape as shape_reads;
use crate::inference::type_tracker::TypeTracker;
use ruby_prism::*;
use std::collections::HashMap;

impl TypeTracker {
    pub(in crate::inference::type_tracker) fn pattern_capture_types_for_value(
        &mut self,
        pattern: &Node<'_>,
        value: &Node<'_>,
    ) -> HashMap<String, RubyType> {
        let mut captures = HashMap::new();
        if let Some(read) = value.as_local_variable_read_node() {
            let name = String::from_utf8_lossy(read.name().as_slice());
            if let Some(ruby_type) = self.environment.types.get(name.as_ref()).cloned() {
                self.collect_pattern_capture_types_from_type(pattern, &ruby_type, &mut captures);
                return captures;
            }
        }
        self.collect_pattern_capture_types(pattern, value, &mut captures);
        captures
    }

    pub(in crate::inference::type_tracker) fn collect_pattern_capture_types_from_type(
        &self,
        pattern: &Node<'_>,
        value_type: &RubyType,
        captures: &mut HashMap<String, RubyType>,
    ) {
        if let Some(implicit) = pattern.as_implicit_node() {
            self.collect_pattern_capture_types_from_type(&implicit.value(), value_type, captures);
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
            self.collect_pattern_capture_types_from_type(&assoc.value(), &field_type, captures);
        }
    }

    pub(in crate::inference::type_tracker) fn collect_pattern_capture_types(
        &mut self,
        pattern: &Node<'_>,
        value: &Node<'_>,
        captures: &mut HashMap<String, RubyType>,
    ) {
        if let Some(target) = pattern.as_local_variable_target_node() {
            let name = String::from_utf8_lossy(target.name().as_slice()).to_string();
            captures.insert(name, self.infer_expression(value));
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
                    Some((pattern_symbol_key(&assoc.key())?, assoc.value()))
                })
                .collect::<Vec<_>>();

            for element in pattern_hash.elements().iter() {
                let Some(assoc) = element.as_assoc_node() else {
                    continue;
                };
                let Some(key) = pattern_symbol_key(&assoc.key()) else {
                    continue;
                };
                let Some((_, value_node)) = value_elements
                    .iter()
                    .find(|(value_key, _)| value_key == &key)
                else {
                    continue;
                };
                self.collect_pattern_capture_types(&assoc.value(), value_node, captures);
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
                self.collect_pattern_capture_types(&required, value_node, captures);
            }
        }
    }
}
pub(in crate::inference::type_tracker) fn pattern_symbol_key(node: &Node<'_>) -> Option<String> {
    node.as_symbol_node()
        .map(|symbol| String::from_utf8_lossy(symbol.unescaped()).to_string())
}
