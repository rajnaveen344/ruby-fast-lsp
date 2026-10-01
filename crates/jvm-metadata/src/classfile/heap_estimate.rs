//! Conservative owned-heap estimates for retained classfile metadata.

use super::model::{
    AnnotationInfo, ClassFile, InnerClassInfo, MemberInfo, MethodParameter, RecordComponentInfo,
};
use std::mem::size_of;

impl ClassFile {
    /// Conservative owned-heap estimate used to bound process-local immutable
    /// metadata retention. It counts allocation capacities, not merely logical
    /// lengths, and includes the fixed `ClassFile` allocation itself.
    pub fn estimated_heap_bytes(&self) -> u64 {
        let mut bytes = size_of::<Self>();
        add_capacity(&mut bytes, self.name.capacity(), "class name");
        add_optional_string(&mut bytes, self.super_name.as_ref(), "superclass name");
        add_string_vector(&mut bytes, &self.interfaces, "interface names");
        add_member_vector(&mut bytes, &self.fields, "field metadata");
        add_member_vector(&mut bytes, &self.methods, "method metadata");
        add_optional_string(&mut bytes, self.source_file.as_ref(), "source file");
        add_optional_string(&mut bytes, self.signature.as_ref(), "class signature");
        add_annotation_vector(&mut bytes, &self.annotations, "class annotations");

        add_vector_allocation(
            &mut bytes,
            self.inner_classes.capacity(),
            size_of::<InnerClassInfo>(),
            "inner-class metadata",
        );
        for inner in &self.inner_classes {
            add_capacity(&mut bytes, inner.inner_class.capacity(), "inner class name");
            add_optional_string(&mut bytes, inner.outer_class.as_ref(), "outer class name");
            add_optional_string(&mut bytes, inner.inner_name.as_ref(), "inner simple name");
        }

        add_vector_allocation(
            &mut bytes,
            self.record_components.capacity(),
            size_of::<RecordComponentInfo>(),
            "record-component metadata",
        );
        for component in &self.record_components {
            add_capacity(
                &mut bytes,
                component.name.capacity(),
                "record component name",
            );
            add_capacity(
                &mut bytes,
                component.descriptor.capacity(),
                "record component descriptor",
            );
            add_optional_string(
                &mut bytes,
                component.signature.as_ref(),
                "record component signature",
            );
            add_annotation_vector(
                &mut bytes,
                &component.annotations,
                "record component annotations",
            );
        }

        add_optional_string(&mut bytes, self.module_name.as_ref(), "module name");
        u64::try_from(bytes).expect(
            "INVARIANT VIOLATED: a JVM ClassFile heap estimate does not fit u64. This is a bug because one parsed class cannot exceed the process address space. Fix: inspect class metadata bounds and weight arithmetic.",
        )
    }
}

fn add_member_vector(bytes: &mut usize, members: &Vec<MemberInfo>, label: &'static str) {
    add_vector_allocation(bytes, members.capacity(), size_of::<MemberInfo>(), label);
    for member in members {
        add_capacity(bytes, member.name.capacity(), "member name");
        add_capacity(bytes, member.descriptor.capacity(), "member descriptor");
        add_optional_string(bytes, member.signature.as_ref(), "member signature");
        add_string_vector(bytes, &member.exceptions, "member exceptions");
        add_vector_allocation(
            bytes,
            member.parameters.capacity(),
            size_of::<MethodParameter>(),
            "method parameters",
        );
        for parameter in &member.parameters {
            add_capacity(bytes, parameter.name.capacity(), "method parameter name");
        }
        add_annotation_vector(bytes, &member.annotations, "member annotations");
    }
}

fn add_annotation_vector(
    bytes: &mut usize,
    annotations: &Vec<AnnotationInfo>,
    label: &'static str,
) {
    add_vector_allocation(
        bytes,
        annotations.capacity(),
        size_of::<AnnotationInfo>(),
        label,
    );
    for annotation in annotations {
        add_capacity(
            bytes,
            annotation.descriptor.capacity(),
            "annotation descriptor",
        );
    }
}

fn add_string_vector(bytes: &mut usize, values: &Vec<String>, label: &'static str) {
    add_vector_allocation(bytes, values.capacity(), size_of::<String>(), label);
    for value in values {
        add_capacity(bytes, value.capacity(), label);
    }
}

fn add_optional_string(bytes: &mut usize, value: Option<&String>, label: &'static str) {
    if let Some(value) = value {
        add_capacity(bytes, value.capacity(), label);
    }
}

fn add_vector_allocation(
    bytes: &mut usize,
    capacity: usize,
    element_size: usize,
    label: &'static str,
) {
    let allocation = capacity.checked_mul(element_size).unwrap_or_else(|| {
        panic!(
            "INVARIANT VIOLATED: JVM {label} allocation weight overflowed usize. This is a bug because parsed metadata is bounded by the process address space. Fix: inspect vector capacity and element-size accounting."
        )
    });
    add_capacity(bytes, allocation, label);
}

fn add_capacity(bytes: &mut usize, capacity: usize, label: &'static str) {
    *bytes = bytes.checked_add(capacity).unwrap_or_else(|| {
        panic!(
            "INVARIANT VIOLATED: JVM {label} heap weight overflowed usize. This is a bug because parsed metadata is bounded by the process address space. Fix: inspect nested metadata weight accounting."
        )
    });
}
