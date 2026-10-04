//! Immutable per-artifact Java metadata products, their persistent envelope,
//! and the bounded process cache that shares them between projects.

use super::super::classpath::{ArtifactKind, ClasspathArtifact};
use super::catalog::JavaCatalogContent;
use super::{artifact_kind, JavaCatalogError};
use crate::invariant::ExpectInvariant;
use crate::utils::persistent_cache::{PersistentProduct, PersistentProductKind};
use crate::utils::single_flight::{BlockingBoundedSingleFlightCache, SingleFlightStat};
use anyhow::{anyhow, Context, Result as AnyResult};
use parking_lot::Mutex;
use ruby_analysis::stats::StatsSnapshot;
use ruby_fast_lsp_jvm_metadata::{
    parse_archive, ArchiveLimits, ArchiveMetadata, ARCHIVE_PRODUCT_SEMANTIC_VERSION,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::borrow::Cow;
use std::collections::HashMap;
use std::fs;
use std::mem::size_of;
use std::sync::{Arc, Weak};

const JAVA_ARTIFACT_PRODUCT_SCHEMA: u32 = 2;
const DEFAULT_JAVA_ARTIFACT_CACHE_ENTRIES: usize = 256;
const DEFAULT_JAVA_ARTIFACT_CACHE_WEIGHT_BYTES: u64 = 256 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JavaArtifactProductKey {
    cache_id: String,
    pub(super) artifact_fingerprint_sha256: String,
    pub(super) artifact_kind: ArtifactKind,
    jdk_feature: u16,
    limits_fingerprint_sha256: String,
}

#[derive(Debug, Clone)]
pub struct JavaArtifactProduct {
    pub(super) key: JavaArtifactProductKey,
    pub(super) archive: Arc<ArchiveMetadata>,
    estimated_weight_bytes: u64,
}

/// SHA-256 over the ordered `(artifact path, product identity)` list a
/// catalog is composed from. Equal identities compose equal catalog content.
pub(super) type CatalogContentIdentity = [u8; 32];

/// Bounded process-owned reuse for exact immutable Java artifact metadata.
/// Project classpath order and paths stay consumer-owned; parsed archives and
/// catalog content composed from the same ordered artifacts are shared.
#[derive(Clone)]
pub struct JavaArtifactProductCache {
    inner: BlockingBoundedSingleFlightCache<JavaArtifactProductKey, JavaArtifactProduct, String>,
    /// Live catalog content by composition identity. Entries never retain
    /// content; the projects that hold it do.
    contents: Arc<Mutex<HashMap<CatalogContentIdentity, Weak<JavaCatalogContent>>>>,
    /// Live archives by product cache id. A catalog that outlives its
    /// product's cache entry still shares the archive with later requests
    /// instead of decoding a second copy.
    archives: Arc<Mutex<HashMap<String, Weak<ArchiveMetadata>>>>,
}

impl Default for JavaArtifactProductCache {
    fn default() -> Self {
        Self::new(
            DEFAULT_JAVA_ARTIFACT_CACHE_ENTRIES,
            DEFAULT_JAVA_ARTIFACT_CACHE_WEIGHT_BYTES,
        )
    }
}

impl JavaArtifactProductCache {
    pub fn new(max_entries: usize, max_weight_bytes: u64) -> Self {
        Self {
            inner: BlockingBoundedSingleFlightCache::new(
                max_entries,
                max_weight_bytes,
                JavaArtifactProduct::estimated_weight_bytes,
            ),
            contents: Arc::default(),
            archives: Arc::default(),
        }
    }

    pub fn get_or_try_init(
        &self,
        key: JavaArtifactProductKey,
        producer: impl FnOnce() -> Result<JavaArtifactProduct, String>,
    ) -> Result<Arc<JavaArtifactProduct>, String> {
        let live_key = key.clone();
        self.inner.get_or_try_init(key, || {
            let live = self
                .archives
                .lock()
                .get(&live_key.cache_id)
                .and_then(Weak::upgrade);
            if let Some(archive) = live {
                return Ok(JavaArtifactProduct::new(live_key, archive));
            }
            let product = producer()?;
            let mut archives = self.archives.lock();
            archives.retain(|_, archive| archive.strong_count() > 0);
            archives.insert(live_key.cache_id, Arc::downgrade(&product.archive));
            Ok(product)
        })
    }

    pub fn snapshot(&self) -> StatsSnapshot<SingleFlightStat> {
        self.inner.snapshot()
    }

    pub fn retained_weight_bytes(&self) -> u64 {
        self.inner.retained_weight()
    }

    /// Reuse live content composed from the same ordered artifacts, or
    /// compose it once while other projects with this identity wait.
    pub(super) fn shared_content(
        &self,
        identity: CatalogContentIdentity,
        compose: impl FnOnce() -> JavaCatalogContent,
    ) -> Arc<JavaCatalogContent> {
        let mut contents = self.contents.lock();
        if let Some(content) = contents.get(&identity).and_then(Weak::upgrade) {
            return content;
        }
        contents.retain(|_, content| content.strong_count() > 0);
        let content = Arc::new(compose());
        contents.insert(identity, Arc::downgrade(&content));
        content
    }
}

#[derive(Debug, Deserialize)]
struct PersistentJavaArtifactProduct {
    schema: u32,
    cache_id: String,
    artifact_fingerprint_sha256: String,
    artifact_kind: u8,
    jdk_feature: u16,
    limits_fingerprint_sha256: String,
    archive: ArchiveMetadata,
}

/// The encoding side of [`PersistentJavaArtifactProduct`], borrowing the
/// shared archive instead of cloning it.
#[derive(Serialize)]
struct PersistentJavaArtifactProductRef<'a> {
    schema: u32,
    cache_id: &'a str,
    artifact_fingerprint_sha256: &'a str,
    artifact_kind: u8,
    jdk_feature: u16,
    limits_fingerprint_sha256: &'a str,
    archive: &'a ArchiveMetadata,
}

impl JavaArtifactProductKey {
    pub fn new(artifact: &ClasspathArtifact, jdk_feature: u16, limits: ArchiveLimits) -> Self {
        let limits_fingerprint_sha256 = archive_limits_fingerprint(limits);
        let mut identity = Sha256::new();
        identity.update(JAVA_ARTIFACT_PRODUCT_SCHEMA.to_le_bytes());
        hash_field(&mut identity, env!("CARGO_PKG_VERSION").as_bytes());
        hash_field(&mut identity, ARCHIVE_PRODUCT_SEMANTIC_VERSION.as_bytes());
        hash_field(&mut identity, artifact.fingerprint_sha256.as_bytes());
        identity.update([artifact_kind_tag(artifact.kind)]);
        identity.update(jdk_feature.to_le_bytes());
        hash_field(&mut identity, limits_fingerprint_sha256.as_bytes());
        Self {
            cache_id: format!("{:x}", identity.finalize()),
            artifact_fingerprint_sha256: artifact.fingerprint_sha256.clone(),
            artifact_kind: artifact.kind,
            jdk_feature,
            limits_fingerprint_sha256,
        }
    }

    pub fn cache_id(&self) -> &str {
        &self.cache_id
    }
}

impl JavaArtifactProduct {
    pub fn cache_id(&self) -> &str {
        self.key.cache_id()
    }

    pub fn build(
        artifact: &ClasspathArtifact,
        key: &JavaArtifactProductKey,
        archive_limits: ArchiveLimits,
    ) -> Result<Self, JavaCatalogError> {
        let expected = JavaArtifactProductKey::new(artifact, key.jdk_feature, archive_limits);
        invariant_eq!(
            key,
            &expected,
            what = "Java artifact product key does not match the artifact and limits",
            why = "metadata would publish under the wrong identity",
            fix = "derive the key from the artifact, JDK feature, and limits right before building",
        );
        let bytes = fs::read(&artifact.path).map_err(|error| JavaCatalogError::Read {
            path: artifact.path.clone(),
            message: error.to_string(),
        })?;
        if format!("{:x}", Sha256::digest(&bytes)) != artifact.fingerprint_sha256 {
            return Err(JavaCatalogError::ArtifactFingerprintMismatch {
                path: artifact.path.clone(),
            });
        }
        let archive = parse_archive(
            &bytes,
            artifact_kind(artifact.kind),
            key.jdk_feature,
            archive_limits,
        )
        .map_err(|error| JavaCatalogError::Archive {
            path: artifact.path.clone(),
            message: format!("{error:?}"),
        })?;
        invariant_eq!(
            archive.fingerprint_sha256,
            artifact.fingerprint_sha256,
            what = "archive parser content identity differs from the pre-parse SHA-256",
            why = "both hashes cover the same immutable bytes",
            fix = "keep artifact and archive fingerprint algorithms identical",
        );
        Ok(Self::new(key.clone(), Arc::new(archive)))
    }

    fn new(key: JavaArtifactProductKey, archive: Arc<ArchiveMetadata>) -> Self {
        let estimated_weight_bytes = estimate_product_weight(&key, &archive);
        Self {
            key,
            archive,
            estimated_weight_bytes,
        }
    }

    pub fn estimated_weight_bytes(&self) -> u64 {
        self.estimated_weight_bytes
    }
}

impl PersistentProduct for JavaArtifactProduct {
    type Key = JavaArtifactProductKey;

    const KIND: PersistentProductKind = PersistentProductKind::JavaArtifact;

    fn key_cache_id(key: &JavaArtifactProductKey) -> Cow<'_, str> {
        Cow::Borrowed(key.cache_id())
    }

    fn product_cache_id(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.cache_id())
    }

    fn encode(&self) -> AnyResult<Vec<u8>> {
        postcard::to_allocvec(&PersistentJavaArtifactProductRef {
            schema: JAVA_ARTIFACT_PRODUCT_SCHEMA,
            cache_id: &self.key.cache_id,
            artifact_fingerprint_sha256: &self.key.artifact_fingerprint_sha256,
            artifact_kind: artifact_kind_tag(self.key.artifact_kind),
            jdk_feature: self.key.jdk_feature,
            limits_fingerprint_sha256: &self.key.limits_fingerprint_sha256,
            archive: &self.archive,
        })
        .context("serializing persistent Java artifact metadata")
    }

    fn decode(key: &JavaArtifactProductKey, payload: &[u8]) -> AnyResult<Self> {
        let mut persisted: PersistentJavaArtifactProduct = postcard::from_bytes(payload)
            .context("deserializing persistent Java artifact metadata")?;
        if persisted.schema != JAVA_ARTIFACT_PRODUCT_SCHEMA {
            return Err(anyhow!(
                "persistent Java artifact schema {} does not match {}",
                persisted.schema,
                JAVA_ARTIFACT_PRODUCT_SCHEMA
            ));
        }
        if persisted.cache_id != key.cache_id
            || persisted.artifact_fingerprint_sha256 != key.artifact_fingerprint_sha256
            || persisted.artifact_kind != artifact_kind_tag(key.artifact_kind)
            || persisted.jdk_feature != key.jdk_feature
            || persisted.limits_fingerprint_sha256 != key.limits_fingerprint_sha256
            || persisted.archive.fingerprint_sha256 != key.artifact_fingerprint_sha256
            || persisted.archive.kind != artifact_kind(key.artifact_kind)
        {
            return Err(anyhow!(
                "persistent Java artifact metadata identity does not match the requested product"
            ));
        }
        persisted.archive.share_strings();
        Ok(Self::new(key.clone(), Arc::new(persisted.archive)))
    }
}

fn estimate_product_weight(key: &JavaArtifactProductKey, archive: &ArchiveMetadata) -> u64 {
    let mut bytes = size_of::<JavaArtifactProduct>() + size_of::<ArchiveMetadata>();
    for capacity in [
        key.cache_id.capacity(),
        key.artifact_fingerprint_sha256.capacity(),
        key.limits_fingerprint_sha256.capacity(),
    ] {
        add_product_weight(&mut bytes, capacity, "identity string");
    }
    add_product_weight(
        &mut bytes,
        usize::try_from(archive.estimated_heap_bytes()).expect_invariant(
            "a Java archive heap estimate does not fit usize",
            "the parsed archive exists in this process",
            "inspect cross-architecture metadata weight conversion",
        ),
        "shared archive metadata",
    );
    u64::try_from(bytes).expect_invariant(
        "Java artifact product weight does not fit u64",
        "one product cannot exceed the process address space",
        "inspect product weight arithmetic",
    )
}

fn add_product_weight(bytes: &mut usize, additional: usize, label: &'static str) {
    *bytes = bytes.checked_add(additional).unwrap_or_else(|| {
        unreachable_invariant!(
            what = "Java artifact {label} weight overflowed usize",
            why = "archive inputs and class counts are bounded",
            fix = "inspect product weight accounting",
            label = label,
        )
    });
}

fn artifact_kind_tag(kind: ArtifactKind) -> u8 {
    match kind {
        ArtifactKind::Jar => 1,
        ArtifactKind::Jmod => 2,
    }
}

pub(super) fn hash_field(identity: &mut Sha256, value: &[u8]) {
    identity.update(
        u64::try_from(value.len())
            .expect_invariant(
                "Java artifact identity field length exceeded u64",
                "one process cannot hold such a field",
                "reject oversized classpath identity input before hashing",
            )
            .to_le_bytes(),
    );
    identity.update(value);
}

fn archive_limits_fingerprint(limits: ArchiveLimits) -> String {
    let mut identity = Sha256::new();
    for value in [
        limits.max_archive_bytes,
        limits.max_entries,
        limits.max_entry_bytes,
        limits.max_total_decompressed_bytes,
        limits.max_class_count,
        limits.class.max_class_bytes,
        limits.class.max_constant_pool_entries,
        limits.class.max_members,
        limits.class.max_attributes,
        limits.class.max_attribute_bytes,
        limits.class.max_annotations,
        limits.class.max_annotation_depth,
    ] {
        identity.update(
            u64::try_from(value)
                .expect_invariant(
                    "Java archive limit exceeded u64",
                    "persistent product identities must be architecture-independent",
                    "keep bounded archive limits representable as u64",
                )
                .to_le_bytes(),
        );
    }
    format!("{:x}", identity.finalize())
}
