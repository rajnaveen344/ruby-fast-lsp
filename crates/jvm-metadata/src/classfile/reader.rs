//! Byte cursor, parser state, and shared attribute bounds.

use super::constant_pool::ConstantPool;
use super::model::ClassLimits;
use super::strings::JvmStringInterner;
use super::MetadataError;

pub(super) struct ClassParser<'a, 's> {
    pub(super) cursor: Cursor<'a>,
    pub(super) limits: ClassLimits,
    pub(super) attributes_seen: usize,
    pub(super) strings: &'s mut JvmStringInterner,
}

impl<'a, 's> ClassParser<'a, 's> {
    pub(super) fn new(
        bytes: &'a [u8],
        limits: ClassLimits,
        strings: &'s mut JvmStringInterner,
    ) -> Self {
        Self {
            cursor: Cursor::new(bytes),
            limits,
            attributes_seen: 0,
            strings,
        }
    }

    pub(super) fn bounded_count(&mut self, name: &'static str) -> Result<usize, MetadataError> {
        let count = usize::from(self.cursor.u2()?);
        if count > self.limits.max_members {
            return Err(MetadataError::LimitExceeded(name));
        }
        Ok(count)
    }

    pub(super) fn bounded_attribute_count(&mut self) -> Result<usize, MetadataError> {
        let count = usize::from(self.cursor.u2()?);
        self.record_attribute_count(count)?;
        Ok(count)
    }

    pub(super) fn bounded_attribute_count_from(
        &mut self,
        cursor: &mut Cursor<'_>,
    ) -> Result<usize, MetadataError> {
        let count = usize::from(cursor.u2()?);
        self.record_attribute_count(count)?;
        Ok(count)
    }

    pub(super) fn record_attribute_count(&mut self, count: usize) -> Result<(), MetadataError> {
        self.attributes_seen = self
            .attributes_seen
            .checked_add(count)
            .ok_or(MetadataError::LimitExceeded("attributes"))?;
        if self.attributes_seen > self.limits.max_attributes {
            return Err(MetadataError::LimitExceeded("attributes"));
        }
        Ok(())
    }

    pub(super) fn attribute_bytes(&mut self) -> Result<&'a [u8], MetadataError> {
        let length = usize::try_from(self.cursor.u4()?)
            .map_err(|_| MetadataError::LimitExceeded("attribute bytes"))?;
        if length > self.limits.max_attribute_bytes {
            return Err(MetadataError::LimitExceeded("attribute bytes"));
        }
        self.cursor.take(length)
    }
}

pub(super) fn require_unique_attribute(
    seen: &mut std::collections::HashSet<String>,
    name: &str,
) -> Result<(), MetadataError> {
    if !seen.insert(name.to_string()) {
        return Err(MetadataError::DuplicateAttribute(name.to_string()));
    }
    Ok(())
}

pub(super) fn require_finished(cursor: &Cursor<'_>) -> Result<(), MetadataError> {
    if !cursor.is_finished() {
        return Err(MetadataError::InvalidAttribute(
            "attribute contains trailing bytes",
        ));
    }
    Ok(())
}

/// Generic signatures are not retained; the attribute must still name a
/// valid Utf8 constant and contain nothing else.
pub(super) fn validate_signature_attribute(
    bytes: &[u8],
    constant_pool: &ConstantPool,
) -> Result<(), MetadataError> {
    let mut cursor = Cursor::new(bytes);
    constant_pool.utf8(cursor.u2()?)?;
    require_finished(&cursor)
}

pub(super) fn bounded_nested_count(
    cursor: &mut Cursor<'_>,
    maximum: usize,
    name: &'static str,
) -> Result<usize, MetadataError> {
    let count = usize::from(cursor.u2()?);
    if count > maximum {
        return Err(MetadataError::LimitExceeded(name));
    }
    Ok(count)
}

pub(super) struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub(super) fn u1(&mut self) -> Result<u8, MetadataError> {
        Ok(self.take(1)?[0])
    }

    pub(super) fn u2(&mut self) -> Result<u16, MetadataError> {
        let bytes = self.take(2)?;
        Ok(u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    pub(super) fn u4(&mut self) -> Result<u32, MetadataError> {
        let bytes = self.take(4)?;
        Ok(u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    pub(super) fn take(&mut self, length: usize) -> Result<&'a [u8], MetadataError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(MetadataError::Truncated)?;
        let result = self
            .bytes
            .get(self.offset..end)
            .ok_or(MetadataError::Truncated)?;
        self.offset = end;
        Ok(result)
    }

    pub(super) fn is_finished(&self) -> bool {
        self.offset == self.bytes.len()
    }
}
