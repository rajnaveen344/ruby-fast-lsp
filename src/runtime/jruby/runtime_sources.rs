use super::classpath::{ArtifactOrigin, ClasspathArtifact};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use zip::ZipArchive;

const MAX_JRUBY_ARCHIVE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_RUNTIME_SOURCE_BYTES: u64 = 1024 * 1024;
const RUNTIME_SOURCE_ENTRIES: [&str; 3] = [
    "jruby/java/core_ext/kernel.rb",
    "jruby/java/core_ext/module.rb",
    "jruby/java/core_ext/object.rb",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JrubyRuntimeSource {
    pub path: PathBuf,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JrubyRuntimeSourceError {
    WrongArtifactOrigin,
    ArchiveTooLarge(u64),
    Read { path: PathBuf, message: String },
    FingerprintMismatch,
    InvalidArchive(String),
    MissingEntry(&'static str),
    EntryTooLarge { entry: &'static str, bytes: u64 },
    InvalidUtf8 { entry: &'static str },
    CacheWrite { path: PathBuf, message: String },
}

pub fn materialize_jruby_runtime_sources(
    artifact: &ClasspathArtifact,
    cache_root: &Path,
) -> Result<Vec<JrubyRuntimeSource>, JrubyRuntimeSourceError> {
    if artifact.origin != ArtifactOrigin::JrubyRuntime {
        return Err(JrubyRuntimeSourceError::WrongArtifactOrigin);
    }
    if artifact.byte_length > MAX_JRUBY_ARCHIVE_BYTES {
        return Err(JrubyRuntimeSourceError::ArchiveTooLarge(
            artifact.byte_length,
        ));
    }
    let bytes = fs::read(&artifact.path).map_err(|error| JrubyRuntimeSourceError::Read {
        path: artifact.path.clone(),
        message: error.to_string(),
    })?;
    if format!("{:x}", Sha256::digest(&bytes)) != artifact.fingerprint_sha256 {
        return Err(JrubyRuntimeSourceError::FingerprintMismatch);
    }
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| JrubyRuntimeSourceError::InvalidArchive(error.to_string()))?;
    let mut sources = Vec::with_capacity(RUNTIME_SOURCE_ENTRIES.len());
    for entry_name in RUNTIME_SOURCE_ENTRIES {
        let mut entry = archive
            .by_name(entry_name)
            .map_err(|_| JrubyRuntimeSourceError::MissingEntry(entry_name))?;
        if entry.size() > MAX_RUNTIME_SOURCE_BYTES {
            return Err(JrubyRuntimeSourceError::EntryTooLarge {
                entry: entry_name,
                bytes: entry.size(),
            });
        }
        let mut content = String::with_capacity(entry.size() as usize);
        entry
            .read_to_string(&mut content)
            .map_err(|_| JrubyRuntimeSourceError::InvalidUtf8 { entry: entry_name })?;
        let path = cache_root.join(entry_name);
        let parent = path.parent().expect(
            "INVARIANT VIOLATED: allowlisted JRuby runtime source has no cache parent. \
             This is a bug because every entry is a relative multi-component path. \
             Fix: keep the runtime source allowlist path-safe and relative.",
        );
        fs::create_dir_all(parent).map_err(|error| JrubyRuntimeSourceError::CacheWrite {
            path: parent.to_path_buf(),
            message: error.to_string(),
        })?;
        if !fs::read_to_string(&path).is_ok_and(|existing| existing == content) {
            fs::write(&path, content.as_bytes()).map_err(|error| {
                JrubyRuntimeSourceError::CacheWrite {
                    path: path.clone(),
                    message: error.to_string(),
                }
            })?;
        }
        sources.push(JrubyRuntimeSource { path, content });
    }
    Ok(sources)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::jruby::classpath::ArtifactKind;
    use std::io::Write;
    use tempfile::tempdir;
    use zip::write::SimpleFileOptions;

    #[test]
    fn materializes_only_bounded_allowlisted_runtime_sources() {
        let temp = tempdir().unwrap();
        let archive_path = temp.path().join("jruby.jar");
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for entry in RUNTIME_SOURCE_ENTRIES {
            writer
                .start_file(entry, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(format!("# {entry}\n").as_bytes()).unwrap();
        }
        writer
            .start_file("../escape.rb", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(b"raise 'must not extract'\n").unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        fs::write(&archive_path, &bytes).unwrap();
        let artifact = ClasspathArtifact {
            path: archive_path,
            origin: ArtifactOrigin::JrubyRuntime,
            kind: ArtifactKind::Jar,
            fingerprint_sha256: format!("{:x}", Sha256::digest(&bytes)),
            byte_length: bytes.len() as u64,
        };

        let cache = temp.path().join("cache");
        let sources = materialize_jruby_runtime_sources(&artifact, &cache).unwrap();

        assert_eq!(sources.len(), RUNTIME_SOURCE_ENTRIES.len());
        assert!(sources.iter().any(|source| {
            source.path.ends_with("jruby/java/core_ext/object.rb")
                && source.content.contains("core_ext/object.rb")
        }));
        assert!(!temp.path().join("escape.rb").exists());
    }

    #[test]
    fn rejects_runtime_archive_content_drift() {
        let temp = tempdir().unwrap();
        let archive_path = temp.path().join("jruby.jar");
        fs::write(&archive_path, b"not the recorded archive").unwrap();
        let artifact = ClasspathArtifact {
            path: archive_path,
            origin: ArtifactOrigin::JrubyRuntime,
            kind: ArtifactKind::Jar,
            fingerprint_sha256: "wrong".to_string(),
            byte_length: 24,
        };

        assert_eq!(
            materialize_jruby_runtime_sources(&artifact, &temp.path().join("cache")),
            Err(JrubyRuntimeSourceError::FingerprintMismatch)
        );
    }
}
