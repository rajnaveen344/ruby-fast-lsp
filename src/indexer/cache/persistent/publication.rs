//! Atomic publication and lock release for reserved persistent products.

use crate::environment::runtime::jruby::java_catalog::JavaArtifactProduct;
use crate::indexer::cache::dependency_product::GemDependencyProduct;
use anyhow::{anyhow, Context, Result};
use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::atomic::Ordering;

use super::compiled_wasm::{encode_compiled_wasm_payload, CompiledWasmProductKey};
use super::envelope::encode_envelope;
use super::{
    PersistentCompiledWasmReservation, PersistentDerivedProductReservation,
    PersistentGemProductReservation, PersistentJavaArtifactReservation, MAX_COMPRESSED_ENTRY_BYTES,
    TEMP_SEQUENCE,
};

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
