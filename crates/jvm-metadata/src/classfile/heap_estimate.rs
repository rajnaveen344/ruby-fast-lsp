//! Conservative owned-heap estimates for retained classfile metadata.

use super::model::{ClassFile, MemberInfo, MethodParameter};
use crate::invariant::ExpectInvariant;
use std::collections::HashSet;
use std::mem::size_of;
use std::sync::Arc;

/// Weighs shared strings. A standalone class counts every string occurrence;
/// an archive counts each shared allocation once.
pub(crate) struct StringWeigher {
    seen: Option<HashSet<*const u8>>,
}

impl StringWeigher {
    pub(crate) fn per_occurrence() -> Self {
        Self { seen: None }
    }

    pub(crate) fn deduplicating() -> Self {
        Self {
            seen: Some(HashSet::new()),
        }
    }

    fn add(&mut self, bytes: &mut usize, value: &Arc<str>, label: &'static str) {
        if let Some(seen) = self.seen.as_mut() {
            if !seen.insert(value.as_ptr()) {
                return;
            }
        }
        // `Arc<str>` stores strong and weak counts beside the bytes.
        add_capacity(bytes, 2 * size_of::<usize>(), label);
        add_capacity(bytes, value.len(), label);
    }

    fn add_optional(&mut self, bytes: &mut usize, value: Option<&Arc<str>>, label: &'static str) {
        if let Some(value) = value {
            self.add(bytes, value, label);
        }
    }

    fn add_slice(&mut self, bytes: &mut usize, values: &[Arc<str>], label: &'static str) {
        add_slice_allocation(bytes, values.len(), size_of::<Arc<str>>(), label);
        for value in values {
            self.add(bytes, value, label);
        }
    }
}

impl ClassFile {
    /// Conservative owned-heap estimate used to bound process-local immutable
    /// metadata retention. It counts every string occurrence as its own
    /// allocation and includes the fixed `ClassFile` allocation itself.
    pub fn estimated_heap_bytes(&self) -> u64 {
        u64::try_from(self.weigh(&mut StringWeigher::per_occurrence())).expect_invariant(
            "a JVM ClassFile heap estimate does not fit u64",
            "one parsed class cannot exceed the process address space",
            "inspect class metadata bounds and weight arithmetic",
        )
    }

    pub(crate) fn weigh(&self, strings: &mut StringWeigher) -> usize {
        let mut bytes = size_of::<Self>();
        strings.add(&mut bytes, &self.name, "class name");
        strings.add_optional(&mut bytes, self.super_name.as_ref(), "superclass name");
        strings.add_slice(&mut bytes, &self.interfaces, "interface names");
        add_members(&mut bytes, &self.fields, strings, "field metadata");
        add_members(&mut bytes, &self.methods, strings, "method metadata");
        strings.add_optional(&mut bytes, self.source_file.as_ref(), "source file");
        bytes
    }
}

fn add_members(
    bytes: &mut usize,
    members: &[MemberInfo],
    strings: &mut StringWeigher,
    label: &'static str,
) {
    add_slice_allocation(bytes, members.len(), size_of::<MemberInfo>(), label);
    for member in members {
        strings.add(bytes, &member.name, "member name");
        strings.add(bytes, &member.descriptor, "member descriptor");
        strings.add_slice(bytes, &member.exceptions, "member exceptions");
        add_slice_allocation(
            bytes,
            member.parameters.len(),
            size_of::<MethodParameter>(),
            "method parameters",
        );
        for parameter in &member.parameters {
            strings.add(bytes, &parameter.name, "method parameter name");
        }
    }
}

pub(crate) fn add_slice_allocation(
    bytes: &mut usize,
    len: usize,
    element_size: usize,
    label: &'static str,
) {
    let allocation = len.checked_mul(element_size).unwrap_or_else(|| {
        unreachable_invariant!(
            what = "JVM {label} allocation weight overflowed usize",
            why = "parsed metadata is bounded by the process address space",
            fix = "inspect slice length and element-size accounting",
            label = label,
        )
    });
    add_capacity(bytes, allocation, label);
}

pub(crate) fn add_capacity(bytes: &mut usize, capacity: usize, label: &'static str) {
    *bytes = bytes.checked_add(capacity).unwrap_or_else(|| {
        unreachable_invariant!(
            what = "JVM {label} heap weight overflowed usize",
            why = "parsed metadata is bounded by the process address space",
            fix = "inspect nested metadata weight accounting",
            label = label,
        )
    });
}
