use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;
use std::mem::size_of;

use crate::{FullyQualifiedName, RubyType, TypeSubject};

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
        RubyType::Array(types) | RubyType::Union(types) => {
            vec_payload_bytes(types) + types.iter().map(ruby_type_heap_bytes).sum::<usize>()
        }
        RubyType::Hash(keys, values) => {
            vec_payload_bytes(keys)
                + vec_payload_bytes(values)
                + keys.iter().map(ruby_type_heap_bytes).sum::<usize>()
                + values.iter().map(ruby_type_heap_bytes).sum::<usize>()
        }
        RubyType::Unknown => 0,
    }
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
