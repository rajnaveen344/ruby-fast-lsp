//! Bounded JVM classfile parsing into declaration metadata.

mod annotations;
mod class_attributes;
mod constant_pool;
mod heap_estimate;
mod members;
mod model;
mod reader;
mod strings;

use members::MemberKind;
use reader::{
    require_finished, require_unique_attribute, validate_signature_attribute, ClassParser, Cursor,
};

pub(crate) use heap_estimate::{add_capacity, add_slice_allocation, StringWeigher};
pub use model::{ClassFile, ClassKind, ClassLimits, MemberInfo, MethodParameter, Visibility};
pub use strings::JvmStringInterner;

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
    parse_class_with_interner(bytes, limits, &mut JvmStringInterner::default())
}

/// Parse one class, sharing retained strings through `strings`. Archives pass
/// one interner for all their classes.
pub fn parse_class_with_interner(
    bytes: &[u8],
    limits: ClassLimits,
    strings: &mut JvmStringInterner,
) -> Result<ClassFile, MetadataError> {
    if bytes.len() > limits.max_class_bytes {
        return Err(MetadataError::LimitExceeded("class bytes"));
    }
    let mut parser = ClassParser::new(bytes, limits, strings);
    if parser.cursor.u4()? != 0xcafebabe {
        return Err(MetadataError::InvalidMagic);
    }
    let minor_version = parser.cursor.u2()?;
    let major_version = parser.cursor.u2()?;
    let constant_pool = parser.parse_constant_pool()?;
    let access_flags = parser.cursor.u2()?;
    let this_class = parser.cursor.u2()?;
    let super_class = parser.cursor.u2()?;
    let name = parser.strings.intern(constant_pool.class_name(this_class)?);
    let super_name = if super_class == 0 {
        None
    } else {
        Some(
            parser
                .strings
                .intern(constant_pool.class_name(super_class)?),
        )
    };

    let interface_count = parser.bounded_count("interfaces")?;
    let mut interfaces = Vec::with_capacity(interface_count);
    for _ in 0..interface_count {
        let interface = constant_pool.class_name(parser.cursor.u2()?)?;
        interfaces.push(parser.strings.intern(interface));
    }
    let fields = parser.parse_members(&constant_pool, "fields", MemberKind::Field)?;
    let methods = parser.parse_members(&constant_pool, "methods", MemberKind::Method)?;

    let class_attribute_count = parser.bounded_attribute_count()?;
    let mut source_file = None;
    let mut is_record = false;
    let mut unique_attributes = std::collections::HashSet::new();
    for _ in 0..class_attribute_count {
        let attribute_name = constant_pool.utf8(parser.cursor.u2()?)?;
        let attribute = parser.attribute_bytes()?;
        match attribute_name {
            "SourceFile" => {
                require_unique_attribute(&mut unique_attributes, attribute_name)?;
                let mut attribute_cursor = Cursor::new(attribute);
                source_file = Some(
                    parser
                        .strings
                        .intern(constant_pool.utf8(attribute_cursor.u2()?)?),
                );
                require_finished(&attribute_cursor)?;
            }
            "Signature" => {
                require_unique_attribute(&mut unique_attributes, attribute_name)?;
                validate_signature_attribute(attribute, &constant_pool)?;
            }
            "RuntimeVisibleAnnotations" | "RuntimeInvisibleAnnotations" => {
                parser.validate_annotations(attribute, &constant_pool)?;
            }
            "InnerClasses" => {
                require_unique_attribute(&mut unique_attributes, attribute_name)?;
                parser.validate_inner_classes(attribute, &constant_pool)?;
            }
            "Record" => {
                require_unique_attribute(&mut unique_attributes, attribute_name)?;
                is_record = parser.validate_record_components(attribute, &constant_pool)? > 0;
            }
            "Module" => {
                require_unique_attribute(&mut unique_attributes, attribute_name)?;
                parser.validate_module(attribute, &constant_pool)?;
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
        is_record,
        name,
        super_name,
        interfaces: interfaces.into_boxed_slice(),
        fields,
        methods,
        source_file,
    })
}

#[cfg(test)]
mod tests;
