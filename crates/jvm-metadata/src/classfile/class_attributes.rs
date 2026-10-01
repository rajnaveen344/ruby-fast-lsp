//! Class-level InnerClasses, Record, and Module attributes.

use super::constant_pool::ConstantPool;
use super::model::{InnerClassInfo, RecordComponentInfo};
use super::reader::{
    bounded_nested_count, parse_signature_attribute, require_finished, require_unique_attribute,
    ClassParser, Cursor,
};
use super::MetadataError;

impl<'a> ClassParser<'a> {
    pub(super) fn parse_inner_classes(
        &self,
        bytes: &[u8],
        constant_pool: &ConstantPool,
    ) -> Result<Vec<InnerClassInfo>, MetadataError> {
        let mut cursor = Cursor::new(bytes);
        let count =
            bounded_nested_count(&mut cursor, self.limits.max_members, "inner class entries")?;
        let mut classes = Vec::with_capacity(count);
        for _ in 0..count {
            let inner_index = cursor.u2()?;
            if inner_index == 0 {
                return Err(MetadataError::InvalidAttribute(
                    "InnerClasses entry has zero inner class",
                ));
            }
            let outer_index = cursor.u2()?;
            let name_index = cursor.u2()?;
            classes.push(InnerClassInfo {
                inner_class: constant_pool.class_name(inner_index)?,
                outer_class: if outer_index == 0 {
                    None
                } else {
                    Some(constant_pool.class_name(outer_index)?)
                },
                inner_name: if name_index == 0 {
                    None
                } else {
                    Some(constant_pool.utf8(name_index)?.to_string())
                },
                access_flags: cursor.u2()?,
            });
        }
        require_finished(&cursor)?;
        Ok(classes)
    }

    pub(super) fn parse_record_components(
        &mut self,
        bytes: &[u8],
        constant_pool: &ConstantPool,
    ) -> Result<Vec<RecordComponentInfo>, MetadataError> {
        let mut cursor = Cursor::new(bytes);
        let count =
            bounded_nested_count(&mut cursor, self.limits.max_members, "record components")?;
        let mut components = Vec::with_capacity(count);
        for _ in 0..count {
            let name = constant_pool.utf8(cursor.u2()?)?.to_string();
            let descriptor = constant_pool.utf8(cursor.u2()?)?.to_string();
            crate::descriptor::parse_field_descriptor(&descriptor)
                .map_err(|_| MetadataError::InvalidDescriptor)?;
            let attribute_count = self.bounded_attribute_count_from(&mut cursor)?;
            let mut signature = None;
            let mut annotations = Vec::new();
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
                        signature = Some(parse_signature_attribute(attribute, constant_pool)?);
                    }
                    "RuntimeVisibleAnnotations" | "RuntimeInvisibleAnnotations" => {
                        annotations.extend(self.parse_annotations(attribute, constant_pool)?);
                    }
                    "RuntimeVisibleTypeAnnotations" | "RuntimeInvisibleTypeAnnotations" => {}
                    _ => {}
                }
            }
            components.push(RecordComponentInfo {
                name,
                descriptor,
                signature,
                annotations,
            });
        }
        require_finished(&cursor)?;
        Ok(components)
    }

    pub(super) fn parse_module_name(
        &self,
        bytes: &[u8],
        constant_pool: &ConstantPool,
    ) -> Result<String, MetadataError> {
        let mut cursor = Cursor::new(bytes);
        let module_index = cursor.u2()?;
        let name = constant_pool.module_name(module_index)?.to_string();
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
        require_finished(&cursor)?;
        Ok(name)
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
