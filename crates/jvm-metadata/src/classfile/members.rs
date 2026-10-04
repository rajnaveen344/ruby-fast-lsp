//! Field and method declarations, including Code-derived parameter names.

use super::constant_pool::ConstantPool;
use super::model::{ClassLimits, MemberInfo, MethodParameter};
use super::reader::{
    bounded_nested_count, require_finished, require_unique_attribute, validate_signature_attribute,
    ClassParser, Cursor,
};
use super::MetadataError;
use std::sync::Arc;

impl ClassParser<'_, '_> {
    pub(super) fn parse_members(
        &mut self,
        constant_pool: &ConstantPool,
        limit_name: &'static str,
        kind: MemberKind,
    ) -> Result<Box<[MemberInfo]>, MetadataError> {
        let count = self.bounded_count(limit_name)?;
        let mut members = Vec::with_capacity(count);
        for _ in 0..count {
            let access_flags = self.cursor.u2()?;
            let name = constant_pool.utf8(self.cursor.u2()?)?;
            let descriptor = constant_pool.utf8(self.cursor.u2()?)?;
            match kind {
                MemberKind::Field => {
                    crate::descriptor::parse_field_descriptor(descriptor)
                        .map_err(|_| MetadataError::InvalidDescriptor)?;
                }
                MemberKind::Method => {
                    crate::descriptor::parse_method_descriptor(descriptor)
                        .map_err(|_| MetadataError::InvalidDescriptor)?;
                }
            }
            let attribute_count = self.bounded_attribute_count()?;
            let mut exceptions = Vec::new();
            let mut parameters = Vec::new();
            let mut first_line = None;
            let mut local_variables = Vec::new();
            let mut unique_attributes = std::collections::HashSet::new();
            for _ in 0..attribute_count {
                let attribute_name = constant_pool.utf8(self.cursor.u2()?)?;
                let attribute = self.attribute_bytes()?;
                match attribute_name {
                    "Signature" => {
                        require_unique_attribute(&mut unique_attributes, attribute_name)?;
                        validate_signature_attribute(attribute, constant_pool)?;
                    }
                    "Exceptions" => {
                        require_unique_attribute(&mut unique_attributes, attribute_name)?;
                        exceptions = self.parse_exceptions(attribute, constant_pool)?;
                    }
                    "MethodParameters" => {
                        require_unique_attribute(&mut unique_attributes, attribute_name)?;
                        parameters = self.parse_method_parameters(attribute, constant_pool)?;
                    }
                    "RuntimeVisibleAnnotations" | "RuntimeInvisibleAnnotations" => {
                        self.validate_annotations(attribute, constant_pool)?;
                    }
                    "Code" => {
                        require_unique_attribute(&mut unique_attributes, attribute_name)?;
                        let code = self.parse_code_metadata(attribute, constant_pool)?;
                        first_line = code.first_line;
                        local_variables = code.local_variables;
                    }
                    "AnnotationDefault"
                    | "ConstantValue"
                    | "Deprecated"
                    | "RuntimeVisibleParameterAnnotations"
                    | "RuntimeInvisibleParameterAnnotations"
                    | "RuntimeVisibleTypeAnnotations"
                    | "RuntimeInvisibleTypeAnnotations"
                    | "Synthetic" => {}
                    _ => {}
                }
            }
            if kind == MemberKind::Method {
                let method_descriptor = crate::descriptor::parse_method_descriptor(descriptor)
                    .map_err(|_| MetadataError::InvalidDescriptor)?;
                if parameters.len() > method_descriptor.parameters.len() {
                    return Err(MetadataError::InvalidAttribute(
                        "MethodParameters exceeds descriptor arity",
                    ));
                }
                let mut slot = if access_flags & 0x0008 == 0 { 1 } else { 0 };
                for (index, parameter_type) in method_descriptor.parameters.iter().enumerate() {
                    if index >= parameters.len() {
                        let debug_name = local_variables
                            .iter()
                            .find(|local| local.slot == slot)
                            .map(|local| Arc::clone(&local.name));
                        parameters.push(MethodParameter {
                            name: debug_name
                                .unwrap_or_else(|| self.strings.intern(&format!("arg{index}"))),
                        });
                    }
                    slot += match parameter_type {
                        crate::descriptor::JvmType::Long | crate::descriptor::JvmType::Double => 2,
                        crate::descriptor::JvmType::Byte
                        | crate::descriptor::JvmType::Char
                        | crate::descriptor::JvmType::Float
                        | crate::descriptor::JvmType::Int
                        | crate::descriptor::JvmType::Short
                        | crate::descriptor::JvmType::Boolean
                        | crate::descriptor::JvmType::Object(_)
                        | crate::descriptor::JvmType::Array(_) => 1,
                        crate::descriptor::JvmType::Void => {
                            return Err(MetadataError::InvalidDescriptor)
                        }
                    };
                }
                for index in parameters.len()..method_descriptor.parameters.len() {
                    parameters.push(MethodParameter {
                        name: self.strings.intern(&format!("arg{index}")),
                    });
                }
            }
            members.push(MemberInfo {
                access_flags,
                first_line,
                name: self.strings.intern(name),
                descriptor: self.strings.intern(descriptor),
                exceptions: exceptions.into_boxed_slice(),
                parameters: parameters.into_boxed_slice(),
            });
        }
        Ok(members.into_boxed_slice())
    }

    fn parse_exceptions(
        &mut self,
        bytes: &[u8],
        constant_pool: &ConstantPool,
    ) -> Result<Vec<Arc<str>>, MetadataError> {
        let mut cursor = Cursor::new(bytes);
        let count = bounded_nested_count(&mut cursor, self.limits.max_members, "exceptions")?;
        let mut exceptions = Vec::with_capacity(count);
        for _ in 0..count {
            let exception = constant_pool.class_name(cursor.u2()?)?;
            exceptions.push(self.strings.intern(exception));
        }
        require_finished(&cursor)?;
        Ok(exceptions)
    }

    fn parse_method_parameters(
        &mut self,
        bytes: &[u8],
        constant_pool: &ConstantPool,
    ) -> Result<Vec<MethodParameter>, MetadataError> {
        let mut cursor = Cursor::new(bytes);
        let count = usize::from(cursor.u1()?);
        if count > self.limits.max_members {
            return Err(MetadataError::LimitExceeded("method parameters"));
        }
        let mut parameters = Vec::with_capacity(count);
        for index in 0..count {
            let name_index = cursor.u2()?;
            let name = if name_index == 0 {
                self.strings.intern(&format!("arg{index}"))
            } else {
                self.strings.intern(constant_pool.utf8(name_index)?)
            };
            // Parameter access flags (final, synthetic, mandated) are not retained.
            cursor.u2()?;
            parameters.push(MethodParameter { name });
        }
        require_finished(&cursor)?;
        Ok(parameters)
    }

    fn parse_code_metadata(
        &mut self,
        bytes: &[u8],
        constant_pool: &ConstantPool,
    ) -> Result<CodeMetadata, MetadataError> {
        let mut cursor = Cursor::new(bytes);
        cursor.u2()?;
        cursor.u2()?;
        let code_length = usize::try_from(cursor.u4()?)
            .map_err(|_| MetadataError::LimitExceeded("code bytes"))?;
        if code_length > self.limits.max_attribute_bytes {
            return Err(MetadataError::LimitExceeded("code bytes"));
        }
        cursor.take(code_length)?;
        let exception_count =
            bounded_nested_count(&mut cursor, self.limits.max_members, "code exceptions")?;
        cursor.take(
            exception_count
                .checked_mul(8)
                .ok_or(MetadataError::LimitExceeded("code exceptions"))?,
        )?;
        let attribute_count = self.bounded_attribute_count_from(&mut cursor)?;
        let mut first_line = None;
        let mut local_variables = Vec::new();
        let mut saw_line_table = false;
        let mut saw_local_variable_table = false;
        for _ in 0..attribute_count {
            let name = constant_pool.utf8(cursor.u2()?)?;
            let length = usize::try_from(cursor.u4()?)
                .map_err(|_| MetadataError::LimitExceeded("attribute bytes"))?;
            if length > self.limits.max_attribute_bytes {
                return Err(MetadataError::LimitExceeded("attribute bytes"));
            }
            let attribute = cursor.take(length)?;
            if name == "LineNumberTable" {
                if saw_line_table {
                    return Err(MetadataError::DuplicateAttribute(name.to_string()));
                }
                saw_line_table = true;
                first_line = parse_first_line(attribute, self.limits)?;
            } else if name == "LocalVariableTable" {
                if saw_local_variable_table {
                    return Err(MetadataError::DuplicateAttribute(name.to_string()));
                }
                saw_local_variable_table = true;
                local_variables =
                    parse_local_variables(attribute, constant_pool, self.limits, self.strings)?;
            }
        }
        require_finished(&cursor)?;
        Ok(CodeMetadata {
            first_line,
            local_variables,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MemberKind {
    Field,
    Method,
}

struct CodeMetadata {
    first_line: Option<u16>,
    local_variables: Vec<LocalVariableInfo>,
}

/// A local live from method entry; only those can name parameters.
struct LocalVariableInfo {
    slot: usize,
    name: Arc<str>,
}

fn parse_first_line(bytes: &[u8], limits: ClassLimits) -> Result<Option<u16>, MetadataError> {
    let mut cursor = Cursor::new(bytes);
    let count = bounded_nested_count(&mut cursor, limits.max_members, "line number table entries")?;
    let mut first_line = None;
    for _ in 0..count {
        cursor.u2()?;
        let line = cursor.u2()?;
        first_line = Some(first_line.map_or(line, |current: u16| current.min(line)));
    }
    require_finished(&cursor)?;
    Ok(first_line)
}

fn parse_local_variables(
    bytes: &[u8],
    constant_pool: &ConstantPool,
    limits: ClassLimits,
    strings: &mut super::strings::JvmStringInterner,
) -> Result<Vec<LocalVariableInfo>, MetadataError> {
    let mut cursor = Cursor::new(bytes);
    let count = bounded_nested_count(
        &mut cursor,
        limits.max_members,
        "local variable table entries",
    )?;
    let mut locals = Vec::with_capacity(count);
    for _ in 0..count {
        let start_pc = cursor.u2()?;
        cursor.u2()?;
        let name = constant_pool.utf8(cursor.u2()?)?;
        let descriptor = constant_pool.utf8(cursor.u2()?)?;
        crate::descriptor::parse_field_descriptor(descriptor)
            .map_err(|_| MetadataError::InvalidDescriptor)?;
        let slot = usize::from(cursor.u2()?);
        // Only locals live at method entry can name parameters; the rest
        // are validated and dropped.
        if start_pc != 0 {
            continue;
        }
        locals.push(LocalVariableInfo {
            slot,
            name: strings.intern(name),
        });
    }
    require_finished(&cursor)?;
    Ok(locals)
}
