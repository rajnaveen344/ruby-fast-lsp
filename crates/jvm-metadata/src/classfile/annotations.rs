//! Runtime annotation attributes and bounded element-value skipping.

use super::constant_pool::ConstantPool;
use super::model::ClassLimits;
use super::reader::{bounded_nested_count, require_finished, ClassParser, Cursor};
use super::MetadataError;

impl ClassParser<'_, '_> {
    /// Annotations are not retained; they are still bounded and validated so
    /// malformed classfiles fail exactly as before.
    pub(super) fn validate_annotations(
        &self,
        bytes: &[u8],
        constant_pool: &ConstantPool,
    ) -> Result<(), MetadataError> {
        let mut cursor = Cursor::new(bytes);
        let count = bounded_nested_count(&mut cursor, self.limits.max_annotations, "annotations")?;
        for _ in 0..count {
            validate_annotation(&mut cursor, constant_pool, self.limits, 0)?;
        }
        require_finished(&cursor)
    }
}

fn validate_annotation(
    cursor: &mut Cursor<'_>,
    constant_pool: &ConstantPool,
    limits: ClassLimits,
    depth: usize,
) -> Result<(), MetadataError> {
    if depth > limits.max_annotation_depth {
        return Err(MetadataError::LimitExceeded("annotation depth"));
    }
    let descriptor = constant_pool.utf8(cursor.u2()?)?;
    crate::descriptor::parse_field_descriptor(descriptor)
        .map_err(|_| MetadataError::InvalidDescriptor)?;
    let pair_count =
        bounded_nested_count(cursor, limits.max_annotations, "annotation element pairs")?;
    for _ in 0..pair_count {
        constant_pool.utf8(cursor.u2()?)?;
        skip_annotation_value(cursor, constant_pool, limits, depth)?;
    }
    Ok(())
}

fn skip_annotation_value(
    cursor: &mut Cursor<'_>,
    constant_pool: &ConstantPool,
    limits: ClassLimits,
    depth: usize,
) -> Result<(), MetadataError> {
    match cursor.u1()? {
        b'B' | b'C' | b'D' | b'F' | b'I' | b'J' | b'S' | b'Z' | b's' | b'c' => {
            cursor.u2()?;
        }
        b'e' => {
            cursor.u2()?;
            cursor.u2()?;
        }
        b'@' => {
            validate_annotation(cursor, constant_pool, limits, depth + 1)?;
        }
        b'[' => {
            let count =
                bounded_nested_count(cursor, limits.max_annotations, "annotation array values")?;
            for _ in 0..count {
                skip_annotation_value(cursor, constant_pool, limits, depth + 1)?;
            }
        }
        _ => {
            return Err(MetadataError::InvalidAttribute(
                "unknown annotation element value tag",
            ))
        }
    }
    Ok(())
}
