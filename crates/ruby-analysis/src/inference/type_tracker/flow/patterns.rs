use crate::core::RubyType;
use crate::inference::r#type::pattern::{capture_types_from_type, capture_types_from_value};
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
                capture_types_from_type(pattern, &ruby_type, &mut captures);
                return captures;
            }
        }
        capture_types_from_value(pattern, value, &mut captures, &mut |element| {
            self.infer_expression(element)
        });
        captures
    }
}
