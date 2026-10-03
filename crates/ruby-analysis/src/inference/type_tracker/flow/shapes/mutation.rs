use crate::core::{RubyType, UnknownReason};
use crate::inference::r#type::literal::literal_key;
use crate::inference::r#type::shape as shape_reads;
use crate::inference::type_tracker::flow::shapes::values::{
    merge_shape_types, shape_cleared, shape_field_keys, shape_frozen, shape_literal_read_type,
    shape_with_required_field, shape_without_field,
};
use crate::inference::type_tracker::TypeTracker;
use crate::invariant::ExpectInvariant;
use ruby_prism::*;
use std::collections::BTreeSet;

impl TypeTracker {
    pub(in crate::inference::type_tracker) fn infer_shape_read_from_type(
        &mut self,
        call: &CallNode<'_>,
        method_name: &str,
        receiver_type: &RubyType,
    ) -> Option<RubyType> {
        let argument_nodes = call
            .arguments()
            .map(|arguments| arguments.arguments().iter().collect::<Vec<_>>())
            .unwrap_or_default();
        let argument_types = argument_nodes
            .iter()
            .map(|argument| self.track_node(argument))
            .collect::<Vec<_>>();
        if let Some(read) = shape_reads::CallRead::select(
            method_name,
            &argument_nodes,
            &argument_types,
            call.block().is_some(),
        ) {
            return Some(read.apply(receiver_type).unwrap_or(RubyType::Unknown));
        }
        shape_reads::CallRead::is_read_method(method_name).then_some(RubyType::Unknown)
    }

    pub(in crate::inference::type_tracker) fn infer_shape_call_effect(
        &mut self,
        call: &CallNode<'_>,
        method_name: &str,
    ) -> Option<RubyType> {
        let receiver_identities = call
            .receiver()
            .map(|receiver| self.shape_identities_for_alias_expression(&receiver))
            .unwrap_or_default();
        let argument_nodes = call
            .arguments()
            .map(|arguments| arguments.arguments().iter().collect::<Vec<_>>())
            .unwrap_or_default();
        let mut escaped_argument_identities = BTreeSet::new();
        for argument in &argument_nodes {
            escaped_argument_identities
                .extend(self.shape_identities_in_escape_expression(argument));
        }

        if receiver_identities.is_empty() && escaped_argument_identities.is_empty() {
            return None;
        }

        // Ruby evaluates the receiver before explicit arguments. Identity
        // lookup alone is not a semantic read: track the receiver expression
        // so its exact read-site type (or invalidation reason) is installed
        // for hover, chained dispatch, and every other engine consumer.
        if let Some(receiver) = call.receiver() {
            self.track_node(&receiver);
        }

        // Evaluate explicit arguments before entering the method. This also
        // lets a nested shape-producing expression provide the exact value
        // used by a known mutation.
        let argument_types = argument_nodes
            .iter()
            .map(|argument| self.track_node(argument))
            .collect::<Vec<_>>();

        if receiver_identities.is_empty() {
            self.environment.invalidate_identities(
                &escaped_argument_identities,
                UnknownReason::MutableShapeInvalidated,
            );
            return None;
        }

        if let Some(read) = shape_reads::CallRead::select(
            method_name,
            &argument_nodes,
            &argument_types,
            call.block().is_some(),
        ) {
            return Some(
                self.shape_identity_type(&receiver_identities)
                    .and_then(|receiver_type| read.apply(&receiver_type))
                    .unwrap_or(RubyType::Unknown),
            );
        }

        match method_name {
            "[]=" if argument_nodes.len() == 2 => {
                let Some(key) = literal_key(&argument_nodes[0]) else {
                    self.environment.invalidate_identities(
                        &receiver_identities,
                        UnknownReason::MutableShapeInvalidated,
                    );
                    return Some(RubyType::Unknown);
                };
                let value_type = argument_types[1].clone();
                if value_type == RubyType::Unknown {
                    let affected = receiver_identities
                        .union(&escaped_argument_identities)
                        .copied()
                        .collect();
                    self.environment
                        .invalidate_identities(&affected, UnknownReason::MutableShapeInvalidated);
                    return Some(RubyType::Unknown);
                }
                self.environment
                    .detach_contained_shapes(&receiver_identities, Some(&key));
                if self
                    .transform_shape_identities(&receiver_identities, |shape| {
                        shape_with_required_field(shape, key.clone(), value_type.clone())
                    })
                    .is_err()
                {
                    return Some(RubyType::Unknown);
                }
                if !escaped_argument_identities.is_empty()
                    && self
                        .link_assigned_shape_child(
                            &receiver_identities,
                            &key,
                            &escaped_argument_identities,
                            &value_type,
                        )
                        .is_err()
                {
                    let affected = receiver_identities
                        .union(&escaped_argument_identities)
                        .copied()
                        .collect();
                    self.environment
                        .invalidate_identities(&affected, UnknownReason::MutableShapeInvalidated);
                    return Some(RubyType::Unknown);
                }
                Some(value_type)
            }
            "delete" if argument_nodes.len() == 1 => {
                let Some(key) = literal_key(&argument_nodes[0]) else {
                    self.environment.invalidate_identities(
                        &receiver_identities,
                        UnknownReason::MutableShapeInvalidated,
                    );
                    return Some(RubyType::Unknown);
                };
                let deleted_types = self
                    .shape_identity_type(&receiver_identities)
                    .ok()
                    .map(|receiver_type| shape_literal_read_type(&receiver_type, &key));
                self.environment
                    .detach_contained_shapes(&receiver_identities, Some(&key));
                if self
                    .transform_shape_identities(&receiver_identities, |shape| {
                        shape_without_field(shape, &key)
                    })
                    .is_err()
                {
                    return Some(RubyType::Unknown);
                }
                Some(deleted_types.unwrap_or(RubyType::Unknown))
            }
            "clear" if argument_nodes.is_empty() => {
                self.environment
                    .detach_contained_shapes(&receiver_identities, None);
                let result = self
                    .transform_shape_identities(&receiver_identities, shape_cleared)
                    .unwrap_or(RubyType::Unknown);
                Some(result)
            }
            "merge!" | "update" if argument_types.len() == 1 => {
                let right = argument_types[0].clone();
                let overwritten_keys = match shape_field_keys(&right) {
                    Ok(keys) => keys,
                    Err(reason) => {
                        self.environment
                            .invalidate_identities(&receiver_identities, reason);
                        return Some(RubyType::Unknown);
                    }
                };
                for key in &overwritten_keys {
                    self.environment
                        .detach_contained_shapes(&receiver_identities, Some(key));
                }
                if self
                    .transform_shape_identities(&receiver_identities, |left| {
                        merge_shape_types(left, &right, true)
                    })
                    .is_err()
                {
                    return Some(RubyType::Unknown);
                }
                if let Err(reason) = self.link_mutating_merge_children(
                    &receiver_identities,
                    &argument_nodes[0],
                    &overwritten_keys,
                ) {
                    let affected = receiver_identities
                        .union(&escaped_argument_identities)
                        .copied()
                        .collect();
                    self.environment.invalidate_identities(&affected, reason);
                    return Some(RubyType::Unknown);
                }
                Some(self.shape_identity_type(&receiver_identities).expect_invariant(
                    "mutating merge linking succeeded but the receiver shape is not proven",
                    "the helper validates the receiver after linking",
                    "return an error from link_mutating_merge_children when enforcement invalidates it",
                ))
            }
            "merge" if argument_types.len() == 1 => {
                let left = self
                    .shape_identity_type(&receiver_identities)
                    .unwrap_or(RubyType::Unknown);
                let merged = merge_shape_types(&left, &argument_types[0], false)
                    .unwrap_or(RubyType::Unknown);
                Some(merged)
            }
            "freeze" if argument_nodes.is_empty() => {
                let result = self
                    .transform_shape_identities(&receiver_identities, shape_frozen)
                    .unwrap_or(RubyType::Unknown);
                Some(result)
            }
            // These calls do not change the receiver's field set. Phase 4
            // owns their precise results; retaining identity here merely
            // prevents a read from being mistaken for an unsupported mutator.
            "[]" | "fetch" | "dig" | "key?" | "has_key?" | "include?" | "member?" | "keys"
            | "values" | "each" | "each_pair" | "each_key" | "each_value" | "empty?" | "length"
            | "size" | "to_h" | "inspect" | "hash" | "eql?" | "==" | "merge" => None,
            _ => {
                let affected = receiver_identities
                    .union(&escaped_argument_identities)
                    .copied()
                    .collect();
                self.environment
                    .invalidate_identities(&affected, UnknownReason::MutableShapeInvalidated);
                Some(RubyType::Unknown)
            }
        }
    }
}
