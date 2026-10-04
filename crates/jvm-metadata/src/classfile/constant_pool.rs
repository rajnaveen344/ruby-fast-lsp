//! Constant pool decoding and typed entry lookup.

use super::reader::ClassParser;
use super::MetadataError;

impl ClassParser<'_, '_> {
    pub(super) fn parse_constant_pool(&mut self) -> Result<ConstantPool, MetadataError> {
        let count = usize::from(self.cursor.u2()?);
        if count == 0 {
            return Err(MetadataError::InvalidConstantPool(
                "constant pool count is zero",
            ));
        }
        if count > self.limits.max_constant_pool_entries {
            return Err(MetadataError::LimitExceeded("constant pool entries"));
        }
        let mut entries = Vec::with_capacity(count);
        entries.push(ConstantPoolEntry::Unusable);
        let mut index = 1usize;
        while index < count {
            let tag = self.cursor.u1()?;
            let entry = match tag {
                1 => {
                    let length = usize::from(self.cursor.u2()?);
                    let bytes = self.cursor.take(length)?;
                    let value = decode_modified_utf8(bytes)?;
                    ConstantPoolEntry::Utf8(value)
                }
                3 | 4 => {
                    self.cursor.take(4)?;
                    ConstantPoolEntry::Other
                }
                5 | 6 => {
                    self.cursor.take(8)?;
                    entries.push(ConstantPoolEntry::Other);
                    index += 1;
                    ConstantPoolEntry::Other
                }
                7 => ConstantPoolEntry::Class(self.cursor.u2()?),
                19 => ConstantPoolEntry::Module(self.cursor.u2()?),
                8 | 16 | 20 => {
                    self.cursor.take(2)?;
                    ConstantPoolEntry::Other
                }
                9 | 10 | 11 | 12 | 17 | 18 => {
                    self.cursor.take(4)?;
                    ConstantPoolEntry::Other
                }
                15 => {
                    self.cursor.take(3)?;
                    ConstantPoolEntry::Other
                }
                _ => {
                    return Err(MetadataError::InvalidConstantPool(
                        "unknown constant pool tag",
                    ))
                }
            };
            entries.push(entry);
            index += 1;
        }
        if entries.len() != count {
            return Err(MetadataError::InvalidConstantPool(
                "wide constant pool entry exceeds declared count",
            ));
        }
        Ok(ConstantPool { entries })
    }
}

enum ConstantPoolEntry {
    Unusable,
    Utf8(Option<String>),
    Class(u16),
    Module(u16),
    Other,
}

pub(super) struct ConstantPool {
    entries: Vec<ConstantPoolEntry>,
}

impl ConstantPool {
    fn entry(&self, index: u16) -> Result<&ConstantPoolEntry, MetadataError> {
        if index == 0 {
            return Err(MetadataError::InvalidIndex(index));
        }
        self.entries
            .get(usize::from(index))
            .ok_or(MetadataError::InvalidIndex(index))
    }

    pub(super) fn utf8(&self, index: u16) -> Result<&str, MetadataError> {
        match self.entry(index)? {
            ConstantPoolEntry::Utf8(Some(value)) => Ok(value),
            ConstantPoolEntry::Utf8(None) => Err(MetadataError::InvalidUtf8),
            ConstantPoolEntry::Unusable
            | ConstantPoolEntry::Class(_)
            | ConstantPoolEntry::Module(_)
            | ConstantPoolEntry::Other => Err(MetadataError::InvalidConstantPool(
                "constant pool entry is not Utf8",
            )),
        }
    }

    pub(super) fn class_name(&self, index: u16) -> Result<&str, MetadataError> {
        match self.entry(index)? {
            ConstantPoolEntry::Class(name_index) => self.utf8(*name_index),
            ConstantPoolEntry::Unusable
            | ConstantPoolEntry::Utf8(_)
            | ConstantPoolEntry::Module(_)
            | ConstantPoolEntry::Other => Err(MetadataError::InvalidConstantPool(
                "constant pool entry is not Class",
            )),
        }
    }

    pub(super) fn module_name(&self, index: u16) -> Result<&str, MetadataError> {
        match self.entry(index)? {
            ConstantPoolEntry::Module(name_index) => self.utf8(*name_index),
            ConstantPoolEntry::Unusable
            | ConstantPoolEntry::Utf8(_)
            | ConstantPoolEntry::Class(_)
            | ConstantPoolEntry::Other => Err(MetadataError::InvalidConstantPool(
                "constant pool entry is not Module",
            )),
        }
    }
}

pub(super) fn decode_modified_utf8(bytes: &[u8]) -> Result<Option<String>, MetadataError> {
    let mut utf16 = Vec::with_capacity(bytes.len());
    let mut offset = 0usize;
    while offset < bytes.len() {
        let first = bytes[offset];
        match first {
            0x01..=0x7f => {
                utf16.push(u16::from(first));
                offset += 1;
            }
            0xc0 => {
                if bytes.get(offset + 1) != Some(&0x80) {
                    return Err(MetadataError::InvalidUtf8);
                }
                utf16.push(0);
                offset += 2;
            }
            0xc2..=0xdf => {
                let second = *bytes.get(offset + 1).ok_or(MetadataError::InvalidUtf8)?;
                if second & 0xc0 != 0x80 {
                    return Err(MetadataError::InvalidUtf8);
                }
                utf16.push((u16::from(first & 0x1f) << 6) | u16::from(second & 0x3f));
                offset += 2;
            }
            0xe0..=0xef => {
                let second = *bytes.get(offset + 1).ok_or(MetadataError::InvalidUtf8)?;
                let third = *bytes.get(offset + 2).ok_or(MetadataError::InvalidUtf8)?;
                if second & 0xc0 != 0x80 || third & 0xc0 != 0x80 || (first == 0xe0 && second < 0xa0)
                {
                    return Err(MetadataError::InvalidUtf8);
                }
                utf16.push(
                    (u16::from(first & 0x0f) << 12)
                        | (u16::from(second & 0x3f) << 6)
                        | u16::from(third & 0x3f),
                );
                offset += 3;
            }
            0x00 | 0x80..=0xbf | 0xc1 | 0xf0..=0xff => {
                return Err(MetadataError::InvalidUtf8);
            }
        }
    }
    Ok(String::from_utf16(&utf16).ok())
}
