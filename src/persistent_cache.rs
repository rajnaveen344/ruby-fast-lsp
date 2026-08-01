use crate::dependency_product::{GemDependencyManifest, GemDependencyProduct};
use crate::runtime::jruby::java_catalog::{JavaArtifactProduct, JavaArtifactProductKey};
use anyhow::{anyhow, Context, Result};
use fs2::FileExt;
use parking_lot::{Mutex, RwLock};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

const CACHE_NAMESPACE: &str = "derived-products";
const GEM_PRODUCT_NAMESPACE: &str = "gem-products-v1";
const JAVA_ARTIFACT_PRODUCT_NAMESPACE: &str = "java-artifacts-v1";
const COMPILED_WASM_PRODUCT_NAMESPACE: &str = "compiled-wasm-v1";
const PRODUCT_EXTENSION: &str = "rflsp-product";
const GEM_PRODUCT_MAGIC: &[u8; 8] = b"RFLSPG01";
const JAVA_ARTIFACT_PRODUCT_MAGIC: &[u8; 8] = b"RFLSPJ01";
const COMPILED_WASM_PRODUCT_MAGIC: &[u8; 8] = b"RFLSPW01";
const COMPILED_WASM_PAYLOAD_SCHEMA: u32 = 1;
const COMPILED_WASM_PAYLOAD_HEADER_BYTES: usize = 4 + 8 + 32 + 8 + 8 + 32;
const ENVELOPE_SCHEMA: u32 = 1;
const ENVELOPE_HEADER_BYTES: usize = 8 + 4 + 8 + 8 + 32;
const DEFAULT_MAX_ENTRIES: usize = 4096;
const DEFAULT_MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_COMPRESSED_ENTRY_BYTES: u64 = 256 * 1024 * 1024;
const MAX_LOGICAL_ENTRY_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_COMPILED_WASM_LOGICAL_ENTRY_BYTES: u64 = 64 * 1024 * 1024;
const LOCK_WAIT_TIMEOUT: Duration = Duration::from_secs(180);
const LOCK_RETRY_INTERVAL: Duration = Duration::from_millis(20);
const RESCAN_PUBLICATION_INTERVAL: u64 = 64;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
pub struct PersistentDerivedProductCache {
    inner: Arc<PersistentDerivedProductCacheInner>,
}

struct PersistentDerivedProductCacheInner {
    root: RwLock<PathBuf>,
    max_entries: usize,
    max_bytes: u64,
    counters: PersistentProductCounters,
    java_artifact_counters: PersistentProductCounters,
    compiled_wasm_counters: PersistentProductCounters,
    accounting: Mutex<CacheAccounting>,
}

#[derive(Debug, Default)]
struct CacheAccounting {
    initialized: bool,
    entries: usize,
    bytes: u64,
    publications_since_scan: u64,
}

#[derive(Debug, Default)]
struct PersistentProductCounters {
    lookups: AtomicU64,
    hits: AtomicU64,
    misses: AtomicU64,
    producers: AtomicU64,
    corruptions: AtomicU64,
    lock_waits: AtomicU64,
    publications: AtomicU64,
    publication_failures: AtomicU64,
    evictions: AtomicU64,
    physical_read_bytes: AtomicU64,
    logical_read_bytes: AtomicU64,
    write_bytes: AtomicU64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PersistentProductSnapshot {
    pub lookups: u64,
    pub hits: u64,
    pub misses: u64,
    pub producers: u64,
    pub corruptions: u64,
    pub lock_waits: u64,
    pub publications: u64,
    pub publication_failures: u64,
    pub evictions: u64,
    pub physical_read_bytes: u64,
    pub logical_read_bytes: u64,
    pub write_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistentCacheSummary {
    pub root: PathBuf,
    pub entries: usize,
    pub bytes: u64,
}

pub enum PersistentGemProductLookup {
    Hit(Arc<GemDependencyProduct>),
    Reservation(PersistentGemProductReservation),
}

pub struct PersistentGemProductReservation {
    inner: PersistentDerivedProductReservation,
}

pub enum PersistentJavaArtifactLookup {
    Hit(Arc<JavaArtifactProduct>),
    Reservation(PersistentJavaArtifactReservation),
}

pub struct PersistentJavaArtifactReservation {
    inner: PersistentDerivedProductReservation,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledWasmProductKey {
    cache_id: String,
    source_length: u64,
    source_sha256: [u8; 32],
    compiler_identity: u64,
}

impl CompiledWasmProductKey {
    pub fn new(wasm_bytes: &[u8], compiler_identity: u64) -> Self {
        let source_length = u64::try_from(wasm_bytes.len()).expect(
            "INVARIANT VIOLATED: Wasm extension length does not fit u64. This is a bug because a source artifact cannot exceed the process address space. Fix: reject corrupt extension metadata before constructing a persistent product key.",
        );
        let source_sha256: [u8; 32] = Sha256::digest(wasm_bytes).into();
        let mut digest = Sha256::new();
        digest.update(b"ruby-fast-lsp-compiled-wasm-product-v1\0");
        digest.update(source_length.to_le_bytes());
        digest.update(source_sha256);
        digest.update(compiler_identity.to_le_bytes());
        let cache_id = format!("{:x}", digest.finalize());
        Self {
            cache_id,
            source_length,
            source_sha256,
            compiler_identity,
        }
    }

    pub fn cache_id(&self) -> &str {
        &self.cache_id
    }
}

pub enum PersistentCompiledWasmLookup {
    Hit(Arc<Vec<u8>>),
    Reservation(PersistentCompiledWasmReservation),
}

pub struct PersistentCompiledWasmReservation {
    inner: PersistentDerivedProductReservation,
}

#[derive(Debug, Clone, Copy)]
enum PersistentProductKind {
    Gem,
    JavaArtifact,
    CompiledWasm,
}

struct PersistentDerivedProductReservation {
    cache: PersistentDerivedProductCache,
    kind: PersistentProductKind,
    cache_id: String,
    product_path: PathBuf,
    key_lock: Option<File>,
    maintenance_lock: Option<File>,
}

enum DiskLookup<T> {
    Missing,
    Hit(T, u64, u64),
    Corrupt(anyhow::Error),
}

enum PersistentDerivedProductLookup<T> {
    Hit(T),
    Reservation(PersistentDerivedProductReservation),
}

#[derive(Debug)]
struct CacheEntry {
    path: PathBuf,
    bytes: u64,
    modified: SystemTime,
}

impl PersistentDerivedProductCache {
    pub fn new(root: PathBuf) -> Self {
        Self::with_limits(root, DEFAULT_MAX_ENTRIES, DEFAULT_MAX_BYTES)
    }

    pub fn with_limits(root: PathBuf, max_entries: usize, max_bytes: u64) -> Self {
        assert!(
            root.is_absolute(),
            "INVARIANT VIOLATED: persistent cache root is not absolute. This is a bug because cache ownership must never depend on the server's working directory. Fix: resolve the platform user-cache root before constructing the cache."
        );
        assert!(
            max_entries > 0,
            "INVARIANT VIOLATED: persistent cache entry limit is zero. This is a bug because a persistent cache must have a positive ownership bound. Fix: configure at least one entry."
        );
        assert!(
            max_bytes > 0,
            "INVARIANT VIOLATED: persistent cache byte limit is zero. This is a bug because a persistent cache must have a positive disk bound. Fix: configure a measured positive byte limit."
        );
        Self {
            inner: Arc::new(PersistentDerivedProductCacheInner {
                root: RwLock::new(root),
                max_entries,
                max_bytes,
                counters: PersistentProductCounters::default(),
                java_artifact_counters: PersistentProductCounters::default(),
                compiled_wasm_counters: PersistentProductCounters::default(),
                accounting: Mutex::new(CacheAccounting::default()),
            }),
        }
    }

    #[cfg(test)]
    pub fn set_root_for_tests(&self, root: PathBuf) {
        assert!(root.is_absolute(), "test cache root must be absolute");
        *self.inner.root.write() = root;
        *self.inner.accounting.lock() = CacheAccounting::default();
    }

    pub fn lookup_or_reserve(
        &self,
        manifest: &GemDependencyManifest,
    ) -> Result<PersistentGemProductLookup> {
        let cache_id = manifest.cache_id();
        match self.lookup_derived_product(PersistentProductKind::Gem, &cache_id, |payload| {
            GemDependencyProduct::decode_persistent_payload(manifest, &payload)
        })? {
            PersistentDerivedProductLookup::Hit(product) => {
                Ok(PersistentGemProductLookup::Hit(Arc::new(product)))
            }
            PersistentDerivedProductLookup::Reservation(inner) => Ok(
                PersistentGemProductLookup::Reservation(PersistentGemProductReservation { inner }),
            ),
        }
    }

    pub fn lookup_java_artifact_or_reserve(
        &self,
        key: &JavaArtifactProductKey,
    ) -> Result<PersistentJavaArtifactLookup> {
        match self.lookup_derived_product(
            PersistentProductKind::JavaArtifact,
            key.cache_id(),
            |payload| JavaArtifactProduct::decode_persistent_payload(key, &payload),
        )? {
            PersistentDerivedProductLookup::Hit(product) => {
                Ok(PersistentJavaArtifactLookup::Hit(Arc::new(product)))
            }
            PersistentDerivedProductLookup::Reservation(inner) => {
                Ok(PersistentJavaArtifactLookup::Reservation(
                    PersistentJavaArtifactReservation { inner },
                ))
            }
        }
    }

    pub fn gem_product_snapshot(&self) -> PersistentProductSnapshot {
        snapshot_counters(&self.inner.counters)
    }

    pub fn java_artifact_snapshot(&self) -> PersistentProductSnapshot {
        snapshot_counters(&self.inner.java_artifact_counters)
    }

    pub fn lookup_compiled_wasm_or_reserve(
        &self,
        key: &CompiledWasmProductKey,
    ) -> Result<PersistentCompiledWasmLookup> {
        match self.lookup_derived_product(
            PersistentProductKind::CompiledWasm,
            key.cache_id(),
            |payload| decode_compiled_wasm_payload(key, payload),
        )? {
            PersistentDerivedProductLookup::Hit(product) => {
                Ok(PersistentCompiledWasmLookup::Hit(Arc::new(product)))
            }
            PersistentDerivedProductLookup::Reservation(inner) => {
                Ok(PersistentCompiledWasmLookup::Reservation(
                    PersistentCompiledWasmReservation { inner },
                ))
            }
        }
    }

    pub fn compiled_wasm_snapshot(&self) -> PersistentProductSnapshot {
        snapshot_counters(&self.inner.compiled_wasm_counters)
    }

    pub fn invalidate_compiled_wasm(&self, key: &CompiledWasmProductKey) -> Result<bool> {
        let counters = self.counters(PersistentProductKind::CompiledWasm);
        let maintenance_lock = self.open_maintenance_lock()?;
        acquire_lock(&maintenance_lock, true, counters)?;
        let key_lock = open_private_lock_file(
            &self.key_lock_path(PersistentProductKind::CompiledWasm, key.cache_id()),
        )?;
        acquire_lock(&key_lock, true, counters)?;
        let product_path = self.product_path(PersistentProductKind::CompiledWasm, key.cache_id());
        let removed = match std::fs::remove_file(&product_path) {
            Ok(()) => true,
            Err(error) if error.kind() == ErrorKind::NotFound => false,
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "removing rejected compiled Wasm product {}",
                        product_path.display()
                    )
                });
            }
        };
        if removed {
            counters.corruptions.fetch_add(1, Ordering::Relaxed);
            *self.inner.accounting.lock() = CacheAccounting::default();
        }
        FileExt::unlock(&key_lock).context("unlocking rejected compiled Wasm key")?;
        FileExt::unlock(&maintenance_lock)
            .context("unlocking persistent cache after compiled Wasm rejection")?;
        Ok(removed)
    }

    fn counters(&self, kind: PersistentProductKind) -> &PersistentProductCounters {
        match kind {
            PersistentProductKind::Gem => &self.inner.counters,
            PersistentProductKind::JavaArtifact => &self.inner.java_artifact_counters,
            PersistentProductKind::CompiledWasm => &self.inner.compiled_wasm_counters,
        }
    }

    fn lookup_derived_product<T>(
        &self,
        kind: PersistentProductKind,
        cache_id: &str,
        decode: impl Fn(Vec<u8>) -> Result<T>,
    ) -> Result<PersistentDerivedProductLookup<T>> {
        let counters = self.counters(kind);
        counters.lookups.fetch_add(1, Ordering::Relaxed);
        self.ensure_accounting(kind)?;
        validate_cache_id(cache_id)?;
        let product_path = self.product_path(kind, cache_id);
        let initial = self.lookup_disk(kind, &product_path, &decode)?;
        let initial_was_corrupt = matches!(initial, DiskLookup::Corrupt(_));
        match initial {
            DiskLookup::Hit(product, physical, logical) => {
                record_hit(counters, physical, logical);
                return Ok(PersistentDerivedProductLookup::Hit(product));
            }
            DiskLookup::Missing => {}
            DiskLookup::Corrupt(error) => {
                log::warn!(
                    "Ignoring corrupt persistent {} product {}: {error:#}",
                    kind.label(),
                    product_path.display()
                );
                counters.corruptions.fetch_add(1, Ordering::Relaxed);
            }
        }

        let maintenance_lock = self.open_maintenance_lock()?;
        acquire_lock(&maintenance_lock, false, counters)?;
        let key_lock_path = self.key_lock_path(kind, cache_id);
        let key_lock = open_private_lock_file(&key_lock_path)?;
        acquire_lock(&key_lock, true, counters)?;

        match self.lookup_disk(kind, &product_path, &decode)? {
            DiskLookup::Hit(product, physical, logical) => {
                FileExt::unlock(&key_lock)
                    .with_context(|| format!("unlocking persistent {} key", kind.label()))?;
                FileExt::unlock(&maintenance_lock)
                    .context("unlocking persistent cache maintenance lease")?;
                record_hit(counters, physical, logical);
                Ok(PersistentDerivedProductLookup::Hit(product))
            }
            DiskLookup::Missing => {
                record_reservation(counters);
                Ok(PersistentDerivedProductLookup::Reservation(
                    PersistentDerivedProductReservation {
                        cache: self.clone(),
                        kind,
                        cache_id: cache_id.to_string(),
                        product_path,
                        key_lock: Some(key_lock),
                        maintenance_lock: Some(maintenance_lock),
                    },
                ))
            }
            DiskLookup::Corrupt(error) => {
                if !initial_was_corrupt {
                    counters.corruptions.fetch_add(1, Ordering::Relaxed);
                }
                log::warn!(
                    "Removing corrupt persistent {} product {} under the ownership lock: {error:#}",
                    kind.label(),
                    product_path.display()
                );
                match std::fs::remove_file(&product_path) {
                    Ok(()) => {}
                    Err(remove_error) if remove_error.kind() == ErrorKind::NotFound => {}
                    Err(remove_error) => {
                        return Err(remove_error).with_context(|| {
                            format!(
                                "removing corrupt persistent {} product {}",
                                kind.label(),
                                product_path.display()
                            )
                        });
                    }
                }
                record_reservation(counters);
                Ok(PersistentDerivedProductLookup::Reservation(
                    PersistentDerivedProductReservation {
                        cache: self.clone(),
                        kind,
                        cache_id: cache_id.to_string(),
                        product_path,
                        key_lock: Some(key_lock),
                        maintenance_lock: Some(maintenance_lock),
                    },
                ))
            }
        }
    }

    pub fn summary(&self) -> Result<PersistentCacheSummary> {
        let maintenance_lock = self.open_maintenance_lock()?;
        acquire_lock(&maintenance_lock, false, &self.inner.counters)?;
        let entries = scan_product_entries(&self.namespace_root())?;
        FileExt::unlock(&maintenance_lock).context("unlocking persistent cache after summary")?;
        Ok(PersistentCacheSummary {
            root: self.namespace_root(),
            entries: entries.len(),
            bytes: entries.iter().try_fold(0u64, |total, entry| {
                total.checked_add(entry.bytes).ok_or_else(|| {
                    anyhow!("persistent cache byte accounting overflowed while summarizing")
                })
            })?,
        })
    }

    pub fn clear(&self) -> Result<PersistentCacheSummary> {
        let maintenance_lock = self.open_maintenance_lock()?;
        acquire_lock(&maintenance_lock, true, &self.inner.counters)?;
        let before = scan_product_entries(&self.namespace_root())?;
        let bytes = before.iter().try_fold(0u64, |total, entry| {
            total.checked_add(entry.bytes).ok_or_else(|| {
                anyhow!("persistent cache byte accounting overflowed while clearing")
            })
        })?;
        let namespace = self.namespace_root();
        if namespace.exists() {
            std::fs::remove_dir_all(&namespace).with_context(|| {
                format!(
                    "clearing Ruby Fast LSP-owned derived products under {}",
                    namespace.display()
                )
            })?;
        }
        FileExt::unlock(&maintenance_lock).context("unlocking persistent cache after clear")?;
        *self.inner.accounting.lock() = CacheAccounting {
            initialized: true,
            entries: 0,
            bytes: 0,
            publications_since_scan: 0,
        };
        Ok(PersistentCacheSummary {
            root: namespace,
            entries: before.len(),
            bytes,
        })
    }

    #[cfg(test)]
    pub fn product_path_for_tests(&self, manifest: &GemDependencyManifest) -> PathBuf {
        self.product_path(PersistentProductKind::Gem, &manifest.cache_id())
    }

    fn lookup_disk<T>(
        &self,
        kind: PersistentProductKind,
        product_path: &Path,
        decode: &impl Fn(Vec<u8>) -> Result<T>,
    ) -> Result<DiskLookup<T>> {
        let mut file = match File::open(product_path) {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(DiskLookup::Missing),
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "opening persistent {} product {}",
                        kind.label(),
                        product_path.display()
                    )
                });
            }
        };
        let physical_len = file
            .metadata()
            .with_context(|| {
                format!(
                    "reading persistent product metadata {}",
                    product_path.display()
                )
            })?
            .len();
        if physical_len > MAX_COMPRESSED_ENTRY_BYTES {
            return Ok(DiskLookup::Corrupt(anyhow!(
                "entry is {physical_len} bytes; maximum is {MAX_COMPRESSED_ENTRY_BYTES}"
            )));
        }
        let capacity = usize::try_from(physical_len)
            .map_err(|_| anyhow!("persistent product length does not fit usize"))?;
        let mut encoded = Vec::with_capacity(capacity);
        file.read_to_end(&mut encoded).with_context(|| {
            format!(
                "reading persistent {} product {}",
                kind.label(),
                product_path.display()
            )
        })?;
        match decode_envelope(kind.magic(), kind.max_logical_entry_bytes(), &encoded)
            .and_then(decode)
        {
            Ok(product) => Ok(DiskLookup::Hit(
                product,
                physical_len,
                encoded_payload_logical_len(&encoded)?,
            )),
            Err(error) => Ok(DiskLookup::Corrupt(error)),
        }
    }

    fn ensure_accounting(&self, kind: PersistentProductKind) -> Result<()> {
        if self.inner.accounting.lock().initialized {
            return Ok(());
        }
        let maintenance_lock = self.open_maintenance_lock()?;
        acquire_lock(&maintenance_lock, true, self.counters(kind))?;
        let entries = scan_product_entries(&self.namespace_root())?;
        let bytes = entries.iter().try_fold(0u64, |total, entry| {
            total
                .checked_add(entry.bytes)
                .ok_or_else(|| anyhow!("persistent cache byte accounting overflowed"))
        })?;
        {
            let mut accounting = self.inner.accounting.lock();
            if !accounting.initialized {
                accounting.initialized = true;
                accounting.entries = entries.len();
                accounting.bytes = bytes;
                accounting.publications_since_scan = 0;
            }
        }
        FileExt::unlock(&maintenance_lock)
            .context("unlocking persistent cache accounting initialization")?;
        Ok(())
    }

    fn record_publication_and_cleanup(&self, bytes: u64) -> Result<()> {
        let should_cleanup = {
            let mut accounting = self.inner.accounting.lock();
            accounting.entries = accounting.entries.saturating_add(1);
            accounting.bytes = accounting.bytes.saturating_add(bytes);
            accounting.publications_since_scan =
                accounting.publications_since_scan.saturating_add(1);
            accounting.entries > self.inner.max_entries
                || accounting.bytes > self.inner.max_bytes
                || accounting.publications_since_scan >= RESCAN_PUBLICATION_INTERVAL
        };
        if should_cleanup {
            self.cleanup_to_limits()?;
        }
        Ok(())
    }

    fn cleanup_to_limits(&self) -> Result<()> {
        let maintenance_lock = self.open_maintenance_lock()?;
        acquire_lock(&maintenance_lock, true, &self.inner.counters)?;
        let mut entries = scan_product_entries(&self.namespace_root())?;
        entries.sort_by(|left, right| {
            left.modified
                .cmp(&right.modified)
                .then_with(|| left.path.cmp(&right.path))
        });
        let mut total = entries.iter().try_fold(0u64, |sum, entry| {
            sum.checked_add(entry.bytes)
                .ok_or_else(|| anyhow!("persistent cache byte accounting overflowed"))
        })?;
        let mut count = entries.len();
        for entry in entries {
            if count <= self.inner.max_entries && total <= self.inner.max_bytes {
                break;
            }
            match std::fs::remove_file(&entry.path) {
                Ok(()) => {
                    total = total.checked_sub(entry.bytes).ok_or_else(|| {
                        anyhow!("persistent cache eviction byte accounting underflowed")
                    })?;
                    count = count.checked_sub(1).ok_or_else(|| {
                        anyhow!("persistent cache eviction entry accounting underflowed")
                    })?;
                    self.counters(product_kind_for_path(&entry.path))
                        .evictions
                        .fetch_add(1, Ordering::Relaxed);
                }
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(error).with_context(|| {
                        format!(
                            "evicting Ruby Fast LSP persistent product {}",
                            entry.path.display()
                        )
                    });
                }
            }
        }
        *self.inner.accounting.lock() = CacheAccounting {
            initialized: true,
            entries: count,
            bytes: total,
            publications_since_scan: 0,
        };
        FileExt::unlock(&maintenance_lock)
            .context("unlocking persistent cache after bounded cleanup")?;
        Ok(())
    }

    fn cache_root(&self) -> PathBuf {
        self.inner.root.read().clone()
    }

    fn namespace_root(&self) -> PathBuf {
        self.cache_root().join(CACHE_NAMESPACE)
    }

    fn products_root(&self, kind: PersistentProductKind) -> PathBuf {
        self.namespace_root().join(kind.namespace())
    }

    fn product_path(&self, kind: PersistentProductKind, cache_id: &str) -> PathBuf {
        self.products_root(kind)
            .join(&cache_id[..2])
            .join(format!("{cache_id}.{PRODUCT_EXTENSION}"))
    }

    fn key_lock_path(&self, kind: PersistentProductKind, cache_id: &str) -> PathBuf {
        self.namespace_root()
            .join("locks")
            .join(format!("{}-{cache_id}.lock", kind.namespace()))
    }

    fn open_maintenance_lock(&self) -> Result<File> {
        open_private_lock_file(
            &self
                .cache_root()
                .join(".ruby-fast-lsp-derived-products.lock"),
        )
    }
}

impl PersistentGemProductReservation {
    pub fn publish(self, product: &GemDependencyProduct) -> Result<()> {
        self.inner
            .publish_payload(&product.cache_id(), product.encode_persistent_payload()?)
    }
}

impl PersistentJavaArtifactReservation {
    pub fn publish(self, product: &JavaArtifactProduct) -> Result<()> {
        self.inner
            .publish_payload(product.cache_id(), product.encode_persistent_payload()?)
    }
}

impl PersistentCompiledWasmReservation {
    pub fn publish(self, key: &CompiledWasmProductKey, artifact: &[u8]) -> Result<()> {
        self.inner
            .publish_payload(key.cache_id(), encode_compiled_wasm_payload(key, artifact)?)
    }
}

impl PersistentDerivedProductReservation {
    fn publish_payload(mut self, product_cache_id: &str, payload: Vec<u8>) -> Result<()> {
        if product_cache_id != self.cache_id {
            self.cache
                .counters(self.kind)
                .publication_failures
                .fetch_add(1, Ordering::Relaxed);
            return Err(anyhow!(
                "persistent reservation identity does not match {} product",
                self.kind.label()
            ));
        }
        let result = self.publish_inner(&payload);
        if result.is_err() {
            self.cache
                .counters(self.kind)
                .publication_failures
                .fetch_add(1, Ordering::Relaxed);
        }
        let write_bytes = result?;
        self.unlock()?;
        let counters = self.cache.counters(self.kind);
        counters.publications.fetch_add(1, Ordering::Relaxed);
        counters
            .write_bytes
            .fetch_add(write_bytes, Ordering::Relaxed);
        self.cache.record_publication_and_cleanup(write_bytes)?;
        Ok(())
    }

    fn publish_inner(&self, payload: &[u8]) -> Result<u64> {
        let encoded = encode_envelope(
            self.kind.magic(),
            self.kind.max_logical_entry_bytes(),
            payload,
        )?;
        let encoded_len = u64::try_from(encoded.len())
            .map_err(|_| anyhow!("persistent product encoded length exceeded u64"))?;
        if encoded_len > MAX_COMPRESSED_ENTRY_BYTES {
            return Err(anyhow!(
                "persistent product is {encoded_len} bytes; maximum is {MAX_COMPRESSED_ENTRY_BYTES}"
            ));
        }
        let parent = self.product_path.parent().ok_or_else(|| {
            anyhow!(
                "persistent product path has no parent: {}",
                self.product_path.display()
            )
        })?;
        std::fs::create_dir_all(parent).with_context(|| {
            format!("creating persistent product directory {}", parent.display())
        })?;
        let temp_path = parent.join(format!(
            ".{}.{}.{}.tmp",
            self.cache_id,
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut temp = options.open(&temp_path).with_context(|| {
            format!("creating atomic persistent product {}", temp_path.display())
        })?;
        let write_result = (|| -> Result<()> {
            temp.write_all(&encoded)
                .with_context(|| format!("writing persistent product {}", temp_path.display()))?;
            temp.sync_all()
                .with_context(|| format!("syncing persistent product {}", temp_path.display()))?;
            std::fs::rename(&temp_path, &self.product_path).with_context(|| {
                format!(
                    "publishing persistent product {}",
                    self.product_path.display()
                )
            })?;
            sync_directory(parent)?;
            Ok(())
        })();
        if write_result.is_err() {
            let _ = std::fs::remove_file(&temp_path);
        }
        write_result?;
        Ok(encoded_len)
    }

    fn unlock(&mut self) -> Result<()> {
        if let Some(key_lock) = self.key_lock.take() {
            FileExt::unlock(&key_lock).context("unlocking persistent gem-product key")?;
        }
        if let Some(maintenance_lock) = self.maintenance_lock.take() {
            FileExt::unlock(&maintenance_lock)
                .context("unlocking persistent cache maintenance lease")?;
        }
        Ok(())
    }
}

impl Drop for PersistentDerivedProductReservation {
    fn drop(&mut self) {
        if let Some(key_lock) = self.key_lock.take() {
            if let Err(error) = FileExt::unlock(&key_lock) {
                log::error!(
                    "Failed to unlock persistent {} key: {error}",
                    self.kind.label()
                );
            }
        }
        if let Some(maintenance_lock) = self.maintenance_lock.take() {
            if let Err(error) = FileExt::unlock(&maintenance_lock) {
                log::error!("Failed to unlock persistent cache maintenance lease: {error}");
            }
        }
    }
}

impl PersistentProductKind {
    fn namespace(self) -> &'static str {
        match self {
            Self::Gem => GEM_PRODUCT_NAMESPACE,
            Self::JavaArtifact => JAVA_ARTIFACT_PRODUCT_NAMESPACE,
            Self::CompiledWasm => COMPILED_WASM_PRODUCT_NAMESPACE,
        }
    }

    fn magic(self) -> &'static [u8; 8] {
        match self {
            Self::Gem => GEM_PRODUCT_MAGIC,
            Self::JavaArtifact => JAVA_ARTIFACT_PRODUCT_MAGIC,
            Self::CompiledWasm => COMPILED_WASM_PRODUCT_MAGIC,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Gem => "gem",
            Self::JavaArtifact => "Java artifact",
            Self::CompiledWasm => "compiled Wasm",
        }
    }

    fn max_logical_entry_bytes(self) -> u64 {
        match self {
            Self::Gem | Self::JavaArtifact => MAX_LOGICAL_ENTRY_BYTES,
            Self::CompiledWasm => MAX_COMPILED_WASM_LOGICAL_ENTRY_BYTES,
        }
    }
}

fn snapshot_counters(counters: &PersistentProductCounters) -> PersistentProductSnapshot {
    PersistentProductSnapshot {
        lookups: counters.lookups.load(Ordering::Relaxed),
        hits: counters.hits.load(Ordering::Relaxed),
        misses: counters.misses.load(Ordering::Relaxed),
        producers: counters.producers.load(Ordering::Relaxed),
        corruptions: counters.corruptions.load(Ordering::Relaxed),
        lock_waits: counters.lock_waits.load(Ordering::Relaxed),
        publications: counters.publications.load(Ordering::Relaxed),
        publication_failures: counters.publication_failures.load(Ordering::Relaxed),
        evictions: counters.evictions.load(Ordering::Relaxed),
        physical_read_bytes: counters.physical_read_bytes.load(Ordering::Relaxed),
        logical_read_bytes: counters.logical_read_bytes.load(Ordering::Relaxed),
        write_bytes: counters.write_bytes.load(Ordering::Relaxed),
    }
}

fn record_hit(counters: &PersistentProductCounters, physical: u64, logical: u64) {
    counters.hits.fetch_add(1, Ordering::Relaxed);
    counters
        .physical_read_bytes
        .fetch_add(physical, Ordering::Relaxed);
    counters
        .logical_read_bytes
        .fetch_add(logical, Ordering::Relaxed);
}

fn record_reservation(counters: &PersistentProductCounters) {
    counters.misses.fetch_add(1, Ordering::Relaxed);
    counters.producers.fetch_add(1, Ordering::Relaxed);
}

fn product_kind_for_path(path: &Path) -> PersistentProductKind {
    if path
        .components()
        .any(|component| component.as_os_str() == JAVA_ARTIFACT_PRODUCT_NAMESPACE)
    {
        PersistentProductKind::JavaArtifact
    } else if path
        .components()
        .any(|component| component.as_os_str() == COMPILED_WASM_PRODUCT_NAMESPACE)
    {
        PersistentProductKind::CompiledWasm
    } else {
        assert!(
            path.components()
                .any(|component| component.as_os_str() == GEM_PRODUCT_NAMESPACE),
            "INVARIANT VIOLATED: persistent cleanup found a product outside every registered \
             product namespace. This is a bug because cleanup must never assign ownership by \
             guessing. Fix: add the product namespace to the explicit kind mapping."
        );
        PersistentProductKind::Gem
    }
}

fn validate_cache_id(cache_id: &str) -> Result<()> {
    if cache_id.len() != 64
        || !cache_id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(anyhow!(
            "persistent product cache identity must be 64 lowercase hexadecimal characters"
        ));
    }
    Ok(())
}

fn open_private_lock_file(path: &Path) -> Result<File> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("cache lock path has no parent: {}", path.display()))?;
    std::fs::create_dir_all(parent).with_context(|| {
        format!(
            "creating persistent cache lock directory {}",
            parent.display()
        )
    })?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .with_context(|| format!("opening persistent cache lock {}", path.display()))
}

fn acquire_lock(file: &File, exclusive: bool, counters: &PersistentProductCounters) -> Result<()> {
    let started = Instant::now();
    let mut waited = false;
    loop {
        let result = if exclusive {
            FileExt::try_lock_exclusive(file)
        } else {
            FileExt::try_lock_shared(file)
        };
        match result {
            Ok(()) => {
                if waited {
                    counters.lock_waits.fetch_add(1, Ordering::Relaxed);
                }
                return Ok(());
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                waited = true;
                if started.elapsed() >= LOCK_WAIT_TIMEOUT {
                    return Err(anyhow!(
                        "timed out after {:?} waiting for persistent cache ownership lock",
                        LOCK_WAIT_TIMEOUT
                    ));
                }
                std::thread::sleep(LOCK_RETRY_INTERVAL);
            }
            Err(error) => return Err(error).context("acquiring persistent cache ownership lock"),
        }
    }
}

fn encode_compiled_wasm_payload(key: &CompiledWasmProductKey, artifact: &[u8]) -> Result<Vec<u8>> {
    let artifact_length = u64::try_from(artifact.len())
        .map_err(|_| anyhow!("compiled Wasm artifact length does not fit u64"))?;
    let capacity = COMPILED_WASM_PAYLOAD_HEADER_BYTES
        .checked_add(artifact.len())
        .ok_or_else(|| anyhow!("compiled Wasm payload length overflowed usize"))?;
    let mut payload = Vec::with_capacity(capacity);
    payload.extend_from_slice(&COMPILED_WASM_PAYLOAD_SCHEMA.to_le_bytes());
    payload.extend_from_slice(&key.source_length.to_le_bytes());
    payload.extend_from_slice(&key.source_sha256);
    payload.extend_from_slice(&key.compiler_identity.to_le_bytes());
    payload.extend_from_slice(&artifact_length.to_le_bytes());
    payload.extend_from_slice(&Sha256::digest(artifact));
    payload.extend_from_slice(artifact);
    Ok(payload)
}

fn decode_compiled_wasm_payload(
    key: &CompiledWasmProductKey,
    mut payload: Vec<u8>,
) -> Result<Vec<u8>> {
    if payload.len() < COMPILED_WASM_PAYLOAD_HEADER_BYTES {
        return Err(anyhow!(
            "compiled Wasm payload is shorter than its identity header"
        ));
    }
    let schema = u32::from_le_bytes(
        payload[0..4]
            .try_into()
            .map_err(|_| anyhow!("compiled Wasm payload schema is malformed"))?,
    );
    if schema != COMPILED_WASM_PAYLOAD_SCHEMA {
        return Err(anyhow!(
            "compiled Wasm payload schema {schema} does not match {COMPILED_WASM_PAYLOAD_SCHEMA}"
        ));
    }
    let source_length = u64::from_le_bytes(
        payload[4..12]
            .try_into()
            .map_err(|_| anyhow!("compiled Wasm source length is malformed"))?,
    );
    let source_sha256: [u8; 32] = payload[12..44]
        .try_into()
        .map_err(|_| anyhow!("compiled Wasm source checksum is malformed"))?;
    let compiler_identity = u64::from_le_bytes(
        payload[44..52]
            .try_into()
            .map_err(|_| anyhow!("compiled Wasm compiler identity is malformed"))?,
    );
    if source_length != key.source_length
        || source_sha256 != key.source_sha256
        || compiler_identity != key.compiler_identity
    {
        return Err(anyhow!(
            "compiled Wasm payload identity does not match its source/compiler cache key"
        ));
    }
    let artifact_length = u64::from_le_bytes(
        payload[52..60]
            .try_into()
            .map_err(|_| anyhow!("compiled Wasm artifact length is malformed"))?,
    );
    let expected_payload_length = u64::try_from(COMPILED_WASM_PAYLOAD_HEADER_BYTES)
        .expect("compiled Wasm header length must fit u64")
        .checked_add(artifact_length)
        .ok_or_else(|| anyhow!("compiled Wasm payload length overflowed u64"))?;
    if u64::try_from(payload.len()).ok() != Some(expected_payload_length) {
        return Err(anyhow!(
            "compiled Wasm payload length {} does not match declared {expected_payload_length}",
            payload.len()
        ));
    }
    let expected_artifact_sha256 = &payload[60..92];
    let actual_artifact_sha256 = Sha256::digest(&payload[COMPILED_WASM_PAYLOAD_HEADER_BYTES..]);
    if actual_artifact_sha256.as_slice() != expected_artifact_sha256 {
        return Err(anyhow!("compiled Wasm artifact checksum does not match"));
    }
    payload.drain(..COMPILED_WASM_PAYLOAD_HEADER_BYTES);
    Ok(payload)
}

fn encode_envelope(
    magic: &[u8; 8],
    max_logical_entry_bytes: u64,
    payload: &[u8],
) -> Result<Vec<u8>> {
    let logical_len = u64::try_from(payload.len())
        .map_err(|_| anyhow!("persistent product payload length exceeded u64"))?;
    if logical_len > max_logical_entry_bytes {
        return Err(anyhow!(
            "persistent product payload is {logical_len} bytes; maximum is {max_logical_entry_bytes}"
        ));
    }
    let compressed = zstd::stream::encode_all(payload, 3)
        .context("compressing persistent gem dependency product")?;
    let compressed_len = u64::try_from(compressed.len())
        .map_err(|_| anyhow!("compressed persistent product length exceeded u64"))?;
    let mut encoded = Vec::with_capacity(
        ENVELOPE_HEADER_BYTES
            .checked_add(compressed.len())
            .ok_or_else(|| anyhow!("persistent envelope size overflowed usize"))?,
    );
    encoded.extend_from_slice(magic);
    encoded.extend_from_slice(&ENVELOPE_SCHEMA.to_le_bytes());
    encoded.extend_from_slice(&logical_len.to_le_bytes());
    encoded.extend_from_slice(&compressed_len.to_le_bytes());
    encoded.extend_from_slice(&Sha256::digest(payload));
    encoded.extend_from_slice(&compressed);
    Ok(encoded)
}

fn decode_envelope(
    magic: &[u8; 8],
    max_logical_entry_bytes: u64,
    encoded: &[u8],
) -> Result<Vec<u8>> {
    if encoded.len() < ENVELOPE_HEADER_BYTES {
        return Err(anyhow!("persistent product is shorter than its envelope"));
    }
    if &encoded[..8] != magic {
        return Err(anyhow!("persistent product magic does not match"));
    }
    let schema = u32::from_le_bytes(
        encoded[8..12]
            .try_into()
            .map_err(|_| anyhow!("persistent product schema bytes are not exactly four bytes"))?,
    );
    if schema != ENVELOPE_SCHEMA {
        return Err(anyhow!(
            "persistent product envelope schema {schema} does not match {ENVELOPE_SCHEMA}"
        ));
    }
    let logical_len = encoded_payload_logical_len(encoded)?;
    let compressed_len = u64::from_le_bytes(
        encoded[20..28]
            .try_into()
            .map_err(|_| anyhow!("persistent product length field is malformed"))?,
    );
    if logical_len > max_logical_entry_bytes {
        return Err(anyhow!(
            "persistent product declares {logical_len} logical bytes; maximum is {max_logical_entry_bytes}"
        ));
    }
    if compressed_len > MAX_COMPRESSED_ENTRY_BYTES {
        return Err(anyhow!(
            "persistent product declares {compressed_len} compressed bytes; maximum is {MAX_COMPRESSED_ENTRY_BYTES}"
        ));
    }
    let compressed_len_usize = usize::try_from(compressed_len)
        .map_err(|_| anyhow!("compressed persistent product length does not fit usize"))?;
    let expected_total = ENVELOPE_HEADER_BYTES
        .checked_add(compressed_len_usize)
        .ok_or_else(|| anyhow!("persistent product total length overflowed usize"))?;
    if encoded.len() != expected_total {
        return Err(anyhow!(
            "persistent product length {} does not match declared {}",
            encoded.len(),
            expected_total
        ));
    }
    let decoder = zstd::stream::read::Decoder::new(&encoded[ENVELOPE_HEADER_BYTES..])
        .context("initializing persistent product decompressor")?;
    let limit = logical_len
        .checked_add(1)
        .ok_or_else(|| anyhow!("persistent product logical read limit overflowed"))?;
    let mut payload = Vec::with_capacity(
        usize::try_from(logical_len)
            .map_err(|_| anyhow!("persistent product logical length does not fit usize"))?,
    );
    decoder
        .take(limit)
        .read_to_end(&mut payload)
        .context("decompressing persistent gem dependency product")?;
    if u64::try_from(payload.len()).ok() != Some(logical_len) {
        return Err(anyhow!(
            "persistent product decompressed to {} bytes; expected {logical_len}",
            payload.len()
        ));
    }
    let expected_checksum = &encoded[28..60];
    let actual_checksum = Sha256::digest(&payload);
    if actual_checksum.as_slice() != expected_checksum {
        return Err(anyhow!(
            "persistent product payload checksum does not match"
        ));
    }
    Ok(payload)
}

fn encoded_payload_logical_len(encoded: &[u8]) -> Result<u64> {
    if encoded.len() < 20 {
        return Err(anyhow!(
            "persistent product is too short to contain logical length"
        ));
    }
    Ok(u64::from_le_bytes(encoded[12..20].try_into().map_err(
        |_| anyhow!("persistent product logical length field is malformed"),
    )?))
}

fn scan_product_entries(namespace_root: &Path) -> Result<Vec<CacheEntry>> {
    if !namespace_root.exists() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for kind in [
        PersistentProductKind::Gem,
        PersistentProductKind::JavaArtifact,
        PersistentProductKind::CompiledWasm,
    ] {
        let products_root = namespace_root.join(kind.namespace());
        if !products_root.exists() {
            continue;
        }
        for shard in std::fs::read_dir(&products_root).with_context(|| {
            format!(
                "reading persistent product root {}",
                products_root.display()
            )
        })? {
            let shard = shard?;
            if !shard.file_type()?.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(shard.path())? {
                let entry = entry?;
                if !entry.file_type()?.is_file()
                    || entry.path().extension().and_then(|value| value.to_str())
                        != Some(PRODUCT_EXTENSION)
                {
                    continue;
                }
                let metadata = entry.metadata()?;
                entries.push(CacheEntry {
                    path: entry.path(),
                    bytes: metadata.len(),
                    modified: metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                });
            }
        }
    }
    Ok(entries)
}

fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        File::open(path)
            .with_context(|| format!("opening cache directory {} for sync", path.display()))?
            .sync_all()
            .with_context(|| format!("syncing cache directory {}", path.display()))?;
    }
    #[cfg(windows)]
    {
        let _ = path;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        decode_envelope, CompiledWasmProductKey, PersistentCompiledWasmLookup,
        PersistentDerivedProductCache, PersistentGemProductLookup, PersistentJavaArtifactLookup,
        PersistentProductKind, COMPILED_WASM_PRODUCT_MAGIC, ENVELOPE_HEADER_BYTES, ENVELOPE_SCHEMA,
        MAX_COMPILED_WASM_LOGICAL_ENTRY_BYTES,
    };
    use crate::dependency_product::{
        GemDependencyFileTemplate, GemDependencyManifest, GemDependencyProduct, GemDependencySource,
    };
    use ruby_analysis::core::{
        FullyQualifiedName, GraphNodeFact, GraphNodeKind, RubyConstant, SourceFileId, SymbolFact,
        SymbolKind, TextRange,
    };
    use ruby_analysis::engine::{
        AnalysisEngine, AnalysisQuery, FileFacts, ProjectNeutralFileFactsTemplate,
    };
    use ruby_fast_lsp_jvm_metadata::ArchiveLimits;
    use sha2::{Digest, Sha256};
    use std::io::{Cursor, Write};
    use std::path::{Path, PathBuf};
    use zip::write::SimpleFileOptions;

    use crate::runtime::jruby::classpath::{ArtifactKind, ArtifactOrigin, ClasspathArtifact};
    use crate::runtime::jruby::java_catalog::{JavaArtifactProduct, JavaArtifactProductKey};

    fn source_with_content(physical_path: &Path, content: &str) -> GemDependencySource {
        GemDependencySource::new(
            0,
            "gems/widget/1.0.0/ruby/registry/0/widget.rb".to_string(),
            physical_path.to_path_buf(),
            content.to_string(),
        )
        .unwrap()
    }

    fn manifest(physical_path: &Path) -> GemDependencyManifest {
        manifest_with_provider(physical_path, None)
    }

    fn manifest_with_provider(
        physical_path: &Path,
        runtime_provider_fingerprint: Option<&str>,
    ) -> GemDependencyManifest {
        manifest_with_inputs(
            physical_path,
            "class Widget; end",
            runtime_provider_fingerprint,
            &["widget:1.0.0:ruby:registry".to_string()],
            "class CacheSeed; end",
        )
    }

    fn manifest_with_inputs(
        physical_path: &Path,
        content: &str,
        runtime_provider_fingerprint: Option<&str>,
        closure_identities: &[String],
        seed_content: &str,
    ) -> GemDependencyManifest {
        let mut seed_engine = AnalysisEngine::new();
        let seed_file = seed_engine.register_file(ruby_analysis::engine::SourceFileInput {
            path: PathBuf::from("/stubs/cache_seed.rb"),
            content: seed_content.to_string(),
            kind: ruby_analysis::core::SourceKind::Stub,
        });
        let seed_range = TextRange::new(
            seed_file,
            0,
            u32::try_from(seed_content.len()).expect("test seed must fit a source range"),
        );
        let seed_constant = if seed_content.contains("ChangedCacheSeed") {
            "ChangedCacheSeed"
        } else {
            "CacheSeed"
        };
        let seed_fqn =
            FullyQualifiedName::namespace(vec![RubyConstant::new(seed_constant).unwrap()]);
        seed_engine.replace_facts(
            seed_file,
            FileFacts {
                symbols: vec![SymbolFact::new(
                    seed_fqn.clone(),
                    SymbolKind::Class,
                    seed_range,
                )],
                graph_nodes: vec![GraphNodeFact::new(
                    seed_fqn,
                    GraphNodeKind::Class,
                    seed_range,
                )],
                ..FileFacts::default()
            },
            ruby_analysis::engine::ResolveMode::Deferred,
        );
        GemDependencyManifest::new(
            seed_engine.semantic_context_fingerprint(),
            runtime_provider_fingerprint,
            closure_identities,
            vec![source_with_content(physical_path, content)],
        )
        .unwrap()
    }

    fn product(manifest: &GemDependencyManifest) -> GemDependencyProduct {
        let file_id = SourceFileId(91);
        let range = TextRange::new(file_id, 0, 17);
        let fqn = FullyQualifiedName::namespace(vec![RubyConstant::new("Widget").unwrap()]);
        let facts = ProjectNeutralFileFactsTemplate::try_new(
            file_id,
            FileFacts {
                symbols: vec![SymbolFact::new(fqn.clone(), SymbolKind::Class, range)],
                graph_nodes: vec![GraphNodeFact::new(fqn, GraphNodeKind::Class, range)],
                ..FileFacts::default()
            },
        )
        .unwrap();
        GemDependencyProduct::new(
            manifest,
            vec![GemDependencyFileTemplate::new(
                manifest.sources()[0].logical_path.clone(),
                manifest.sources()[0].content_sha256,
                facts,
            )],
        )
        .unwrap()
    }

    fn java_artifact(path: PathBuf) -> ClasspathArtifact {
        let digits = include_str!("../crates/jvm-metadata/fixtures/minimal_class.hex")
            .bytes()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect::<Vec<_>>();
        let class = digits
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("com/example/Demo.class", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(&class).unwrap();
        let bytes = writer.finish().unwrap().into_inner();
        std::fs::write(&path, &bytes).unwrap();
        let file_identity = crate::runtime::jruby::classpath::SourceFileIdentity {
            byte_length: bytes.len() as u64,
            modified: std::fs::metadata(&path).unwrap().modified().unwrap(),
        };
        ClasspathArtifact {
            path,
            origin: ArtifactOrigin::Explicit,
            kind: ArtifactKind::Jar,
            fingerprint_sha256: format!("{:x}", Sha256::digest(&bytes)),
            byte_length: bytes.len() as u64,
            file_identity,
        }
    }

    #[test]
    fn fresh_cache_load_rebinds_exact_path_and_corruption_recovers() {
        let fixture = tempfile::tempdir().unwrap();
        let first_path = PathBuf::from("/projects/one/gems/widget/lib/widget.rb");
        let first_manifest = manifest(&first_path);
        let first_product = product(&first_manifest);

        let first_cache = PersistentDerivedProductCache::with_limits(
            fixture.path().to_path_buf(),
            8,
            1024 * 1024,
        );
        let PersistentGemProductLookup::Reservation(reservation) =
            first_cache.lookup_or_reserve(&first_manifest).unwrap()
        else {
            panic!("a fresh persistent cache must reserve one producer");
        };
        reservation.publish(&first_product).unwrap();
        drop(first_cache);

        let second_path = PathBuf::from("/projects/two/gems/widget/lib/widget.rb");
        let second_manifest = manifest(&second_path);
        let second_cache = PersistentDerivedProductCache::with_limits(
            fixture.path().to_path_buf(),
            8,
            1024 * 1024,
        );
        let PersistentGemProductLookup::Hit(second_product) =
            second_cache.lookup_or_reserve(&second_manifest).unwrap()
        else {
            panic!("a fresh cache instance must load the published product");
        };
        let mut second_engine = AnalysisEngine::new();
        second_product
            .bind_into(&second_manifest, &mut second_engine)
            .unwrap();
        let parts = [RubyConstant::new("Widget").unwrap()];
        let definition =
            AnalysisQuery::new(&second_engine).constant_definition_ranges(&parts, &[])[0];
        assert_eq!(
            second_engine.file(definition.file_id).unwrap().path,
            second_path
        );
        assert_eq!(second_cache.gem_product_snapshot().hits, 1);
        assert_eq!(second_cache.gem_product_snapshot().producers, 0);

        std::fs::write(
            second_cache.product_path_for_tests(&second_manifest),
            b"corrupt",
        )
        .unwrap();
        let recovering_cache = PersistentDerivedProductCache::with_limits(
            fixture.path().to_path_buf(),
            8,
            1024 * 1024,
        );
        let PersistentGemProductLookup::Reservation(rebuild) = recovering_cache
            .lookup_or_reserve(&second_manifest)
            .unwrap()
        else {
            panic!("a corrupt product must reserve one deterministic rebuild");
        };
        assert_eq!(recovering_cache.gem_product_snapshot().corruptions, 1);
        rebuild.publish(&first_product).unwrap();
        assert!(matches!(
            recovering_cache
                .lookup_or_reserve(&second_manifest)
                .unwrap(),
            PersistentGemProductLookup::Hit(_)
        ));
    }

    #[test]
    fn obsolete_gem_products_are_never_selected_across_semantic_input_changes() {
        let fixture = tempfile::tempdir().unwrap();
        let physical_path = Path::new("/projects/one/gems/widget/lib/widget.rb");
        let current = manifest(physical_path);
        let cache = PersistentDerivedProductCache::with_limits(
            fixture.path().to_path_buf(),
            16,
            1024 * 1024,
        );
        let PersistentGemProductLookup::Reservation(reservation) =
            cache.lookup_or_reserve(&current).unwrap()
        else {
            panic!("the current product must reserve its initial publisher");
        };
        reservation.publish(&product(&current)).unwrap();
        assert!(matches!(
            cache.lookup_or_reserve(&current).unwrap(),
            PersistentGemProductLookup::Hit(_)
        ));

        let changed_source = manifest_with_inputs(
            physical_path,
            "class Widget; def changed; end; end",
            None,
            &["widget:1.0.0:ruby:registry".to_string()],
            "class CacheSeed; end",
        );
        let changed_lock_closure = manifest_with_inputs(
            physical_path,
            "class Widget; end",
            None,
            &[
                "widget:1.0.0:ruby:registry".to_string(),
                "support:2.0.0:ruby:registry".to_string(),
            ],
            "class CacheSeed; end",
        );
        let changed_core_or_runtime_seed = manifest_with_inputs(
            physical_path,
            "class Widget; end",
            None,
            &["widget:1.0.0:ruby:registry".to_string()],
            "class ChangedCacheSeed; end",
        );
        let changed_jruby_classpath = manifest_with_inputs(
            physical_path,
            "class Widget; end",
            Some("jruby-classpath-b"),
            &["widget:1.0.0:ruby:registry".to_string()],
            "class CacheSeed; end",
        );

        for obsolete_identity in [
            changed_source,
            changed_lock_closure,
            changed_core_or_runtime_seed,
            changed_jruby_classpath,
        ] {
            assert!(
                matches!(
                    cache.lookup_or_reserve(&obsolete_identity).unwrap(),
                    PersistentGemProductLookup::Reservation(_)
                ),
                "a changed source, lock closure, core/runtime semantic seed, or classpath must never select the old product"
            );
        }
        assert_eq!(cache.gem_product_snapshot().hits, 1);
        assert_eq!(
            cache.summary().unwrap().entries,
            1,
            "unpublished replacement identities must not mutate the valid current product"
        );
    }

    #[test]
    fn fresh_java_artifact_cache_loads_exact_metadata_and_recovers_corruption() {
        let fixture = tempfile::tempdir().unwrap();
        let artifact = java_artifact(fixture.path().join("fixture.jar"));
        let limits = ArchiveLimits::default();
        let key = JavaArtifactProductKey::new(&artifact, 17, limits);
        let product = JavaArtifactProduct::build(&artifact, &key, limits).unwrap();
        let first_cache = PersistentDerivedProductCache::with_limits(
            fixture.path().to_path_buf(),
            8,
            1024 * 1024,
        );
        let PersistentJavaArtifactLookup::Reservation(reservation) =
            first_cache.lookup_java_artifact_or_reserve(&key).unwrap()
        else {
            panic!("a fresh Java artifact cache must reserve one producer");
        };
        reservation.publish(&product).unwrap();
        drop(first_cache);

        let second_cache = PersistentDerivedProductCache::with_limits(
            fixture.path().to_path_buf(),
            8,
            1024 * 1024,
        );
        let PersistentJavaArtifactLookup::Hit(hit) =
            second_cache.lookup_java_artifact_or_reserve(&key).unwrap()
        else {
            panic!("a fresh cache instance must load Java artifact metadata");
        };
        assert_eq!(hit.cache_id(), key.cache_id());
        assert_eq!(second_cache.java_artifact_snapshot().hits, 1);
        assert_eq!(second_cache.summary().unwrap().entries, 1);

        let product_path =
            second_cache.product_path(PersistentProductKind::JavaArtifact, key.cache_id());
        std::fs::write(&product_path, b"corrupt").unwrap();
        let recovering_cache = PersistentDerivedProductCache::with_limits(
            fixture.path().to_path_buf(),
            8,
            1024 * 1024,
        );
        let PersistentJavaArtifactLookup::Reservation(rebuild) = recovering_cache
            .lookup_java_artifact_or_reserve(&key)
            .unwrap()
        else {
            panic!("corrupt Java metadata must reserve one deterministic rebuild");
        };
        assert_eq!(recovering_cache.java_artifact_snapshot().corruptions, 1);
        rebuild.publish(&product).unwrap();
        assert!(matches!(
            recovering_cache
                .lookup_java_artifact_or_reserve(&key)
                .unwrap(),
            PersistentJavaArtifactLookup::Hit(_)
        ));
    }

    #[test]
    fn changed_java_artifact_identity_never_selects_persisted_old_metadata() {
        let fixture = tempfile::tempdir().unwrap();
        let artifact = java_artifact(fixture.path().join("fixture.jar"));
        let limits = ArchiveLimits::default();
        let original_key = JavaArtifactProductKey::new(&artifact, 17, limits);
        let original_product =
            JavaArtifactProduct::build(&artifact, &original_key, limits).unwrap();
        let cache = PersistentDerivedProductCache::with_limits(
            fixture.path().to_path_buf(),
            8,
            1024 * 1024,
        );
        let PersistentJavaArtifactLookup::Reservation(reservation) = cache
            .lookup_java_artifact_or_reserve(&original_key)
            .unwrap()
        else {
            panic!("the original Java artifact must reserve its publisher");
        };
        reservation.publish(&original_product).unwrap();
        assert!(matches!(
            cache
                .lookup_java_artifact_or_reserve(&original_key)
                .unwrap(),
            PersistentJavaArtifactLookup::Hit(_)
        ));

        let mut changed_artifact = artifact.clone();
        changed_artifact.fingerprint_sha256 =
            format!("{:x}", Sha256::digest(b"changed Java artifact bytes"));
        changed_artifact.byte_length += 1;
        changed_artifact.file_identity.byte_length += 1;
        let changed_key = JavaArtifactProductKey::new(&changed_artifact, 17, limits);
        assert_ne!(original_key.cache_id(), changed_key.cache_id());
        assert!(
            matches!(
                cache.lookup_java_artifact_or_reserve(&changed_key).unwrap(),
                PersistentJavaArtifactLookup::Reservation(_)
            ),
            "changed Java bytes must reserve a new exact product instead of selecting old metadata"
        );
    }

    #[test]
    fn compiled_wasm_envelope_rejects_oversized_logical_payload_before_decompression() {
        let mut encoded = Vec::with_capacity(ENVELOPE_HEADER_BYTES);
        encoded.extend_from_slice(COMPILED_WASM_PRODUCT_MAGIC);
        encoded.extend_from_slice(&ENVELOPE_SCHEMA.to_le_bytes());
        encoded.extend_from_slice(&(MAX_COMPILED_WASM_LOGICAL_ENTRY_BYTES + 1).to_le_bytes());
        encoded.extend_from_slice(&0u64.to_le_bytes());
        encoded.extend_from_slice(&[0; 32]);

        let error = decode_envelope(
            COMPILED_WASM_PRODUCT_MAGIC,
            MAX_COMPILED_WASM_LOGICAL_ENTRY_BYTES,
            &encoded,
        )
        .expect_err("oversized logical Wasm payload must fail before allocation/decompression");

        assert!(error.to_string().contains("maximum is 67108864"));
    }

    #[test]
    fn compiled_wasm_cache_validates_source_compiler_payload_and_corruption() {
        let fixture = tempfile::tempdir().unwrap();
        let source = b"\0asm exact extension bytes";
        let key = CompiledWasmProductKey::new(source, 17);
        let artifact = b"byte-exact wasmtime serialized module".to_vec();
        let first = PersistentDerivedProductCache::with_limits(
            fixture.path().to_path_buf(),
            8,
            1024 * 1024,
        );
        let PersistentCompiledWasmLookup::Reservation(reservation) =
            first.lookup_compiled_wasm_or_reserve(&key).unwrap()
        else {
            panic!("a fresh compiled-Wasm identity must reserve one producer");
        };
        reservation.publish(&key, &artifact).unwrap();
        drop(first);

        let second = PersistentDerivedProductCache::with_limits(
            fixture.path().to_path_buf(),
            8,
            1024 * 1024,
        );
        let PersistentCompiledWasmLookup::Hit(hit) =
            second.lookup_compiled_wasm_or_reserve(&key).unwrap()
        else {
            panic!("a fresh cache instance must load the exact compiled Wasm artifact");
        };
        assert_eq!(hit.as_slice(), artifact);
        assert_eq!(second.compiled_wasm_snapshot().hits, 1);

        let changed_source = CompiledWasmProductKey::new(b"\0asm changed extension bytes", 17);
        assert!(matches!(
            second
                .lookup_compiled_wasm_or_reserve(&changed_source)
                .unwrap(),
            PersistentCompiledWasmLookup::Reservation(_)
        ));
        let changed_compiler = CompiledWasmProductKey::new(source, 18);
        assert!(matches!(
            second
                .lookup_compiled_wasm_or_reserve(&changed_compiler)
                .unwrap(),
            PersistentCompiledWasmLookup::Reservation(_)
        ));

        let product_path = second.product_path(PersistentProductKind::CompiledWasm, key.cache_id());
        std::fs::write(&product_path, b"corrupt").unwrap();
        let recovering = PersistentDerivedProductCache::with_limits(
            fixture.path().to_path_buf(),
            8,
            1024 * 1024,
        );
        let PersistentCompiledWasmLookup::Reservation(rebuild) =
            recovering.lookup_compiled_wasm_or_reserve(&key).unwrap()
        else {
            panic!("a corrupt compiled Wasm artifact must reserve one deterministic rebuild");
        };
        assert_eq!(recovering.compiled_wasm_snapshot().corruptions, 1);
        rebuild.publish(&key, &artifact).unwrap();
    }

    #[test]
    fn cross_instance_lock_admits_one_publisher() {
        let fixture = tempfile::tempdir().unwrap();
        let manifest = manifest(Path::new("/projects/one/gems/widget/lib/widget.rb"));
        let product = product(&manifest);
        let first = PersistentDerivedProductCache::with_limits(
            fixture.path().to_path_buf(),
            8,
            1024 * 1024,
        );
        let PersistentGemProductLookup::Reservation(reservation) =
            first.lookup_or_reserve(&manifest).unwrap()
        else {
            panic!("first cache instance must own publication");
        };

        let second = PersistentDerivedProductCache::with_limits(
            fixture.path().to_path_buf(),
            8,
            1024 * 1024,
        );
        let second_thread = second.clone();
        let second_manifest = manifest.clone();
        let waiter =
            std::thread::spawn(move || second_thread.lookup_or_reserve(&second_manifest).unwrap());
        std::thread::sleep(std::time::Duration::from_millis(60));
        assert!(
            !waiter.is_finished(),
            "a second cache instance must wait for the exact ownership lock"
        );
        reservation.publish(&product).unwrap();

        assert!(matches!(
            waiter.join().unwrap(),
            PersistentGemProductLookup::Hit(_)
        ));
        assert_eq!(first.gem_product_snapshot().producers, 1);
        assert_eq!(second.gem_product_snapshot().producers, 0);
        assert_eq!(second.gem_product_snapshot().hits, 1);
        assert_eq!(second.gem_product_snapshot().lock_waits, 1);
    }

    #[test]
    fn cross_instance_compiled_wasm_lock_admits_one_publisher() {
        let fixture = tempfile::tempdir().unwrap();
        let key = CompiledWasmProductKey::new(b"\0asm shared extension", 44);
        let artifact = b"shared serialized module".to_vec();
        let first = PersistentDerivedProductCache::with_limits(
            fixture.path().to_path_buf(),
            8,
            1024 * 1024,
        );
        let PersistentCompiledWasmLookup::Reservation(reservation) =
            first.lookup_compiled_wasm_or_reserve(&key).unwrap()
        else {
            panic!("first cache instance must own compiled Wasm publication");
        };

        let second = PersistentDerivedProductCache::with_limits(
            fixture.path().to_path_buf(),
            8,
            1024 * 1024,
        );
        let second_thread = second.clone();
        let second_key = key.clone();
        let waiter = std::thread::spawn(move || {
            second_thread
                .lookup_compiled_wasm_or_reserve(&second_key)
                .unwrap()
        });
        std::thread::sleep(std::time::Duration::from_millis(60));
        assert!(
            !waiter.is_finished(),
            "a second compiled Wasm requester must wait for the exact ownership lock"
        );
        reservation.publish(&key, &artifact).unwrap();

        let PersistentCompiledWasmLookup::Hit(hit) = waiter.join().unwrap() else {
            panic!("the compiled Wasm waiter must reuse the first publisher's artifact");
        };
        assert_eq!(hit.as_slice(), artifact);
        assert_eq!(first.compiled_wasm_snapshot().producers, 1);
        assert_eq!(second.compiled_wasm_snapshot().producers, 0);
        assert_eq!(second.compiled_wasm_snapshot().hits, 1);
        assert_eq!(second.compiled_wasm_snapshot().lock_waits, 1);
    }

    #[test]
    fn bounded_cleanup_and_clear_touch_only_owned_products() {
        let fixture = tempfile::tempdir().unwrap();
        let unrelated = fixture.path().join("bundler/cache/widget-1.0.0.gem");
        std::fs::create_dir_all(unrelated.parent().unwrap()).unwrap();
        std::fs::write(&unrelated, b"package-manager-owned").unwrap();
        let cache = PersistentDerivedProductCache::with_limits(
            fixture.path().to_path_buf(),
            1,
            1024 * 1024,
        );

        let first_manifest = manifest(Path::new("/projects/one/gems/widget/lib/widget.rb"));
        let first_product = product(&first_manifest);
        let PersistentGemProductLookup::Reservation(first) =
            cache.lookup_or_reserve(&first_manifest).unwrap()
        else {
            panic!("first bounded product must reserve");
        };
        first.publish(&first_product).unwrap();

        let second_manifest = manifest_with_provider(
            Path::new("/projects/one/gems/widget/lib/widget.rb"),
            Some("jruby-catalog"),
        );
        let second_product = product(&second_manifest);
        let PersistentGemProductLookup::Reservation(second) =
            cache.lookup_or_reserve(&second_manifest).unwrap()
        else {
            panic!("second bounded product must reserve");
        };
        second.publish(&second_product).unwrap();
        let summary = cache.summary().unwrap();
        assert_eq!(summary.entries, 1);
        assert!(summary.bytes > 0);
        assert_eq!(cache.gem_product_snapshot().evictions, 1);
        assert!(matches!(
            cache.lookup_or_reserve(&second_manifest).unwrap(),
            PersistentGemProductLookup::Hit(_)
        ));

        let cleared = cache.clear().unwrap();
        assert_eq!(cleared.entries, 1);
        assert_eq!(cache.summary().unwrap().entries, 0);
        assert_eq!(std::fs::read(&unrelated).unwrap(), b"package-manager-owned");
    }

    #[test]
    fn fresh_process_loads_nonempty_semantic_seed_product() {
        const CHILD_ROOT: &str = "RUBY_FAST_LSP_PERSISTENT_CACHE_CHILD_ROOT";
        let physical_path = Path::new("/projects/one/gems/widget/lib/widget.rb");
        if let Some(root) = std::env::var_os(CHILD_ROOT) {
            for index in 0..512 {
                let _ = RubyConstant::new(&format!("PersistentCacheNoise{index}")).unwrap();
            }
            let cache =
                PersistentDerivedProductCache::with_limits(PathBuf::from(root), 8, 1024 * 1024);
            assert!(matches!(
                cache.lookup_or_reserve(&manifest(physical_path)).unwrap(),
                PersistentGemProductLookup::Hit(_)
            ));
            println!("RUBY_FAST_LSP_PERSISTENT_CACHE_CHILD=hit");
            return;
        }

        let fixture = tempfile::tempdir().unwrap();
        let manifest = manifest(physical_path);
        let cache = PersistentDerivedProductCache::with_limits(
            fixture.path().to_path_buf(),
            8,
            1024 * 1024,
        );
        let PersistentGemProductLookup::Reservation(reservation) =
            cache.lookup_or_reserve(&manifest).unwrap()
        else {
            panic!("parent process must publish the product");
        };
        reservation.publish(&product(&manifest)).unwrap();

        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "persistent_cache::tests::fresh_process_loads_nonempty_semantic_seed_product",
                "--nocapture",
            ])
            .env(CHILD_ROOT, fixture.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "persistent-cache child failed:\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8(output.stdout)
            .unwrap()
            .contains("RUBY_FAST_LSP_PERSISTENT_CACHE_CHILD=hit"));
    }

    #[test]
    fn fresh_process_loads_java_artifact_metadata() {
        const CHILD_ROOT: &str = "RUBY_FAST_LSP_JAVA_CACHE_CHILD_ROOT";
        const CHILD_ARTIFACT: &str = "RUBY_FAST_LSP_JAVA_CACHE_CHILD_ARTIFACT";
        if let (Some(root), Some(path)) = (
            std::env::var_os(CHILD_ROOT),
            std::env::var_os(CHILD_ARTIFACT),
        ) {
            let path = PathBuf::from(path);
            let bytes = std::fs::read(&path).unwrap();
            let metadata = std::fs::metadata(&path).unwrap();
            let artifact = ClasspathArtifact {
                path,
                origin: ArtifactOrigin::Explicit,
                kind: ArtifactKind::Jar,
                fingerprint_sha256: format!("{:x}", Sha256::digest(&bytes)),
                byte_length: bytes.len() as u64,
                file_identity: crate::runtime::jruby::classpath::SourceFileIdentity {
                    byte_length: metadata.len(),
                    modified: metadata.modified().unwrap(),
                },
            };
            let key = JavaArtifactProductKey::new(&artifact, 17, ArchiveLimits::default());
            let cache =
                PersistentDerivedProductCache::with_limits(PathBuf::from(root), 8, 1024 * 1024);
            assert!(matches!(
                cache.lookup_java_artifact_or_reserve(&key).unwrap(),
                PersistentJavaArtifactLookup::Hit(_)
            ));
            println!("RUBY_FAST_LSP_JAVA_CACHE_CHILD=hit");
            return;
        }

        let fixture = tempfile::tempdir().unwrap();
        let artifact = java_artifact(fixture.path().join("fresh-process.jar"));
        let key = JavaArtifactProductKey::new(&artifact, 17, ArchiveLimits::default());
        let product =
            JavaArtifactProduct::build(&artifact, &key, ArchiveLimits::default()).unwrap();
        let cache = PersistentDerivedProductCache::with_limits(
            fixture.path().to_path_buf(),
            8,
            1024 * 1024,
        );
        let PersistentJavaArtifactLookup::Reservation(reservation) =
            cache.lookup_java_artifact_or_reserve(&key).unwrap()
        else {
            panic!("parent process must publish Java artifact metadata");
        };
        reservation.publish(&product).unwrap();

        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "persistent_cache::tests::fresh_process_loads_java_artifact_metadata",
                "--nocapture",
            ])
            .env(CHILD_ROOT, fixture.path())
            .env(CHILD_ARTIFACT, &artifact.path)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "Java artifact cache child failed:\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8(output.stdout)
            .unwrap()
            .contains("RUBY_FAST_LSP_JAVA_CACHE_CHILD=hit"));
    }

    #[test]
    fn fresh_process_loads_compiled_wasm_artifact() {
        const CHILD_ROOT: &str = "RUBY_FAST_LSP_WASM_CACHE_CHILD_ROOT";
        let source = b"\0asm fresh-process extension source";
        let artifact = b"fresh-process serialized Wasmtime module";
        let key = CompiledWasmProductKey::new(source, 91);
        if let Some(root) = std::env::var_os(CHILD_ROOT) {
            let cache =
                PersistentDerivedProductCache::with_limits(PathBuf::from(root), 8, 1024 * 1024);
            let PersistentCompiledWasmLookup::Hit(hit) =
                cache.lookup_compiled_wasm_or_reserve(&key).unwrap()
            else {
                panic!("fresh child process must load the compiled Wasm artifact");
            };
            assert_eq!(hit.as_slice(), artifact);
            println!("RUBY_FAST_LSP_WASM_CACHE_CHILD=hit");
            return;
        }

        let fixture = tempfile::tempdir().unwrap();
        let cache = PersistentDerivedProductCache::with_limits(
            fixture.path().to_path_buf(),
            8,
            1024 * 1024,
        );
        let PersistentCompiledWasmLookup::Reservation(reservation) =
            cache.lookup_compiled_wasm_or_reserve(&key).unwrap()
        else {
            panic!("parent process must publish the compiled Wasm artifact");
        };
        reservation.publish(&key, artifact).unwrap();

        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "persistent_cache::tests::fresh_process_loads_compiled_wasm_artifact",
                "--nocapture",
            ])
            .env(CHILD_ROOT, fixture.path())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "compiled Wasm cache child failed:\nstdout={}\nstderr={}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8(output.stdout)
            .unwrap()
            .contains("RUBY_FAST_LSP_WASM_CACHE_CHILD=hit"));
    }
}
