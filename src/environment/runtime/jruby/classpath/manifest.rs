//! Bounded JAR manifest `Class-Path` extraction and entry validation.

use std::io::{Cursor, Read};
use std::path::{Component, Path, PathBuf};

use super::ClasspathError;

pub(super) fn manifest_class_path_entries(
    path: &Path,
    bytes: &[u8],
    max_entries: usize,
) -> Result<Vec<String>, ClasspathError> {
    let Ok(mut archive) = zip::ZipArchive::new(Cursor::new(bytes)) else {
        // Catalog parsing remains the authority for rejecting malformed JARs.
        // Classpath discovery only expands a manifest when a bounded archive is
        // readable, preserving existing content-identity error ownership.
        return Ok(Vec::new());
    };
    if archive.len() > max_entries {
        return Err(ClasspathError::LimitExceeded("manifest archive entries"));
    }
    let mut manifest_index = None;
    for index in 0..archive.len() {
        let entry = archive
            .by_index(index)
            .map_err(|error| ClasspathError::Io {
                path: path.to_path_buf(),
                message: error.to_string(),
            })?;
        if entry.name().eq_ignore_ascii_case("META-INF/MANIFEST.MF") {
            if manifest_index.replace(index).is_some() {
                return Err(ClasspathError::InvalidManifestEntry {
                    artifact: path.to_path_buf(),
                    entry: "duplicate META-INF/MANIFEST.MF".to_string(),
                });
            }
        }
    }
    let Some(index) = manifest_index else {
        return Ok(Vec::new());
    };
    let mut manifest = archive
        .by_index(index)
        .map_err(|error| ClasspathError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
    const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
    if manifest.size() > MAX_MANIFEST_BYTES {
        return Err(ClasspathError::LimitExceeded("JAR manifest bytes"));
    }
    let mut bytes = Vec::with_capacity(manifest.size() as usize);
    manifest
        .read_to_end(&mut bytes)
        .map_err(|error| ClasspathError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
    let source = std::str::from_utf8(&bytes).map_err(|_| ClasspathError::InvalidManifestEntry {
        artifact: path.to_path_buf(),
        entry: "manifest is not UTF-8".to_string(),
    })?;
    let mut logical_lines = Vec::<String>::new();
    for physical in source.lines() {
        let physical = physical.strip_suffix('\r').unwrap_or(physical);
        if physical.is_empty() {
            break;
        }
        if let Some(continuation) = physical.strip_prefix(' ') {
            let Some(previous) = logical_lines.last_mut() else {
                return Err(ClasspathError::InvalidManifestEntry {
                    artifact: path.to_path_buf(),
                    entry: "orphan manifest continuation".to_string(),
                });
            };
            previous.push_str(continuation);
        } else {
            logical_lines.push(physical.to_string());
        }
    }
    let mut class_path = None;
    for line in logical_lines {
        let Some((name, value)) = line.split_once(':') else {
            return Err(ClasspathError::InvalidManifestEntry {
                artifact: path.to_path_buf(),
                entry: line,
            });
        };
        if !name.eq_ignore_ascii_case("Class-Path") {
            continue;
        }
        if class_path.replace(value.trim().to_string()).is_some() {
            return Err(ClasspathError::InvalidManifestEntry {
                artifact: path.to_path_buf(),
                entry: "duplicate Class-Path attribute".to_string(),
            });
        }
    }
    Ok(class_path
        .map(|value| value.split_whitespace().map(str::to_string).collect())
        .unwrap_or_default())
}

pub(super) fn validate_manifest_class_path_entry(
    artifact: &Path,
    entry: &str,
) -> Result<PathBuf, ClasspathError> {
    let path = Path::new(entry);
    let valid = !entry.is_empty()
        && !entry.contains('\\')
        && !entry.contains(':')
        && !entry.contains('%')
        && !path.is_absolute()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        && path.extension().and_then(|extension| extension.to_str()) == Some("jar");
    if !valid {
        return Err(ClasspathError::InvalidManifestEntry {
            artifact: artifact.to_path_buf(),
            entry: entry.to_string(),
        });
    }
    Ok(path.to_path_buf())
}
