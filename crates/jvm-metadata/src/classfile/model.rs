//! Public classfile declaration metadata, limits, and flag accessors.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClassLimits {
    pub max_class_bytes: usize,
    pub max_constant_pool_entries: usize,
    pub max_members: usize,
    pub max_attributes: usize,
    pub max_attribute_bytes: usize,
    pub max_annotations: usize,
    pub max_annotation_depth: usize,
}

impl Default for ClassLimits {
    fn default() -> Self {
        Self {
            max_class_bytes: 16 * 1024 * 1024,
            max_constant_pool_entries: 65_535,
            max_members: 65_535,
            // Attribute counts are u16 per class, field, method, Code body, and
            // record component. Large generated APIs can legitimately exceed
            // 4,096 in aggregate while remaining far below the independent
            // class-byte and member bounds.
            max_attributes: 65_535,
            max_attribute_bytes: 8 * 1024 * 1024,
            max_annotations: 4_096,
            max_annotation_depth: 32,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberInfo {
    pub access_flags: u16,
    pub name: String,
    pub descriptor: String,
    pub signature: Option<String>,
    pub exceptions: Vec<String>,
    pub parameters: Vec<MethodParameter>,
    pub annotations: Vec<AnnotationInfo>,
    pub first_line: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Public,
    Protected,
    Private,
    Package,
}

impl MemberInfo {
    pub fn visibility(&self) -> Visibility {
        if self.access_flags & 0x0001 != 0 {
            Visibility::Public
        } else if self.access_flags & 0x0004 != 0 {
            Visibility::Protected
        } else if self.access_flags & 0x0002 != 0 {
            Visibility::Private
        } else {
            Visibility::Package
        }
    }

    pub fn is_static(&self) -> bool {
        self.access_flags & 0x0008 != 0
    }

    pub fn is_final(&self) -> bool {
        self.access_flags & 0x0010 != 0
    }

    pub fn is_abstract(&self) -> bool {
        self.access_flags & 0x0400 != 0
    }

    pub fn is_native(&self) -> bool {
        self.access_flags & 0x0100 != 0
    }

    pub fn is_varargs(&self) -> bool {
        self.access_flags & 0x0080 != 0
    }

    pub fn is_enum_constant(&self) -> bool {
        self.access_flags & 0x4000 != 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MethodParameter {
    pub name: String,
    pub access_flags: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnnotationInfo {
    pub descriptor: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InnerClassInfo {
    pub inner_class: String,
    pub outer_class: Option<String>,
    pub inner_name: Option<String>,
    pub access_flags: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordComponentInfo {
    pub name: String,
    pub descriptor: String,
    pub signature: Option<String>,
    pub annotations: Vec<AnnotationInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClassKind {
    Class,
    Interface,
    Annotation,
    Enum,
    Record,
    Module,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassFile {
    pub minor_version: u16,
    pub major_version: u16,
    pub access_flags: u16,
    pub name: String,
    pub super_name: Option<String>,
    pub interfaces: Vec<String>,
    pub fields: Vec<MemberInfo>,
    pub methods: Vec<MemberInfo>,
    pub source_file: Option<String>,
    pub signature: Option<String>,
    pub annotations: Vec<AnnotationInfo>,
    pub inner_classes: Vec<InnerClassInfo>,
    pub record_components: Vec<RecordComponentInfo>,
    pub module_name: Option<String>,
}

impl ClassFile {
    pub fn kind(&self) -> ClassKind {
        if self.access_flags & 0x8000 != 0 {
            ClassKind::Module
        } else if self.access_flags & 0x2000 != 0 {
            ClassKind::Annotation
        } else if self.access_flags & 0x4000 != 0 {
            ClassKind::Enum
        } else if !self.record_components.is_empty() {
            ClassKind::Record
        } else if self.access_flags & 0x0200 != 0 {
            ClassKind::Interface
        } else {
            ClassKind::Class
        }
    }
}
