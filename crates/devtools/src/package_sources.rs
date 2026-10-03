//! Source fingerprints for prebuilt mruby Wasm extension packages.
//!
//! A package that checks in its built Wasm records, under `[build]`, the
//! `source_sha256` of every input that produced it: the package's Ruby
//! sources, the mruby SDK (Ruby prelude, native shim, build config, patches,
//! vendored gems), and the Docker builder that pins the mruby and wasi-sdk
//! versions. When a source changes without a rebuild, the recorded value no
//! longer matches and the test below fails, so the checked-in Wasm cannot
//! silently lag behind its sources.

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// The SDK directory every mruby package builds against.
const SDK_DIR: &str = "extensions/mruby-sdk";

/// Package files compiled into the Wasm bundle.
const PACKAGE_INPUTS: &[&str] = &["extension.rb", "runtime.rb"];

/// SDK inputs, relative to the SDK directory. Directories are walked in
/// sorted order.
const SDK_INPUTS: &[&str] = &[
    "ruby_fast_lsp_extension.rb",
    "native",
    "build_config",
    "patches",
    "vendor",
    "Dockerfile.build",
    "scripts/build-wasm.sh",
    "scripts/build-wasm-docker.sh",
];

/// The hex SHA-256 over every build input of the mruby package at `package`.
///
/// Each file contributes its label (`package/<path>` or `sdk/<path>`, with
/// `/` separators), a NUL, its byte length, a NUL, and its bytes, so renames
/// and moved bytes change the result.
pub fn mruby_package_source_sha256(repo_root: &Path, package: &Path) -> anyhow::Result<String> {
    let mut files = Vec::new();
    for input in PACKAGE_INPUTS {
        files.push((format!("package/{input}"), package.join(input)));
    }
    let sdk = repo_root.join(SDK_DIR);
    for input in SDK_INPUTS {
        collect(&sdk, &sdk.join(input), "sdk", &mut files)?;
    }

    let mut hasher = Sha256::new();
    for (label, path) in files {
        let bytes = std::fs::read(&path)
            .map_err(|error| anyhow::anyhow!("read build input {}: {error}", path.display()))?;
        hasher.update(label.as_bytes());
        hasher.update([0]);
        hasher.update(bytes.len().to_string().as_bytes());
        hasher.update([0]);
        hasher.update(&bytes);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect(
    base: &Path,
    path: &Path,
    prefix: &str,
    files: &mut Vec<(String, PathBuf)>,
) -> anyhow::Result<()> {
    if !path.exists() {
        anyhow::bail!("missing build input {}", path.display());
    }
    let mut entries = walkdir::WalkDir::new(path)
        .sort_by_file_name()
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    entries.retain(|entry| entry.file_type().is_file());
    for entry in entries {
        let relative = entry
            .path()
            .strip_prefix(base)
            .unwrap_or_else(|_| {
                unreachable_invariant!(
                    what = "walked path {} is outside its base {}",
                    why = "WalkDir yields only descendants of the walked directory",
                    fix = "walk inputs only from directories under the SDK base",
                    entry.path().display(),
                    base.display(),
                )
            })
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/");
        files.push((format!("{prefix}/{relative}"), entry.into_path()));
    }
    Ok(())
}

/// The manifest's top-level string field, or a `[build]` field when
/// `section` is `Some("build")`.
fn manifest_field(manifest: &toml::Table, section: Option<&str>, field: &str) -> Option<String> {
    let table = match section {
        Some(section) => manifest.get(section)?.as_table()?,
        None => manifest,
    };
    table.get(field)?.as_str().map(str::to_string)
}

/// Every package under `extensions/` that checks in a prebuilt mruby Wasm,
/// with its recorded and current source fingerprints.
pub fn prebuilt_mruby_packages(
    repo_root: &Path,
) -> anyhow::Result<Vec<(PathBuf, Option<String>, String)>> {
    let mut packages = Vec::new();
    let mut directories = std::fs::read_dir(repo_root.join("extensions"))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    directories.sort();
    for directory in directories {
        let manifest_path = directory.join("extension.toml");
        let Ok(text) = std::fs::read_to_string(&manifest_path) else {
            continue;
        };
        let manifest = toml::from_str::<toml::Table>(&text)?;
        let is_mruby = manifest_field(&manifest, None, "runtime").as_deref() == Some("mruby-wasm");
        let is_prebuilt = manifest_field(&manifest, None, "checksum_sha256").is_some();
        if !(is_mruby && is_prebuilt) {
            continue;
        }
        let recorded = manifest_field(&manifest, Some("build"), "source_sha256");
        let current = mruby_package_source_sha256(repo_root, &directory)?;
        packages.push((directory, recorded, current));
    }
    Ok(packages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prebuilt_mruby_wasm_matches_its_recorded_sources() {
        let root = crate::workspace_root();
        let packages = prebuilt_mruby_packages(&root).expect("package manifests must parse");
        assert!(
            !packages.is_empty(),
            "the repository ships at least one prebuilt mruby package"
        );
        for (package, recorded, current) in packages {
            let package = package.strip_prefix(&root).unwrap_or(&package).display();
            assert_eq!(
                recorded.as_deref(),
                Some(current.as_str()),
                "{package}: the sources changed since its Wasm was built. Rebuild it with \
                 `extensions/mruby-sdk/scripts/build-wasm-docker.sh {package}`, update \
                 `checksum_sha256`, then set `[build] source_sha256 = \"{current}\"` \
                 (`cargo run -p devtools --bin extension -- sources {package}` prints it)."
            );
        }
    }

    #[test]
    fn source_fingerprint_changes_with_any_package_input() {
        let root = crate::workspace_root();
        let dir = tempfile::tempdir().expect("temporary package");
        std::fs::write(dir.path().join("extension.rb"), "a").expect("write extension");
        std::fs::write(dir.path().join("runtime.rb"), "b").expect("write runtime");
        let before = mruby_package_source_sha256(&root, dir.path()).expect("fingerprint");
        std::fs::write(dir.path().join("runtime.rb"), "c").expect("rewrite runtime");
        let after = mruby_package_source_sha256(&root, dir.path()).expect("fingerprint");
        assert_ne!(before, after);
    }
}
