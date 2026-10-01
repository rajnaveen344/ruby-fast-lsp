//! Bounded JVM classfile parsing into declaration metadata.

mod annotations;
mod class_attributes;
mod constant_pool;
mod heap_estimate;
mod members;
mod model;
mod reader;

use members::MemberKind;
use reader::{
    parse_signature_attribute, require_finished, require_unique_attribute, ClassParser, Cursor,
};

pub use model::{
    AnnotationInfo, ClassFile, ClassKind, ClassLimits, InnerClassInfo, MemberInfo, MethodParameter,
    RecordComponentInfo, Visibility,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetadataError {
    InvalidMagic,
    Truncated,
    LimitExceeded(&'static str),
    InvalidConstantPool(&'static str),
    InvalidIndex(u16),
    InvalidUtf8,
    InvalidAttribute(&'static str),
    DuplicateAttribute(String),
    InvalidDescriptor,
    TrailingBytes,
}

pub fn parse_class(bytes: &[u8], limits: ClassLimits) -> Result<ClassFile, MetadataError> {
    if bytes.len() > limits.max_class_bytes {
        return Err(MetadataError::LimitExceeded("class bytes"));
    }
    let mut parser = ClassParser::new(bytes, limits);
    if parser.cursor.u4()? != 0xcafebabe {
        return Err(MetadataError::InvalidMagic);
    }
    let minor_version = parser.cursor.u2()?;
    let major_version = parser.cursor.u2()?;
    let constant_pool = parser.parse_constant_pool()?;
    let access_flags = parser.cursor.u2()?;
    let this_class = parser.cursor.u2()?;
    let super_class = parser.cursor.u2()?;
    let name = constant_pool.class_name(this_class)?;
    let super_name = if super_class == 0 {
        None
    } else {
        Some(constant_pool.class_name(super_class)?)
    };

    let interface_count = parser.bounded_count("interfaces")?;
    let mut interfaces = Vec::with_capacity(interface_count);
    for _ in 0..interface_count {
        interfaces.push(constant_pool.class_name(parser.cursor.u2()?)?);
    }
    let fields = parser.parse_members(&constant_pool, "fields", MemberKind::Field)?;
    let methods = parser.parse_members(&constant_pool, "methods", MemberKind::Method)?;

    let class_attribute_count = parser.bounded_attribute_count()?;
    let mut source_file = None;
    let mut signature = None;
    let mut annotations = Vec::new();
    let mut inner_classes = Vec::new();
    let mut record_components = Vec::new();
    let mut module_name = None;
    let mut unique_attributes = std::collections::HashSet::new();
    for _ in 0..class_attribute_count {
        let attribute_name = constant_pool.utf8(parser.cursor.u2()?)?;
        let attribute = parser.attribute_bytes()?;
        match attribute_name {
            "SourceFile" => {
                require_unique_attribute(&mut unique_attributes, attribute_name)?;
                let mut attribute_cursor = Cursor::new(attribute);
                source_file = Some(constant_pool.utf8(attribute_cursor.u2()?)?.to_string());
                require_finished(&attribute_cursor)?;
            }
            "Signature" => {
                require_unique_attribute(&mut unique_attributes, attribute_name)?;
                signature = Some(parse_signature_attribute(attribute, &constant_pool)?);
            }
            "RuntimeVisibleAnnotations" | "RuntimeInvisibleAnnotations" => {
                annotations.extend(parser.parse_annotations(attribute, &constant_pool)?);
            }
            "InnerClasses" => {
                require_unique_attribute(&mut unique_attributes, attribute_name)?;
                inner_classes = parser.parse_inner_classes(attribute, &constant_pool)?;
            }
            "Record" => {
                require_unique_attribute(&mut unique_attributes, attribute_name)?;
                record_components = parser.parse_record_components(attribute, &constant_pool)?;
            }
            "Module" => {
                require_unique_attribute(&mut unique_attributes, attribute_name)?;
                module_name = Some(parser.parse_module_name(attribute, &constant_pool)?);
            }
            "BootstrapMethods"
            | "Deprecated"
            | "EnclosingMethod"
            | "NestHost"
            | "NestMembers"
            | "PermittedSubclasses"
            | "RuntimeVisibleTypeAnnotations"
            | "RuntimeInvisibleTypeAnnotations"
            | "Synthetic" => {}
            _ => {}
        }
    }
    if !parser.cursor.is_finished() {
        return Err(MetadataError::TrailingBytes);
    }

    Ok(ClassFile {
        minor_version,
        major_version,
        access_flags,
        name,
        super_name,
        interfaces,
        fields,
        methods,
        source_file,
        signature,
        annotations,
        inner_classes,
        record_components,
        module_name,
    })
}

#[cfg(test)]
mod tests;
