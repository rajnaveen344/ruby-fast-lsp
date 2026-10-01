//! Cache-wide maintenance: accounting, bounded eviction, summaries, and clearing.

use anyhow::{anyhow, Context, Result};
use fs2::FileExt;
use std::io::ErrorKind;
use std::sync::atomic::Ordering;

use super::layout::{product_kind_for_path, scan_product_entries};
use super::locks::{acquire_lock, try_acquire_lock};
use super::{
    CacheAccounting, PersistentCacheSummary, PersistentDerivedProductCache, PersistentProductKind,
    RESCAN_PUBLICATION_INTERVAL,
};

impl PersistentDerivedProductCache {
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

    pub(super) fn ensure_accounting(&self, kind: PersistentProductKind) -> Result<()> {
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

    pub(super) fn record_publication_and_cleanup(&self, bytes: u64) -> Result<()> {
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
        if !try_acquire_lock(&maintenance_lock, true)? {
            // In-flight reservations hold a shared maintenance lease across
            // product construction. Blocking for exclusive cleanup deadlocks
            // those producers when they later publish. Retry on a later
            // publication once the lease is free.
            log::info!(
                "Skipping persistent product cleanup because in-flight reservations hold the maintenance lease"
            );
            return Ok(());
        }
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
}
