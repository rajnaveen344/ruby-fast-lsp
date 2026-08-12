use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;
use std::mem::size_of;

use crate::{FullyQualifiedName, LiteralValue, RubyType, ShapeRest, ShapeType, TypeSubject};

pub fn map_table_bytes<K, V, S>(map: &HashMap<K, V, S>) -> usize
where
    S: BuildHasher,
{
    // Hashbrown stores control bytes separately/in-line depending on layout.
    // This is an intentionally conservative app-level estimate, not allocator truth.
    map.capacity() * (size_of::<K>() + size_of::<V>() + 1)
}

pub fn set_table_bytes<K, S>(set: &HashSet<K, S>) -> usize
where
    S: BuildHasher,
{
    // Hashbrown stores one value plus a control byte per occupied-capacity slot.
    set.capacity() * (size_of::<K>() + 1)
}

pub fn vec_payload_bytes<T>(values: &Vec<T>) -> usize {
    values.capacity() * size_of::<T>()
}

pub fn string_heap_bytes(value: &String) -> usize {
    value.capacity()
}

pub fn fqn_heap_bytes(fqn: &FullyQualifiedName) -> usize {
    match fqn {
        FullyQualifiedName::Namespace(parts, _)
        | FullyQualifiedName::Constant(parts)
        | FullyQualifiedName::Method(parts, _) => {
            if parts.spilled() {
                parts.capacity() * size_of::<crate::RubyConstant>()
            } else {
                0
            }
        }
        FullyQualifiedName::LocalVariable(_)
        | FullyQualifiedName::InstanceVariable(_)
        | FullyQualifiedName::ClassVariable(_)
        | FullyQualifiedName::GlobalVariable(_) => 0,
    }
}

pub fn ruby_type_heap_bytes(ruby_type: &RubyType) -> usize {
    match ruby_type {
        RubyType::Class(fqn)
        | RubyType::Module(fqn)
        | RubyType::ClassReference(fqn)
        | RubyType::ModuleReference(fqn) => fqn_heap_bytes(fqn),
        RubyType::Literal(value) => size_of::<LiteralValue>() + literal_value_heap_bytes(value),
        RubyType::Array(types) | RubyType::Union(types) => {
            vec_payload_bytes(types) + types.iter().map(ruby_type_heap_bytes).sum::<usize>()
        }
        RubyType::Hash(keys, values) => {
            vec_payload_bytes(keys)
                + vec_payload_bytes(values)
                + keys.iter().map(ruby_type_heap_bytes).sum::<usize>()
                + values.iter().map(ruby_type_heap_bytes).sum::<usize>()
        }
        RubyType::Shape(shape) => size_of::<ShapeType>() + shape_type_heap_bytes(shape),
        RubyType::Unknown => 0,
    }
}

fn literal_value_heap_bytes(value: &LiteralValue) -> usize {
    match value {
        LiteralValue::Symbol(value) | LiteralValue::String(value) => value.capacity(),
    }
}

fn shape_type_heap_bytes(shape: &ShapeType) -> usize {
    shape.fields_allocation_bytes()
        + shape
            .fields()
            .iter()
            .map(|field| field.key().heap_bytes() + ruby_type_heap_bytes(field.value()))
            .sum::<usize>()
        + shape.rest().map_or(0, |rest| {
            size_of::<ShapeRest>()
                + ruby_type_heap_bytes(rest.key())
                + ruby_type_heap_bytes(rest.value())
        })
}

pub fn type_subject_heap_bytes(subject: &TypeSubject) -> usize {
    match subject {
        TypeSubject::Constant(fqn) | TypeSubject::MethodReturn(fqn) => fqn_heap_bytes(fqn),
        TypeSubject::Local { name, .. } | TypeSubject::GlobalVariable(name) => {
            string_heap_bytes(name)
        }
        TypeSubject::InstanceVariable { owner, name }
        | TypeSubject::ClassVariable { owner, name }
        | TypeSubject::Parameter {
            method: owner,
            name,
        } => fqn_heap_bytes(owner) + string_heap_bytes(name),
        TypeSubject::Expression(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LiteralKey, ShapeExactness, ShapeField, ShapeStability};

    #[test]
    fn shape_deep_weight_counts_box_fields_keys_and_nested_literal_payloads() {
        let mut key = String::with_capacity(11);
        key.push_str("label");
        let mut value = String::with_capacity(17);
        value.push_str("ready");
        let shape = ShapeType::try_new(
            [ShapeField::required(
                LiteralKey::String(key),
                RubyType::Literal(Box::new(LiteralValue::String(value))),
            )],
            None,
            ShapeExactness::Exact,
            ShapeStability::TrackedMutable,
        )
        .unwrap();
        let ruby_type = RubyType::Shape(Box::new(shape));
        assert_eq!(
            ruby_type_heap_bytes(&ruby_type),
            size_of::<ShapeType>() + size_of::<ShapeField>() + 11 + size_of::<LiteralValue>() + 17
        );
    }
}
