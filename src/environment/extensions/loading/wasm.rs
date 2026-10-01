use crate::invariant::ExpectInvariant;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use log::warn;

use crate::environment::extensions::loading::config::ExtensionLoadError;
use crate::environment::extensions::loading::manifest::{
    extension_metadata, parse_manifest_applicability, parse_manifest_method_targets,
    parse_manifest_namespace_targets, validate_manifest, validate_manifest_call_names,
    validate_manifest_checksum, wasm_file_stem, ExtensionManifest,
};
use crate::environment::extensions::loading::packages::ExtensionPackage;
use crate::environment::extensions::registry::loaded::LoadedWasmExtension;
use crate::environment::extensions::MAX_EXTENSION_WASM_BYTES;
use crate::indexer::cache::persistent::{
    CompiledWasmProductKey, PersistentCompiledWasmLookup, PersistentDerivedProductCache,
};

pub(super) fn read_manifest(path: &Path) -> Result<ExtensionManifest, ExtensionLoadError> {
    let contents = fs::read_to_string(path).map_err(|err| {
        ExtensionLoadError::new(format!(
            "failed to read extension manifest `{}`: {}",
            path.display(),
            err
        ))
    })?;
    toml::from_str(&contents).map_err(|err| {
        ExtensionLoadError::new(format!(
            "invalid extension manifest `{}`: {}",
            path.display(),
            err
        ))
    })
}

pub(in crate::environment::extensions) fn read_extension_wasm(
    path: &Path,
) -> Result<Arc<[u8]>, ExtensionLoadError> {
    let mut file = fs::File::open(path).map_err(|error| {
        ExtensionLoadError::new(format!(
            "failed to open Wasm extension `{}`: {error}",
            path.display()
        ))
    })?;
    let byte_length = file
        .metadata()
        .map_err(|error| {
            ExtensionLoadError::new(format!(
                "failed to inspect Wasm extension `{}`: {error}",
                path.display()
            ))
        })?
        .len();
    if byte_length > MAX_EXTENSION_WASM_BYTES {
        return Err(ExtensionLoadError::new(format!(
            "Wasm extension `{}` is {byte_length} bytes; maximum is {MAX_EXTENSION_WASM_BYTES}",
            path.display()
        )));
    }
    let capacity = usize::try_from(byte_length).map_err(|_| {
        ExtensionLoadError::new(format!(
            "Wasm extension `{}` length does not fit this platform",
            path.display()
        ))
    })?;
    let mut bytes = Vec::with_capacity(capacity);
    let read_limit = MAX_EXTENSION_WASM_BYTES.checked_add(1).expect_invariant(
        "Wasm source read limit overflowed u64",
        "the fixed 64 MiB limit must fit u64",
        "keep the source limit below u64::MAX",
    );
    let mut bounded = std::io::Read::take(&mut file, read_limit);
    std::io::Read::read_to_end(&mut bounded, &mut bytes).map_err(|error| {
        ExtensionLoadError::new(format!(
            "failed to read Wasm extension `{}`: {error}",
            path.display()
        ))
    })?;
    let actual_length = u64::try_from(bytes.len()).map_err(|_| {
        ExtensionLoadError::new(format!(
            "Wasm extension `{}` read length does not fit u64",
            path.display()
        ))
    })?;
    if actual_length > MAX_EXTENSION_WASM_BYTES {
        return Err(ExtensionLoadError::new(format!(
            "Wasm extension `{}` grew to {actual_length} bytes while reading; maximum is {MAX_EXTENSION_WASM_BYTES}",
            path.display()
        )));
    }
    Ok(Arc::from(bytes))
}

pub(super) fn manifest_wasm_path(
    package_dir: &Path,
    manifest: &ExtensionManifest,
) -> Result<PathBuf, ExtensionLoadError> {
    let relative_path = manifest
        .wasm
        .as_ref()
        .or_else(|| manifest.build.as_ref().map(|build| &build.output))
        .ok_or_else(|| {
            ExtensionLoadError::new(format!(
                "extension `{}` manifest has no `wasm` or `build.output` path",
                manifest.id
            ))
        })?;
    let wasm_path = package_dir.join(relative_path);
    if !wasm_path.is_file() {
        return Err(ExtensionLoadError::new(format!(
            "extension `{}` wasm path `{}` does not exist",
            manifest.id,
            wasm_path.display()
        )));
    }
    Ok(wasm_path)
}

pub(in crate::environment::extensions) fn load_wasm_extension(
    package: ExtensionPackage,
) -> Result<Arc<LoadedWasmExtension>, ExtensionLoadError> {
    load_wasm_extension_with_cache(package, None)
}

pub(super) fn load_wasm_extension_with_cache(
    package: ExtensionPackage,
    persistent_cache: Option<&PersistentDerivedProductCache>,
) -> Result<Arc<LoadedWasmExtension>, ExtensionLoadError> {
    let id = package
        .manifest
        .as_ref()
        .map(|manifest| manifest.id.clone())
        .unwrap_or_else(|| wasm_file_stem(&package.wasm_path));

    let wasm_bytes = package.wasm_bytes.as_ref();

    if let Some(manifest) = &package.manifest {
        validate_manifest(manifest)?;
        validate_manifest_checksum(manifest, wasm_bytes)?;
    }
    let metadata = extension_metadata(&id, package.manifest.as_ref());

    let compiled_extension =
        load_compiled_wasm_extension(&id, &package.wasm_path, wasm_bytes, persistent_cache)?;
    let mut extension = ruby_fast_lsp_extension_wasm_host::WasmExtension::from_compiled(
        id.clone(),
        compiled_extension.clone(),
    )
    .map_err(|err| {
        ExtensionLoadError::new(format!(
            "failed to instantiate Wasm extension `{}` from `{}`: {}",
            id,
            package.wasm_path.display(),
            err
        ))
    })?;

    let abi_version = extension.abi_version().map_err(|err| {
        ExtensionLoadError::new(format!(
            "Wasm extension `{}` ABI check failed: {}",
            extension.id(),
            err
        ))
    })?;
    if abi_version != ruby_fast_lsp_extension_api::ABI_VERSION {
        return Err(ExtensionLoadError::new(format!(
            "Wasm extension `{}` ABI version {} != host ABI version {}",
            extension.id(),
            abi_version,
            ruby_fast_lsp_extension_api::ABI_VERSION
        )));
    }

    if let Some(manifest) = &package.manifest {
        validate_manifest_call_names(manifest, extension.indexed_call_names())?;
    }
    let semantic_targets = package
        .manifest
        .as_ref()
        .map(parse_manifest_method_targets)
        .transpose()?
        .unwrap_or_default();
    let semantic_namespaces = package
        .manifest
        .as_ref()
        .map(parse_manifest_namespace_targets)
        .transpose()?
        .unwrap_or_default();
    let frame_call_names = package
        .manifest
        .as_ref()
        .and_then(|manifest| manifest.indexing.as_ref())
        .map(|indexing| indexing.frame_call_names.iter().cloned().collect())
        .unwrap_or_default();
    let applicability = package
        .manifest
        .as_ref()
        .map(parse_manifest_applicability)
        .transpose()?
        .unwrap_or_default();
    let project_context_delivery = package
        .manifest
        .as_ref()
        .and_then(|manifest| manifest.indexing.as_ref())
        .map(|indexing| indexing.project_context)
        .unwrap_or_default();

    Ok(Arc::new(LoadedWasmExtension::new(
        metadata,
        extension,
        compiled_extension,
        semantic_targets,
        semantic_namespaces,
        frame_call_names,
        applicability,
        project_context_delivery,
    )))
}

fn load_compiled_wasm_extension(
    id: &str,
    wasm_path: &Path,
    wasm_bytes: &[u8],
    persistent_cache: Option<&PersistentDerivedProductCache>,
) -> Result<ruby_fast_lsp_extension_wasm_host::CompiledWasmExtension, ExtensionLoadError> {
    let compiler =
        ruby_fast_lsp_extension_wasm_host::WasmExtensionCompiler::new().map_err(|error| {
            ExtensionLoadError::new(format!(
                "failed to prepare Wasm extension compiler for `{id}` from `{}`: {error}",
                wasm_path.display()
            ))
        })?;
    let key = CompiledWasmProductKey::new(wasm_bytes, compiler.cache_identity());
    if let Some(cache) = persistent_cache {
        for attempt in 0..2 {
            match cache.lookup_compiled_wasm_or_reserve(&key) {
                Ok(PersistentCompiledWasmLookup::Hit(serialized)) => {
                    // SAFETY: the persistent cache verifies its private
                    // envelope checksum, embedded source digest, compiler
                    // identity, artifact length, and artifact checksum before
                    // returning these bytes.
                    match unsafe { compiler.deserialize_verified(serialized.as_slice()) } {
                        Ok(compiled) => return Ok(compiled),
                        Err(error) if attempt == 0 => {
                            warn!(
                                "Rejecting incompatible compiled Wasm cache product for `{id}`: {error:#}"
                            );
                            if let Err(invalidation_error) = cache.invalidate_compiled_wasm(&key) {
                                warn!(
                                    "Failed to invalidate compiled Wasm cache product for `{id}`; compiling without persistence: {invalidation_error:#}"
                                );
                                break;
                            }
                        }
                        Err(error) => {
                            warn!(
                                "Replacement compiled Wasm cache product for `{id}` was still invalid; compiling without persistence: {error:#}"
                            );
                            break;
                        }
                    }
                }
                Ok(PersistentCompiledWasmLookup::Reservation(reservation)) => {
                    let compiled = compiler.compile(wasm_bytes).map_err(|error| {
                        ExtensionLoadError::new(format!(
                            "failed to compile Wasm extension `{id}` from `{}`: {error}",
                            wasm_path.display()
                        ))
                    })?;
                    match compiled.serialize() {
                        Ok(serialized) => {
                            if let Err(error) = reservation.publish(&key, &serialized) {
                                warn!(
                                    "Failed to publish compiled Wasm cache product for `{id}`; using the valid in-memory module: {error:#}"
                                );
                            }
                        }
                        Err(error) => warn!(
                            "Failed to serialize compiled Wasm extension `{id}`; using the valid in-memory module: {error:#}"
                        ),
                    }
                    return Ok(compiled);
                }
                Err(error) => {
                    warn!(
                        "Compiled Wasm cache lookup failed for `{id}`; compiling without persistence: {error:#}"
                    );
                    break;
                }
            }
        }
    }

    compiler.compile(wasm_bytes).map_err(|error| {
        ExtensionLoadError::new(format!(
            "failed to compile Wasm extension `{id}` from `{}`: {error}",
            wasm_path.display()
        ))
    })
}

pub(in crate::environment::extensions) fn require_empty_lifecycle_output(
    output: ruby_fast_lsp_extension_api::ExtensionOutput,
) -> anyhow::Result<()> {
    if output.index_patches.is_empty()
        && output.execution_contexts.is_empty()
        && output.response_patches.is_empty()
        && output.command_patches.is_empty()
        && output.process_requests.is_empty()
        && output.reindex_files.is_empty()
    {
        return Ok(());
    }
    Err(anyhow::anyhow!(
        "lifecycle callback returned patches or requests"
    ))
}

pub(in crate::environment::extensions) fn lifecycle_output_is_empty(
    result: &anyhow::Result<ruby_fast_lsp_extension_api::ExtensionOutput>,
) -> bool {
    result.as_ref().is_ok_and(|output| {
        output.index_patches.is_empty()
            && output.execution_contexts.is_empty()
            && output.response_patches.is_empty()
            && output.command_patches.is_empty()
            && output.process_requests.is_empty()
            && output.reindex_files.is_empty()
    })
}
