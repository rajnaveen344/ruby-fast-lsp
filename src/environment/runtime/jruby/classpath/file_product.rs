//! Process-shared, identity-checked fingerprints and manifest descriptors for classpath files.

use crate::invariant::ExpectInvariant;
use crate::utils::single_flight::{BlockingBoundedSingleFlightCache, SingleFlightSnapshot};
use sha2::{Digest, Sha256};
use std::fs::{self, File, Metadata};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::manifest::manifest_class_path_entries;
use super::{ClasspathError, SourceFileIdentity};

const DEFAULT_CLASSPATH_FILE_CACHE_ENTRIES: usize = 4_096;
const DEFAULT_CLASSPATH_FILE_CACHE_WEIGHT_BYTES: u64 = 16 * 1024 * 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum ClasspathFileProductKind {
    Fingerprint,
    JarManifest { max_archive_entries: usize },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ClasspathFileProductKey {
    canonical_path: PathBuf,
    identity: SourceFileIdentity,
    kind: ClasspathFileProductKind,
}

#[derive(Debug)]
pub(super) struct ClasspathFileProduct {
    pub(super) fingerprint_sha256: String,
    pub(super) manifest_class_path_entries: Vec<String>,
}

impl ClasspathFileProduct {
    fn estimated_weight_bytes(&self) -> u64 {
        // Include a fixed allowance for the cache key, hash-map entry, Arc,
        // String/Vec headers, and typical canonical path. Entry count is an
        // independent hard bound for paths longer than this allowance.
        self.manifest_class_path_entries
            .iter()
            .try_fold(512u64, |total, entry| {
                total.checked_add(u64::try_from(entry.len()).expect_invariant(
                    "a manifest entry length does not fit u64",
                    "manifest input is bounded to one MiB",
                    "retain bounded manifest parsing before caching its logical entries",
                ))
            })
            .and_then(|total| {
                total.checked_add(
                    u64::try_from(self.fingerprint_sha256.len()).expect_invariant(
                        "a SHA-256 string length does not fit u64",
                        "its encoded length is fixed at 64 bytes",
                        "keep fingerprints as bounded SHA-256 hex strings",
                    ),
                )
            })
            .expect_invariant(
                "retained classpath descriptor weight overflowed u64",
                "manifest payloads and cache entry counts are bounded",
                "inspect descriptor weight accounting",
            )
    }
}

/// Process-owned reuse for immutable classpath file descriptors. This never
/// retains raw JAR/JMOD/source bytes and never owns project classpath order or
/// semantic facts; each isolated project composes the returned descriptor into
/// its own `ProjectClasspath`.
#[derive(Clone)]
pub struct ClasspathFileProductCache {
    inner: BlockingBoundedSingleFlightCache<
        ClasspathFileProductKey,
        ClasspathFileProduct,
        ClasspathError,
    >,
}

impl Default for ClasspathFileProductCache {
    fn default() -> Self {
        Self::new(
            DEFAULT_CLASSPATH_FILE_CACHE_ENTRIES,
            DEFAULT_CLASSPATH_FILE_CACHE_WEIGHT_BYTES,
        )
    }
}

impl ClasspathFileProductCache {
    pub fn new(max_entries: usize, max_weight_bytes: u64) -> Self {
        Self {
            inner: BlockingBoundedSingleFlightCache::new(
                max_entries,
                max_weight_bytes,
                ClasspathFileProduct::estimated_weight_bytes,
            ),
        }
    }

    pub fn snapshot(&self) -> SingleFlightSnapshot {
        self.inner.snapshot()
    }

    pub fn retained_weight_bytes(&self) -> u64 {
        self.inner.retained_weight()
    }

    pub(super) fn get_or_read(
        &self,
        path: &Path,
        identity: SourceFileIdentity,
        kind: ClasspathFileProductKind,
        max_file_bytes: u64,
    ) -> Result<Arc<ClasspathFileProduct>, ClasspathError> {
        let key = ClasspathFileProductKey {
            canonical_path: path.to_path_buf(),
            identity,
            kind,
        };
        self.inner.get_or_try_init(key, || {
            read_classpath_file_product(path, identity, kind, max_file_bytes)
        })
    }
}

pub(super) fn classpath_file_identity(path: &Path) -> Result<SourceFileIdentity, ClasspathError> {
    let metadata = fs::metadata(path).map_err(|error| ClasspathError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    classpath_file_identity_from_metadata(path, &metadata)
}

fn classpath_file_identity_from_metadata(
    path: &Path,
    metadata: &Metadata,
) -> Result<SourceFileIdentity, ClasspathError> {
    if !metadata.is_file() {
        return Err(ClasspathError::Io {
            path: path.to_path_buf(),
            message: "classpath product input is not a regular file".to_string(),
        });
    }
    let modified = metadata.modified().map_err(|error| ClasspathError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    Ok(SourceFileIdentity {
        byte_length: metadata.len(),
        modified,
    })
}

pub(super) fn read_classpath_file_product(
    path: &Path,
    expected_identity: SourceFileIdentity,
    kind: ClasspathFileProductKind,
    max_file_bytes: u64,
) -> Result<ClasspathFileProduct, ClasspathError> {
    if expected_identity.byte_length > max_file_bytes {
        return Err(ClasspathError::LimitExceeded("classpath artifact bytes"));
    }
    let mut file = File::open(path).map_err(|error| ClasspathError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    let opened_identity = file
        .metadata()
        .map_err(|error| ClasspathError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        })
        .and_then(|metadata| classpath_file_identity_from_metadata(path, &metadata))?;
    if opened_identity != expected_identity {
        return Err(classpath_file_changed(path));
    }
    let read_limit = max_file_bytes
        .checked_add(1)
        .ok_or(ClasspathError::LimitExceeded("classpath artifact bytes"))?;
    let capacity = usize::try_from(expected_identity.byte_length)
        .map_err(|_| ClasspathError::LimitExceeded("classpath artifact bytes"))?;
    let mut bytes = Vec::with_capacity(capacity);
    (&mut file)
        .take(read_limit)
        .read_to_end(&mut bytes)
        .map_err(|error| ClasspathError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
    if u64::try_from(bytes.len()).expect_invariant(
        "an in-memory classpath buffer length does not fit u64",
        "the read is bounded far below u64::MAX",
        "keep classpath read bounds below the addressable process size",
    ) > max_file_bytes
    {
        return Err(ClasspathError::LimitExceeded("classpath artifact bytes"));
    }
    let handle_after_identity = file
        .metadata()
        .map_err(|error| ClasspathError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        })
        .and_then(|metadata| classpath_file_identity_from_metadata(path, &metadata))?;
    let path_after_identity = classpath_file_identity(path)?;
    if u64::try_from(bytes.len()).expect_invariant(
        "an in-memory classpath buffer length does not fit u64",
        "the read is bounded far below u64::MAX",
        "keep classpath read bounds below the addressable process size",
    ) != expected_identity.byte_length
        || handle_after_identity != expected_identity
        || path_after_identity != expected_identity
    {
        return Err(classpath_file_changed(path));
    }

    let manifest_class_path_entries = match kind {
        ClasspathFileProductKind::Fingerprint => Vec::new(),
        ClasspathFileProductKind::JarManifest {
            max_archive_entries,
        } => manifest_class_path_entries(path, &bytes, max_archive_entries)?,
    };
    Ok(ClasspathFileProduct {
        fingerprint_sha256: format!("{:x}", Sha256::digest(&bytes)),
        manifest_class_path_entries,
    })
}

fn classpath_file_changed(path: &Path) -> ClasspathError {
    ClasspathError::Io {
        path: path.to_path_buf(),
        message: "classpath file changed while its checksum identity was being established"
            .to_string(),
    }
}
