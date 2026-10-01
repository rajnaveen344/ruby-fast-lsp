//! Bounded, process-shared on-disk cache of derived gem, Java artifact, and compiled Wasm products.

use crate::environment::runtime::jruby::java_catalog::JavaArtifactProduct;
use crate::indexer::cache::dependency_product::GemDependencyProduct;
use parking_lot::Mutex;
use std::fs::File;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

mod compiled_wasm;
mod envelope;
mod layout;
mod locks;
mod lookup;
mod maintenance;
mod publication;

pub use compiled_wasm::CompiledWasmProductKey;

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
    root: PathBuf,
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
                root,
                max_entries,
                max_bytes,
                counters: PersistentProductCounters::default(),
                java_artifact_counters: PersistentProductCounters::default(),
                compiled_wasm_counters: PersistentProductCounters::default(),
                accounting: Mutex::new(CacheAccounting::default()),
            }),
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

#[cfg(test)]
mod tests;
