use crate::core::{RubyType, ShapeConstructionError};
use crate::inference::r#type::literal::{
    infer_array_literal_type_fallible, infer_hash_literal_type_fallible, LiteralAnalyzer,
};
use crate::inference::type_tracker::flow::shapes::values::type_contains_shape;
use crate::inference::type_tracker::observations::LocalReadType;
use crate::inference::type_tracker::TypeTracker;
use ruby_prism::*;

pub(in crate::inference::type_tracker) mod callables;
pub(in crate::inference::type_tracker) mod calls;
pub(in crate::inference::type_tracker) mod constants;

impl TypeTracker {
    /// Infer the type of an expression
    ///
    /// Uses literal analyzer for static types, and handles variable reads
    /// by looking up their type from the current environment.
    pub(in crate::inference::type_tracker) fn infer_expression(&mut self, node: &Node) -> RubyType {
        // Hashes and arrays must use the flow-aware resolver recursively. The
        // syntax-only LiteralAnalyzer cannot prove local reads inside a nested
        // collection and would otherwise erase an already-proven shape.
        if let Some(result) = self.infer_collection_literal_type(node) {
            return result.unwrap_or(RubyType::Unknown);
        }

        // Try literal analysis first
        if let Some(ty) = LiteralAnalyzer::new().analyze_literal(node) {
            return ty;
        }

        // Handle local variable reads
        if let Some(read) = node.as_local_variable_read_node() {
            let var_name = String::from_utf8_lossy(read.name().as_slice()).to_string();
            let ruby_type = self
                .environment
                .types
                .get(&var_name)
                .cloned()
                .unwrap_or(RubyType::Unknown);
            let unknown_reason = self.environment.unknown_reason(&var_name);
            if self.observations.record_local_reads
                && (self.observations.has_seen_control_flow
                    || type_contains_shape(&ruby_type)
                    || unknown_reason.is_some())
            {
                let location = read.location();
                let constant_dependencies = self.environment.dependencies(&var_name);
                self.observations.local_reads.push(LocalReadType {
                    start_offset: location.start_offset(),
                    end_offset: location.end_offset(),
                    name: var_name,
                    ruby_type: ruby_type.clone(),
                    unknown_reason,
                    constant_dependencies,
                });
            }
            return ruby_type;
        }

        // Handle method calls
        if let Some(call) = node.as_call_node() {
            return self.infer_call(&call);
        }

        // Handle return statements
        if let Some(ret) = node.as_return_node() {
            return self.infer_return(&ret);
        }

        // A Ruby constant can hold any value. Require an engine-owned value or
        // namespace fact instead of turning unresolved syntax into a class.
        // FactCollector's direct pass installs those facts before local-flow
        // inference, so this also covers constants declared in the same file.
        if let Some((parts, absolute)) = Self::constant_reference(node) {
            if let Some(ruby_type) = self.constant_value_type(&parts, absolute) {
                return ruby_type;
            }
            return RubyType::Unknown;
        }

        // Handle parenthesized expressions
        if let Some(parens) = node.as_parentheses_node() {
            if let Some(body) = parens.body() {
                return self.track_node(&body);
            }
            return RubyType::nil_class();
        }

        // Handle interpolated strings
        if node.as_interpolated_string_node().is_some() {
            return RubyType::string();
        }

        RubyType::Unknown
    }

    /// Infer nested collection literals while preserving a shape-construction
    /// failure from any depth. Non-collection expressions retain the existing
    /// resolver and therefore represent ordinary incompleteness as
    /// `RubyType::Unknown`, not as a shape-bound error.
    pub(in crate::inference::type_tracker) fn infer_collection_literal_type(
        &mut self,
        node: &Node<'_>,
    ) -> Option<Result<RubyType, ShapeConstructionError>> {
        if let Some(hash) = node.as_hash_node() {
            return Some(infer_hash_literal_type_fallible(&hash, |value| {
                self.infer_collection_literal_type(value)
                    .unwrap_or_else(|| Ok(self.infer_expression(value)))
            }));
        }
        node.as_array_node().map(|array| {
            infer_array_literal_type_fallible(&array, |value| {
                self.infer_collection_literal_type(value)
                    .unwrap_or_else(|| Ok(self.infer_expression(value)))
            })
        })
    }
}
