use std::collections::{BTreeMap, HashSet};
use std::io::{Cursor, Read};

use sha2::{Digest, Sha256};

use crate::{parse_class, ClassFile, ClassLimits, MetadataError};

const JMOD_MAGIC: &[u8; 4] = b"JM\x01\x00";
const VERSIONED_PREFIX: &str = "META-INF/versions/";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    Jar,
    Jmod,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveClass {
    pub entry_name: String,
    pub release: Option<u16>,
    pub class: ClassFile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchiveMetadata {
    pub fingerprint_sha256: String,
    pub classes: Vec<ArchiveClass>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArchiveLimits {
    pub max_archive_bytes: usize,
    pub max_entries: usize,
    pub max_entry_bytes: usize,
    pub max_total_decompressed_bytes: usize,
    pub max_class_count: usize,
    pub class: ClassLimits,
}

impl Default for ArchiveLimits {
    fn default() -> Self {
        Self {
            max_archive_bytes: 256 * 1024 * 1024,
            max_entries: 100_000,
            max_entry_bytes: 32 * 1024 * 1024,
            max_total_decompressed_bytes: 512 * 1024 * 1024,
            max_class_count: 100_000,
            class: ClassLimits::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArchiveError {
    InvalidJmodMagic,
    InvalidArchive(String),
    LimitExceeded(&'static str),
    DuplicateEntry(String),
    TraversingEntry(String),
    InvalidMultiReleaseEntry(String),
    ClassPathMismatch { entry: String, declared: String },
    Class { entry: String, error: MetadataError },
}

pub fn parse_archive(
    bytes: &[u8],
    kind: ArchiveKind,
    jdk_feature: u16,
    limits: ArchiveLimits,
) -> Result<ArchiveMetadata, ArchiveError> {
    if bytes.len() > limits.max_archive_bytes {
        return Err(ArchiveError::LimitExceeded("archive bytes"));
    }
    let zip_bytes = match kind {
        ArchiveKind::Jar => bytes,
        ArchiveKind::Jmod => bytes
            .strip_prefix(JMOD_MAGIC)
            .ok_or(ArchiveError::InvalidJmodMagic)?,
    };
    let declared_entry_count = validate_central_directory_names(zip_bytes, limits.max_entries)?;
    let mut archive = zip::ZipArchive::new(Cursor::new(zip_bytes))
        .map_err(|error| ArchiveError::InvalidArchive(error.to_string()))?;
    if archive.len() != declared_entry_count {
        return Err(ArchiveError::InvalidArchive(
            "ZIP reader entry count differs from validated central directory".to_string(),
        ));
    }

    let mut names = HashSet::with_capacity(archive.len());
    let mut entries = Vec::with_capacity(archive.len());
    let mut total_decompressed_bytes = 0usize;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| ArchiveError::InvalidArchive(error.to_string()))?;
        let name = entry.name().to_string();
        validate_entry_name(&name)?;
        if !names.insert(name.clone()) {
            return Err(ArchiveError::DuplicateEntry(name));
        }
        if !is_metadata_entry(&name, kind) {
            continue;
        }
        let declared_size = usize::try_from(entry.size())
            .map_err(|_| ArchiveError::LimitExceeded("archive entry bytes"))?;
        if declared_size > limits.max_entry_bytes {
            return Err(ArchiveError::LimitExceeded("archive entry bytes"));
        }
        total_decompressed_bytes = total_decompressed_bytes.checked_add(declared_size).ok_or(
            ArchiveError::LimitExceeded("total decompressed archive bytes"),
        )?;
        if total_decompressed_bytes > limits.max_total_decompressed_bytes {
            return Err(ArchiveError::LimitExceeded(
                "total decompressed archive bytes",
            ));
        }
        let mut contents = Vec::with_capacity(declared_size);
        entry
            .take(
                u64::try_from(limits.max_entry_bytes)
                    .expect("INVARIANT VIOLATED: archive entry bound must fit u64"),
            )
            .read_to_end(&mut contents)
            .map_err(|error| ArchiveError::InvalidArchive(error.to_string()))?;
        if contents.len() != declared_size {
            return Err(ArchiveError::InvalidArchive(format!(
                "entry `{name}` size changed while reading"
            )));
        }
        entries.push((name, contents));
    }
    entries.sort_by(|left, right| left.0.cmp(&right.0));

    let is_multi_release = kind == ArchiveKind::Jar
        && entries
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("META-INF/MANIFEST.MF"))
            .is_some_and(|(_, manifest)| manifest_is_multi_release(manifest));

    let mut selected: BTreeMap<String, (Option<u16>, String, Vec<u8>)> = BTreeMap::new();
    for (entry_name, contents) in entries {
        let Some((logical_name, release)) =
            class_candidate(&entry_name, kind, is_multi_release, jdk_feature)?
        else {
            continue;
        };
        let should_replace = selected
            .get(&logical_name)
            .is_none_or(|(current, _, _)| release.unwrap_or(0) > current.unwrap_or(0));
        if should_replace {
            selected.insert(logical_name, (release, entry_name, contents));
        }
    }
    if selected.len() > limits.max_class_count {
        return Err(ArchiveError::LimitExceeded("archive classes"));
    }

    let mut classes = Vec::with_capacity(selected.len());
    for (logical_name, (release, entry_name, contents)) in selected {
        let class = parse_class(&contents, limits.class).map_err(|error| ArchiveError::Class {
            entry: entry_name.clone(),
            error,
        })?;
        let expected = logical_name
            .strip_suffix(".class")
            .expect("INVARIANT VIOLATED: selected JVM entry must end in .class");
        if class.name != expected {
            return Err(ArchiveError::ClassPathMismatch {
                entry: entry_name,
                declared: class.name,
            });
        }
        classes.push(ArchiveClass {
            entry_name,
            release,
            class,
        });
    }

    Ok(ArchiveMetadata {
        fingerprint_sha256: format!("{:x}", Sha256::digest(bytes)),
        classes,
    })
}

fn is_metadata_entry(name: &str, kind: ArchiveKind) -> bool {
    match kind {
        ArchiveKind::Jar => {
            name.ends_with(".class") || name.eq_ignore_ascii_case("META-INF/MANIFEST.MF")
        }
        ArchiveKind::Jmod => name.starts_with("classes/") && name.ends_with(".class"),
    }
}

fn validate_entry_name(name: &str) -> Result<(), ArchiveError> {
    if name.is_empty()
        || name.starts_with('/')
        || name.starts_with('\\')
        || name.contains('\\')
        || name.split('/').any(|component| component == "..")
    {
        return Err(ArchiveError::TraversingEntry(name.to_string()));
    }
    Ok(())
}

fn validate_central_directory_names(
    bytes: &[u8],
    max_entries: usize,
) -> Result<usize, ArchiveError> {
    const EOCD_SIGNATURE: &[u8; 4] = b"PK\x05\x06";
    const CENTRAL_SIGNATURE: &[u8; 4] = b"PK\x01\x02";
    const EOCD_MIN_BYTES: usize = 22;
    const MAX_COMMENT_BYTES: usize = u16::MAX as usize;

    let search_start = bytes
        .len()
        .saturating_sub(EOCD_MIN_BYTES + MAX_COMMENT_BYTES);
    let eocd_offset = (search_start..=bytes.len().saturating_sub(EOCD_MIN_BYTES))
        .rev()
        .find(|offset| {
            bytes.get(*offset..*offset + 4) == Some(EOCD_SIGNATURE)
                && bytes
                    .get(*offset + 20..*offset + 22)
                    .map(|value| usize::from(u16::from_le_bytes([value[0], value[1]])))
                    .is_some_and(|comment_length| {
                        *offset + EOCD_MIN_BYTES + comment_length == bytes.len()
                    })
        })
        .ok_or_else(|| ArchiveError::InvalidArchive("missing ZIP end record".to_string()))?;
    let disk_number = read_u16_le(bytes, eocd_offset + 4)?;
    let central_disk = read_u16_le(bytes, eocd_offset + 6)?;
    let entries_on_disk = read_u16_le(bytes, eocd_offset + 8)?;
    let total_entries = read_u16_le(bytes, eocd_offset + 10)?;
    if disk_number != 0 || central_disk != 0 || entries_on_disk != total_entries {
        return Err(ArchiveError::InvalidArchive(
            "split ZIP archives are unsupported".to_string(),
        ));
    }
    if total_entries == u16::MAX {
        return Err(ArchiveError::InvalidArchive(
            "ZIP64 archives are unsupported by the bounded metadata reader".to_string(),
        ));
    }
    let total_entries = usize::from(total_entries);
    if total_entries > max_entries {
        return Err(ArchiveError::LimitExceeded("archive entries"));
    }
    let central_offset = usize::try_from(read_u32_le(bytes, eocd_offset + 16)?)
        .map_err(|_| ArchiveError::LimitExceeded("central directory offset"))?;
    let mut offset = central_offset;
    let mut names = HashSet::with_capacity(total_entries);
    for _ in 0..total_entries {
        if bytes.get(offset..offset + 4) != Some(CENTRAL_SIGNATURE) {
            return Err(ArchiveError::InvalidArchive(
                "invalid central directory entry signature".to_string(),
            ));
        }
        let name_length = usize::from(read_u16_le(bytes, offset + 28)?);
        let extra_length = usize::from(read_u16_le(bytes, offset + 30)?);
        let comment_length = usize::from(read_u16_le(bytes, offset + 32)?);
        let name_start = offset
            .checked_add(46)
            .ok_or(ArchiveError::LimitExceeded("central directory bytes"))?;
        let name_end = name_start
            .checked_add(name_length)
            .ok_or(ArchiveError::LimitExceeded("central directory bytes"))?;
        let name = bytes
            .get(name_start..name_end)
            .ok_or_else(|| ArchiveError::InvalidArchive("truncated entry name".to_string()))?;
        if !names.insert(name.to_vec()) {
            return Err(ArchiveError::DuplicateEntry(
                String::from_utf8_lossy(name).into_owned(),
            ));
        }
        offset = name_end
            .checked_add(extra_length)
            .and_then(|value| value.checked_add(comment_length))
            .ok_or(ArchiveError::LimitExceeded("central directory bytes"))?;
        if offset > eocd_offset {
            return Err(ArchiveError::InvalidArchive(
                "central directory entry exceeds end record".to_string(),
            ));
        }
    }
    if offset != eocd_offset {
        return Err(ArchiveError::InvalidArchive(
            "central directory size does not match declared entries".to_string(),
        ));
    }
    Ok(total_entries)
}

fn read_u16_le(bytes: &[u8], offset: usize) -> Result<u16, ArchiveError> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| ArchiveError::InvalidArchive("truncated ZIP structure".to_string()))?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn read_u32_le(bytes: &[u8], offset: usize) -> Result<u32, ArchiveError> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| ArchiveError::InvalidArchive("truncated ZIP structure".to_string()))?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn manifest_is_multi_release(bytes: &[u8]) -> bool {
    let Ok(contents) = std::str::from_utf8(bytes) else {
        return false;
    };
    contents.lines().any(|line| {
        line.split_once(':').is_some_and(|(name, value)| {
            name.eq_ignore_ascii_case("Multi-Release") && value.trim().eq_ignore_ascii_case("true")
        })
    })
}

fn class_candidate(
    entry_name: &str,
    kind: ArchiveKind,
    is_multi_release: bool,
    jdk_feature: u16,
) -> Result<Option<(String, Option<u16>)>, ArchiveError> {
    match kind {
        ArchiveKind::Jmod => {
            let Some(name) = entry_name.strip_prefix("classes/") else {
                return Ok(None);
            };
            Ok(name.ends_with(".class").then(|| (name.to_string(), None)))
        }
        ArchiveKind::Jar => {
            if let Some(versioned) = entry_name.strip_prefix(VERSIONED_PREFIX) {
                let Some((release, logical_name)) = versioned.split_once('/') else {
                    return Err(ArchiveError::InvalidMultiReleaseEntry(
                        entry_name.to_string(),
                    ));
                };
                let release = release
                    .parse::<u16>()
                    .map_err(|_| ArchiveError::InvalidMultiReleaseEntry(entry_name.to_string()))?;
                if release < 9 || logical_name.is_empty() {
                    return Err(ArchiveError::InvalidMultiReleaseEntry(
                        entry_name.to_string(),
                    ));
                }
                if !is_multi_release || release > jdk_feature || !logical_name.ends_with(".class") {
                    return Ok(None);
                }
                return Ok(Some((logical_name.to_string(), Some(release))));
            }
            if entry_name.starts_with("META-INF/") || !entry_name.ends_with(".class") {
                return Ok(None);
            }
            Ok(Some((entry_name.to_string(), None)))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Write};

    use zip::write::SimpleFileOptions;

    use super::*;

    fn decode_hex(source: &str) -> Vec<u8> {
        let digits = source
            .bytes()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect::<Vec<_>>();
        digits
            .chunks_exact(2)
            .map(|pair| {
                u8::from_str_radix(
                    std::str::from_utf8(pair).expect("fixture hex must be ASCII"),
                    16,
                )
                .expect("fixture byte must be valid hex")
            })
            .collect()
    }

    fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .last_modified_time(zip::DateTime::default());
        for (name, contents) in entries {
            writer
                .start_file(*name, options)
                .expect("checked archive fixture entry must start");
            writer
                .write_all(contents)
                .expect("checked archive fixture entry must write");
        }
        writer
            .finish()
            .expect("checked archive fixture must finish")
            .into_inner()
    }

    fn minimal_class() -> Vec<u8> {
        decode_hex(include_str!("../fixtures/minimal_class.hex"))
    }

    #[test]
    fn selects_highest_eligible_multi_release_class_deterministically() {
        let base = minimal_class();
        let mut java_11 = base.clone();
        java_11[6..8].copy_from_slice(&55u16.to_be_bytes());
        let mut java_17 = base.clone();
        java_17[6..8].copy_from_slice(&61u16.to_be_bytes());
        let manifest = b"Manifest-Version: 1.0\r\nMulti-Release: true\r\n\r\n";
        let archive = zip_bytes(&[
            ("META-INF/versions/17/com/example/Demo.class", &java_17),
            ("com/example/Demo.class", &base),
            ("META-INF/MANIFEST.MF", manifest),
            ("META-INF/versions/11/com/example/Demo.class", &java_11),
        ]);

        let java_10_metadata =
            parse_archive(&archive, ArchiveKind::Jar, 10, ArchiveLimits::default())
                .expect("base class must be selected below the first eligible release");
        assert_eq!(java_10_metadata.classes[0].release, None);
        assert_eq!(java_10_metadata.classes[0].class.major_version, 61);

        let java_11_metadata =
            parse_archive(&archive, ArchiveKind::Jar, 11, ArchiveLimits::default())
                .expect("Java 11 class must be selected");
        assert_eq!(java_11_metadata.classes[0].release, Some(11));
        assert_eq!(java_11_metadata.classes[0].class.major_version, 55);

        let java_21_metadata =
            parse_archive(&archive, ArchiveKind::Jar, 21, ArchiveLimits::default())
                .expect("highest eligible class must be selected");
        assert_eq!(java_21_metadata.classes[0].release, Some(17));
        assert_eq!(java_21_metadata.classes[0].class.major_version, 61);
        assert_eq!(java_21_metadata.fingerprint_sha256.len(), 64);
    }

    #[test]
    fn reads_only_the_classes_tree_from_a_jmod() {
        let class = minimal_class();
        let zip = zip_bytes(&[
            ("classes/com/example/Demo.class", &class),
            ("bin/demo", b"not a class"),
        ]);
        let mut jmod = JMOD_MAGIC.to_vec();
        jmod.extend(zip);

        let metadata = parse_archive(&jmod, ArchiveKind::Jmod, 17, ArchiveLimits::default())
            .expect("checked JMOD fixture must parse");
        assert_eq!(metadata.classes.len(), 1);
        assert_eq!(metadata.classes[0].class.name, "com/example/Demo");
    }

    #[test]
    fn rejects_traversal_duplicates_path_mismatch_and_invalid_jmod_magic() {
        let class = minimal_class();
        let traversal = zip_bytes(&[("../Demo.class", &class)]);
        assert_eq!(
            parse_archive(&traversal, ArchiveKind::Jar, 17, ArchiveLimits::default()),
            Err(ArchiveError::TraversingEntry("../Demo.class".to_string()))
        );

        let mut duplicates = zip_bytes(&[
            ("com/example/Demo.class", &class),
            ("com/example/Xemo.class", &class),
        ]);
        let old_name = b"com/example/Xemo.class";
        for offset in 0..=duplicates.len() - old_name.len() {
            if &duplicates[offset..offset + old_name.len()] == old_name {
                duplicates[offset..offset + old_name.len()]
                    .copy_from_slice(b"com/example/Demo.class");
            }
        }
        assert_eq!(
            parse_archive(&duplicates, ArchiveKind::Jar, 17, ArchiveLimits::default()),
            Err(ArchiveError::DuplicateEntry(
                "com/example/Demo.class".to_string()
            ))
        );

        let mismatch = zip_bytes(&[("wrong/Name.class", &class)]);
        assert!(matches!(
            parse_archive(&mismatch, ArchiveKind::Jar, 17, ArchiveLimits::default()),
            Err(ArchiveError::ClassPathMismatch { .. })
        ));
        assert_eq!(
            parse_archive(
                &zip_bytes(&[("classes/com/example/Demo.class", &class)]),
                ArchiveKind::Jmod,
                17,
                ArchiveLimits::default()
            ),
            Err(ArchiveError::InvalidJmodMagic)
        );
    }

    #[test]
    fn enforces_archive_entry_decompression_and_class_bounds() {
        let class = minimal_class();
        let large_irrelevant_native_payload = vec![0u8; class.len() * 4];
        let archive = zip_bytes(&[
            ("com/example/Demo.class", &class),
            (
                "native/linux/application-binary",
                &large_irrelevant_native_payload,
            ),
        ]);

        let mut entry_limits = ArchiveLimits::default();
        entry_limits.max_entries = 1;
        assert_eq!(
            parse_archive(&archive, ArchiveKind::Jar, 17, entry_limits),
            Err(ArchiveError::LimitExceeded("archive entries"))
        );

        let mut irrelevant_entry_limits = ArchiveLimits::default();
        irrelevant_entry_limits.max_entry_bytes = class.len();
        irrelevant_entry_limits.max_total_decompressed_bytes = class.len();
        let metadata = parse_archive(&archive, ArchiveKind::Jar, 17, irrelevant_entry_limits)
            .expect("non-class payloads must not be decompressed into the metadata budget");
        assert_eq!(metadata.classes.len(), 1);

        let mut decompression_limits = ArchiveLimits::default();
        decompression_limits.max_total_decompressed_bytes = class.len() - 1;
        assert_eq!(
            parse_archive(&archive, ArchiveKind::Jar, 17, decompression_limits),
            Err(ArchiveError::LimitExceeded(
                "total decompressed archive bytes"
            ))
        );

        let mut class_limits = ArchiveLimits::default();
        class_limits.max_class_count = 0;
        assert_eq!(
            parse_archive(&archive, ArchiveKind::Jar, 17, class_limits),
            Err(ArchiveError::LimitExceeded("archive classes"))
        );
    }

    #[test]
    fn rejects_corrupt_archive_bytes_without_panicking() {
        assert!(matches!(
            parse_archive(
                b"this is not a ZIP archive",
                ArchiveKind::Jar,
                17,
                ArchiveLimits::default()
            ),
            Err(ArchiveError::InvalidArchive(_))
        ));
    }
}
