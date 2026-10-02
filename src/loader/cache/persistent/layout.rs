//! On-disk namespace layout: product paths, lock paths, identity validation, and entry scans.

use anyhow::{anyhow, Context, Result};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::locks::open_private_lock_file;
use super::{
    CacheEntry, PersistentDerivedProductCache, PersistentProductKind, CACHE_NAMESPACE,
    COMPILED_WASM_PRODUCT_NAMESPACE, GEM_PRODUCT_NAMESPACE, JAVA_ARTIFACT_PRODUCT_NAMESPACE,
    PRODUCT_EXTENSION,
};

impl PersistentDerivedProductCache {
    pub(crate) fn cache_root(&self) -> PathBuf {
        self.inner.root.clone()
    }

    pub(super) fn namespace_root(&self) -> PathBuf {
        self.cache_root().join(CACHE_NAMESPACE)
    }

    fn products_root(&self, kind: PersistentProductKind) -> PathBuf {
        self.namespace_root().join(kind.namespace())
    }

    pub(super) fn product_path(&self, kind: PersistentProductKind, cache_id: &str) -> PathBuf {
        self.products_root(kind)
            .join(&cache_id[..2])
            .join(format!("{cache_id}.{PRODUCT_EXTENSION}"))
    }

    pub(super) fn key_lock_path(&self, kind: PersistentProductKind, cache_id: &str) -> PathBuf {
        self.namespace_root()
            .join("locks")
            .join(format!("{}-{cache_id}.lock", kind.namespace()))
    }

    pub(super) fn open_maintenance_lock(&self) -> Result<File> {
        open_private_lock_file(
            &self
                .cache_root()
                .join(".ruby-fast-lsp-derived-products.lock"),
        )
    }
}

pub(super) fn product_kind_for_path(path: &Path) -> PersistentProductKind {
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
        invariant!(
            path.components()
                .any(|component| component.as_os_str() == GEM_PRODUCT_NAMESPACE),
            what = "persistent cleanup found a product outside every registered product namespace",
            why = "cleanup must never assign ownership by guessing",
            fix = "add the product namespace to the explicit kind mapping",
        );
        PersistentProductKind::Gem
    }
}

pub(super) fn validate_cache_id(cache_id: &str) -> Result<()> {
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

pub(super) fn scan_product_entries(namespace_root: &Path) -> Result<Vec<CacheEntry>> {
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
