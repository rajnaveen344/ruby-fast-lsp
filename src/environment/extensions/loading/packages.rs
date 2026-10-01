use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use log::warn;
use sha2::{Digest, Sha256};

use crate::environment::extensions::loading::config::{
    ConfiguredExtensionPath, ExtensionLoadConfig, ExtensionLoadError, ExtensionPathSource,
};
use crate::environment::extensions::loading::manifest::ExtensionManifest;
use crate::environment::extensions::loading::wasm::{
    load_wasm_extension_with_cache, manifest_wasm_path, read_extension_wasm, read_manifest,
};
use crate::environment::extensions::registry::loaded::LoadedWasmExtension;
use crate::indexer::cache::persistent::PersistentDerivedProductCache;

pub(in crate::environment::extensions) fn discover_extension_packages(
    config: &ExtensionLoadConfig,
) -> Vec<ExtensionPackage> {
    let mut packages = Vec::new();
    for configured_path in &config.package_paths {
        if let Err(err) = collect_extension_package(configured_path, true, &mut packages) {
            warn!("Skipping Ruby Fast LSP extension package: {}", err);
        }
    }
    for configured_path in &config.directory_paths {
        if let Err(err) = collect_extension_directory(configured_path, &mut packages) {
            warn!("Skipping Ruby Fast LSP extension directory: {}", err);
        }
    }
    for configured_path in &config.project_package_paths {
        if let Err(err) = collect_extension_package(configured_path, false, &mut packages) {
            warn!(
                "Skipping project-local Ruby Fast LSP extension package: {}",
                err
            );
        }
    }
    packages.sort_by(|left, right| {
        extension_package_priority(left)
            .cmp(&extension_package_priority(right))
            .then_with(|| left.wasm_path.cmp(&right.wasm_path))
    });
    packages.dedup_by(|left, right| left.wasm_path == right.wasm_path);
    packages
}

pub(in crate::environment::extensions) fn extension_packages_fingerprint(
    packages: &[ExtensionPackage],
) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"ruby-fast-lsp-extension-discovery-v1\0");
    for package in packages {
        digest.update([match package.source {
            ExtensionPathSource::Environment => 0,
            ExtensionPathSource::ProjectLocal => 1,
            ExtensionPathSource::InitializationOptions => 2,
        }]);
        digest.update([u8::from(package.explicit_package)]);
        let path = package.wasm_path.to_string_lossy();
        digest.update((path.len() as u64).to_le_bytes());
        digest.update(path.as_bytes());
        let manifest = format!("{:?}", package.manifest);
        digest.update((manifest.len() as u64).to_le_bytes());
        digest.update(manifest.as_bytes());
        digest.update((package.wasm_bytes.len() as u64).to_le_bytes());
        digest.update(package.wasm_bytes.as_ref());
    }
    digest.finalize().into()
}

#[cfg(test)]
pub(in crate::environment::extensions) fn load_wasm_extensions(
    config: &ExtensionLoadConfig,
) -> Vec<Arc<LoadedWasmExtension>> {
    load_wasm_extensions_from_packages(discover_extension_packages(config))
}

#[cfg(test)]
fn load_wasm_extensions_from_packages(
    packages: Vec<ExtensionPackage>,
) -> Vec<Arc<LoadedWasmExtension>> {
    load_wasm_extensions_from_packages_with_cache(packages, None)
}

pub(in crate::environment::extensions) fn load_wasm_extensions_from_packages_with_cache(
    packages: Vec<ExtensionPackage>,
    persistent_cache: Option<&PersistentDerivedProductCache>,
) -> Vec<Arc<LoadedWasmExtension>> {
    let mut extension_ids = BTreeSet::new();
    packages
        .into_iter()
        .filter_map(|package| match load_wasm_extension_with_cache(package, persistent_cache) {
            Ok(extension) if extension_ids.insert(extension.metadata.id.clone()) => Some(extension),
            Ok(extension) => {
                warn!(
                    "Skipping duplicate Ruby Fast LSP extension id `{}` from lower-priority package",
                    extension.metadata.id
                );
                None
            }
            Err(err) => {
                warn!("Skipping Ruby Fast LSP extension: {}", err);
                None
            }
        })
        .collect::<Vec<_>>()
}

#[derive(Debug)]
pub(in crate::environment::extensions) struct ExtensionPackage {
    pub(super) wasm_path: PathBuf,
    pub(super) wasm_bytes: Arc<[u8]>,
    pub(super) manifest: Option<ExtensionManifest>,
    source: ExtensionPathSource,
    explicit_package: bool,
}

fn extension_package_priority(package: &ExtensionPackage) -> (u8, u8) {
    let source = match package.source {
        ExtensionPathSource::InitializationOptions => 0,
        ExtensionPathSource::ProjectLocal => 1,
        ExtensionPathSource::Environment => 2,
    };
    let discovery = if package.explicit_package { 0 } else { 1 };
    (source, discovery)
}

pub(in crate::environment::extensions) fn collect_extension_package(
    configured_path: &ConfiguredExtensionPath,
    explicit_package: bool,
    output: &mut Vec<ExtensionPackage>,
) -> Result<(), ExtensionLoadError> {
    let path = &configured_path.path;
    if path.is_file() {
        if path.extension().and_then(|ext| ext.to_str()) != Some("wasm") {
            return Err(ExtensionLoadError::new(format!(
                "extension path `{}` is not a .wasm file or package directory",
                path.display()
            )));
        }
        if configured_path.source == ExtensionPathSource::InitializationOptions {
            return Err(ExtensionLoadError::new(format!(
                "direct wasm path `{}` is not allowed from initialization options; use a package directory with extension.toml",
                path.display()
            )));
        }
        output.push(ExtensionPackage {
            wasm_path: path.to_path_buf(),
            wasm_bytes: read_extension_wasm(path)?,
            manifest: None,
            source: configured_path.source,
            explicit_package,
        });
        return Ok(());
    }

    if !path.is_dir() {
        return Err(ExtensionLoadError::new(format!(
            "extension path `{}` is neither a file nor directory",
            path.display()
        )));
    }

    let manifest_path = path.join("extension.toml");
    if manifest_path.exists() {
        let manifest = read_manifest(&manifest_path)?;
        let wasm_path = manifest_wasm_path(path, &manifest)?;
        let wasm_bytes = read_extension_wasm(&wasm_path)?;
        output.push(ExtensionPackage {
            wasm_path,
            wasm_bytes,
            manifest: Some(manifest),
            source: configured_path.source,
            explicit_package,
        });
        return Ok(());
    }

    if configured_path.source == ExtensionPathSource::InitializationOptions {
        return Err(ExtensionLoadError::new(format!(
            "extension package `{}` has no extension.toml",
            path.display()
        )));
    }

    collect_extension_directory(configured_path, output)
}

fn collect_extension_directory(
    configured_path: &ConfiguredExtensionPath,
    output: &mut Vec<ExtensionPackage>,
) -> Result<(), ExtensionLoadError> {
    let path = &configured_path.path;
    if !path.is_dir() {
        return Err(ExtensionLoadError::new(format!(
            "extension directory `{}` is not a directory",
            path.display()
        )));
    }

    for entry in fs::read_dir(path).map_err(|err| {
        ExtensionLoadError::new(format!(
            "failed to read extension directory `{}`: {}",
            path.display(),
            err
        ))
    })? {
        let entry = entry.map_err(|err| {
            ExtensionLoadError::new(format!(
                "failed to read extension directory entry in `{}`: {}",
                path.display(),
                err
            ))
        })?;
        let entry_path = entry.path();
        if entry_path.is_dir() && entry_path.join("extension.toml").exists() {
            let entry_path = ConfiguredExtensionPath {
                path: entry_path,
                source: configured_path.source,
            };
            if let Err(err) = collect_extension_package(&entry_path, false, output) {
                warn!("Skipping Ruby Fast LSP extension package: {}", err);
            }
        } else if entry_path.extension().and_then(|ext| ext.to_str()) == Some("wasm") {
            if configured_path.source == ExtensionPathSource::InitializationOptions {
                warn!(
                    "Skipping direct wasm extension `{}` from initialization options; use a package directory with extension.toml",
                    entry_path.display()
                );
                continue;
            }
            match read_extension_wasm(&entry_path) {
                Ok(wasm_bytes) => output.push(ExtensionPackage {
                    wasm_path: entry_path,
                    wasm_bytes,
                    manifest: None,
                    source: configured_path.source,
                    explicit_package: false,
                }),
                Err(error) => warn!("Skipping unreadable Wasm extension: {error}"),
            }
        }
    }
    Ok(())
}
