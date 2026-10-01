//! Product lookup, reservation under ownership locks, and per-kind counters.

use crate::environment::runtime::jruby::java_catalog::{
    JavaArtifactProduct, JavaArtifactProductKey,
};
use crate::indexer::cache::dependency_product::{GemDependencyManifest, GemDependencyProduct};
use anyhow::{anyhow, Context, Result};
use fs2::FileExt;
use ruby_analysis::stats::StatsSnapshot;
use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::Path;
use std::sync::Arc;

use super::compiled_wasm::{decode_compiled_wasm_payload, CompiledWasmProductKey};
use super::envelope::{decode_envelope, encoded_payload_logical_len};
use super::layout::validate_cache_id;
use super::locks::{acquire_lock, open_private_lock_file};
use super::{
    CacheAccounting, DiskLookup, PersistentCompiledWasmLookup, PersistentCompiledWasmReservation,
    PersistentDerivedProductCache, PersistentDerivedProductLookup,
    PersistentDerivedProductReservation, PersistentGemProductLookup,
    PersistentGemProductReservation, PersistentJavaArtifactLookup,
    PersistentJavaArtifactReservation, PersistentProductCounters, PersistentProductKind,
    PersistentProductStat, MAX_COMPRESSED_ENTRY_BYTES,
};

impl PersistentDerivedProductCache {
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

    pub fn gem_product_snapshot(&self) -> StatsSnapshot<PersistentProductStat> {
        self.inner.counters.snapshot()
    }

    pub fn java_artifact_snapshot(&self) -> StatsSnapshot<PersistentProductStat> {
        self.inner.java_artifact_counters.snapshot()
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

    pub fn compiled_wasm_snapshot(&self) -> StatsSnapshot<PersistentProductStat> {
        self.inner.compiled_wasm_counters.snapshot()
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
            counters.increment(PersistentProductStat::Corruptions);
            *self.inner.accounting.lock() = CacheAccounting::default();
        }
        FileExt::unlock(&key_lock).context("unlocking rejected compiled Wasm key")?;
        FileExt::unlock(&maintenance_lock)
            .context("unlocking persistent cache after compiled Wasm rejection")?;
        Ok(removed)
    }

    pub(super) fn counters(&self, kind: PersistentProductKind) -> &PersistentProductCounters {
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
        counters.increment(PersistentProductStat::Lookups);
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
                counters.increment(PersistentProductStat::Corruptions);
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
                    counters.increment(PersistentProductStat::Corruptions);
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
}

fn record_hit(counters: &PersistentProductCounters, physical: u64, logical: u64) {
    counters.increment(PersistentProductStat::Hits);
    counters.record(PersistentProductStat::PhysicalReadBytes, physical);
    counters.record(PersistentProductStat::LogicalReadBytes, logical);
}

fn record_reservation(counters: &PersistentProductCounters) {
    counters.increment(PersistentProductStat::Misses);
    counters.increment(PersistentProductStat::Producers);
}
