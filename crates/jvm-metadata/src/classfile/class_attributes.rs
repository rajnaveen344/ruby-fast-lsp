//! Class-level InnerClasses, Record, and Module attributes.
//!
//! None of their contents is retained beyond the record flag, but each is
//! still bounded and validated so malformed classfiles are rejected.

use super::constant_pool::ConstantPool;
use super::reader::{
    bounded_nested_count, require_finished, require_unique_attribute, validate_signature_attribute,
    ClassParser, Cursor,
};
use super::MetadataError;

impl ClassParser<'_, '_> {
    pub(super) fn validate_inner_classes(
        &self,
        bytes: &[u8],
        constant_pool: &ConstantPool,
    ) -> Result<(), MetadataError> {
        let mut cursor = Cursor::new(bytes);
        let count =
            bounded_nested_count(&mut cursor, self.limits.max_members, "inner class entries")?;
        for _ in 0..count {
            let inner_index = cursor.u2()?;
            if inner_index == 0 {
                return Err(MetadataError::InvalidAttribute(
                    "InnerClasses entry has zero inner class",
                ));
            }
            let outer_index = cursor.u2()?;
            let name_index = cursor.u2()?;
            constant_pool.class_name(inner_index)?;
            if outer_index != 0 {
                constant_pool.class_name(outer_index)?;
            }
            if name_index != 0 {
                constant_pool.utf8(name_index)?;
            }
            cursor.u2()?;
        }
        require_finished(&cursor)
    }

    /// Returns the number of record components.
    pub(super) fn validate_record_components(
        &mut self,
        bytes: &[u8],
        constant_pool: &ConstantPool,
    ) -> Result<usize, MetadataError> {
        let mut cursor = Cursor::new(bytes);
        let count =
            bounded_nested_count(&mut cursor, self.limits.max_members, "record components")?;
        for _ in 0..count {
            constant_pool.utf8(cursor.u2()?)?;
            let descriptor = constant_pool.utf8(cursor.u2()?)?;
            crate::descriptor::parse_field_descriptor(descriptor)
                .map_err(|_| MetadataError::InvalidDescriptor)?;
            let attribute_count = self.bounded_attribute_count_from(&mut cursor)?;
            let mut unique_attributes = std::collections::HashSet::new();
            for _ in 0..attribute_count {
                let attribute_name = constant_pool.utf8(cursor.u2()?)?;
                let length = usize::try_from(cursor.u4()?)
                    .map_err(|_| MetadataError::LimitExceeded("attribute bytes"))?;
                if length > self.limits.max_attribute_bytes {
                    return Err(MetadataError::LimitExceeded("attribute bytes"));
                }
                let attribute = cursor.take(length)?;
                match attribute_name {
                    "Signature" => {
                        require_unique_attribute(&mut unique_attributes, attribute_name)?;
                        validate_signature_attribute(attribute, constant_pool)?;
                    }
                    "RuntimeVisibleAnnotations" | "RuntimeInvisibleAnnotations" => {
                        self.validate_annotations(attribute, constant_pool)?;
                    }
                    "RuntimeVisibleTypeAnnotations" | "RuntimeInvisibleTypeAnnotations" => {}
                    _ => {}
                }
            }
        }
        require_finished(&cursor)?;
        Ok(count)
    }

    pub(super) fn validate_module(
        &self,
        bytes: &[u8],
        constant_pool: &ConstantPool,
    ) -> Result<(), MetadataError> {
        let mut cursor = Cursor::new(bytes);
        let module_index = cursor.u2()?;
        constant_pool.module_name(module_index)?;
        cursor.u2()?;
        let version_index = cursor.u2()?;
        if version_index != 0 {
            constant_pool.utf8(version_index)?;
        }
        let requires_count =
            bounded_nested_count(&mut cursor, self.limits.max_members, "module requires")?;
        for _ in 0..requires_count {
            cursor.u2()?;
            cursor.u2()?;
            let requires_version_index = cursor.u2()?;
            if requires_version_index != 0 {
                constant_pool.utf8(requires_version_index)?;
            }
        }
        parse_module_exports_or_opens(&mut cursor, self.limits.max_members, "module exports")?;
        parse_module_exports_or_opens(&mut cursor, self.limits.max_members, "module opens")?;
        let uses_count = bounded_nested_count(&mut cursor, self.limits.max_members, "module uses")?;
        for _ in 0..uses_count {
            cursor.u2()?;
        }
        let provides_count =
            bounded_nested_count(&mut cursor, self.limits.max_members, "module provides")?;
        for _ in 0..provides_count {
            cursor.u2()?;
            let implementation_count = bounded_nested_count(
                &mut cursor,
                self.limits.max_members,
                "module implementations",
            )?;
            for _ in 0..implementation_count {
                cursor.u2()?;
            }
        }
        require_finished(&cursor)
    }
}

fn parse_module_exports_or_opens(
    cursor: &mut Cursor<'_>,
    maximum: usize,
    name: &'static str,
) -> Result<(), MetadataError> {
    let count = bounded_nested_count(cursor, maximum, name)?;
    for _ in 0..count {
        cursor.u2()?;
        cursor.u2()?;
        let target_count = bounded_nested_count(cursor, maximum, name)?;
        for _ in 0..target_count {
            cursor.u2()?;
        }
    }
    Ok(())
}
