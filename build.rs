use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[path = "src/test/simulation/build_identity.rs"]
mod simulation_build_identity;

const GEM_FACT_PRODUCER_TREES: &[&str] = &[
    "crates/ruby-analysis/src",
    "crates/rbs-parser/src",
    "crates/rbs-parser/rbs_types",
];
const GEM_FACT_PRODUCER_FILES: &[&str] = &[
    "src/indexer/file_processor.rs",
    "src/runtime/jruby/imports.rs",
    "src/runtime/jruby/java_catalog.rs",
];

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect(
        "INVARIANT VIOLATED: Cargo did not provide CARGO_MANIFEST_DIR. This is a bug because the build script cannot identify semantic producer sources without the package root. Fix: invoke the build through Cargo.",
    ));
    let mut inputs = Vec::new();
    for relative in GEM_FACT_PRODUCER_TREES {
        println!("cargo:rerun-if-changed={relative}");
        collect_regular_files(&manifest_dir.join(relative), &mut inputs);
    }
    for relative in GEM_FACT_PRODUCER_FILES {
        println!("cargo:rerun-if-changed={relative}");
        inputs.push(manifest_dir.join(relative));
    }
    inputs.sort();
    inputs.dedup();

    let mut digest = Sha256::new();
    for absolute in inputs {
        let relative = absolute.strip_prefix(&manifest_dir).expect(
            "INVARIANT VIOLATED: a gem fact producer input escaped the package root. This is a bug because cache identity must be reproducible across checkouts. Fix: list only workspace-contained producer sources.",
        );
        let normalized = relative.to_str().expect(
            "INVARIANT VIOLATED: a gem fact producer path is not UTF-8. This is a bug because cache identity must be portable across supported platforms. Fix: use UTF-8 source paths.",
        ).replace('\\', "/");
        let content = fs::read(&absolute).unwrap_or_else(|error| {
            panic!(
                "INVARIANT VIOLATED: failed to read gem fact producer source {}: {error}. This is a bug because a partially hashed producer could reuse semantically stale facts. Fix: restore the declared source input or update build.rs.",
                absolute.display()
            )
        });
        hash_field(&mut digest, normalized.as_bytes());
        hash_field(&mut digest, &content);
    }
    let source_sha256: [u8; 32] = digest.finalize().into();
    let generated =
        format!("const GEM_FACT_PRODUCER_SOURCE_SHA256: [u8; 32] = {source_sha256:?};\n");
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect(
        "INVARIANT VIOLATED: Cargo did not provide OUT_DIR. This is a bug because the build script cannot publish the semantic producer identity. Fix: invoke the build through Cargo.",
    ));
    fs::write(out_dir.join("gem_fact_producer_identity.rs"), generated).expect(
        "INVARIANT VIOLATED: failed to write the gem fact producer identity. This is a bug because the binary must embed the exact analyzer source identity. Fix: make Cargo's OUT_DIR writable.",
    );

    write_simulation_build_identity(&manifest_dir, &out_dir);
}

fn write_simulation_build_identity(manifest_dir: &Path, out_dir: &Path) {
    use simulation_build_identity::hashing::{manifest_sha256, reader_sha256};

    let mut inputs = Vec::new();
    for relative in [
        "Cargo.toml",
        "Cargo.lock",
        "rust-toolchain.toml",
        "build.rs",
        "support/simulation/oracle_cases.json",
        "src",
        "crates",
    ] {
        println!("cargo:rerun-if-changed={relative}");
        collect_simulation_inputs(&manifest_dir.join(relative), &mut inputs);
    }
    // Only Rust workspace extension packages participate. Editor assets and
    // packaged runtime inputs outside these roots are intentionally out of scope.
    let extensions = manifest_dir.join("extensions");
    println!("cargo:rerun-if-changed=extensions");
    for entry in fs::read_dir(&extensions).expect(
        "INVARIANT VIOLATED: the simulation build cannot enumerate extensions. This is a bug because Rust workspace extension packages must participate in replay identity. Fix: restore the workspace extensions directory.",
    ) {
        let entry = entry.expect(
            "INVARIANT VIOLATED: an extension directory entry cannot be read. This is a bug because replay identity cannot omit an unreadable workspace package. Fix: repair extensions directory permissions.",
        );
        let package = entry.path();
        if excluded_simulation_input(&package) {
            continue;
        }
        let file_type = entry.file_type().expect(
            "INVARIANT VIOLATED: an extension entry cannot be classified. This is a bug because replay identity must distinguish package directories from ordinary files. Fix: restore readable metadata for the extension entry.",
        );
        assert!(
            !file_type.is_symlink(),
            "INVARIANT VIOLATED: simulation extension input {} is a symlink. This is a bug because an external link target makes the declared workspace source scope ambiguous. Fix: keep workspace extension packages as regular directories.",
            package.display()
        );
        if file_type.is_dir() && package.join("Cargo.toml").exists() {
            collect_simulation_inputs(&package.join("Cargo.toml"), &mut inputs);
            collect_simulation_inputs(&package.join("src"), &mut inputs);
        }
    }
    inputs.sort();
    inputs.dedup();
    let mut records = inputs
        .iter()
        .map(|absolute| {
            let relative = absolute.strip_prefix(manifest_dir).expect(
                "INVARIANT VIOLATED: a simulation source input escaped the workspace. This is a bug because replay source identity must remain checkout independent. Fix: collect only regular files under the declared workspace roots.",
            );
            let normalized = relative
                .to_str()
                .expect(
                    "INVARIANT VIOLATED: a simulation source path is not UTF-8. This is a bug because replay manifests require portable path strings. Fix: use UTF-8 workspace source paths.",
                )
                .replace('\\', "/");
            println!("cargo:rerun-if-changed={normalized}");
            let file = fs::File::open(absolute).unwrap_or_else(|error| {
                panic!(
                    "INVARIANT VIOLATED: simulation source {} cannot be opened: {error}. This is a bug because a partial manifest cannot identify the tested source. Fix: restore readable access to every declared workspace source.",
                    absolute.display()
                )
            });
            let sha256 = reader_sha256(file).unwrap_or_else(|error| {
                panic!(
                    "INVARIANT VIOLATED: simulation source {} cannot be hashed: {error}. This is a bug because replay identity cannot contain a partial source digest. Fix: restore readable access to the complete workspace file.",
                    absolute.display()
                )
            });
            (normalized, sha256)
        })
        .collect::<Vec<_>>();
    // PathBuf orders path components, while portable manifest paths use one
    // normalized string order. Publish the same order on every host platform.
    records.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    let source_sha256 = manifest_sha256(
        records
            .iter()
            .map(|(path, sha256)| (path.as_str(), sha256.as_str())),
    );

    let rustc = env::var_os("RUSTC").expect(
        "INVARIANT VIOLATED: Cargo did not identify the Rust compiler. This is a bug because simulation build metadata must name the compiler actually selected for this build. Fix: invoke the build through Cargo.",
    );
    let rustc_output = Command::new(rustc).arg("--version").output().expect(
        "INVARIANT VIOLATED: the selected Rust compiler did not report its version. This is a bug because replay metadata cannot guess the compiler identity. Fix: restore the Cargo-selected compiler executable.",
    );
    assert!(
        rustc_output.status.success(),
        "INVARIANT VIOLATED: the selected Rust compiler version probe failed. This is a bug because replay metadata requires the actual compiler version. Fix: repair the Cargo-selected compiler before building."
    );
    let rustc_version = String::from_utf8(rustc_output.stdout).expect(
        "INVARIANT VIOLATED: the selected Rust compiler version is not UTF-8. This is a bug because replay metadata requires portable compiler text. Fix: use a compiler whose version output follows the Rust CLI contract.",
    );
    let mut feature_names = env::vars_os()
        .filter_map(|(name, _)| {
            let name = name.to_str()?;
            name.starts_with("CARGO_FEATURE_").then(|| name.to_owned())
        })
        .collect::<Vec<_>>();
    feature_names.sort();
    let profile = simulation_cargo_metadata("PROFILE");
    let target = simulation_cargo_metadata("TARGET");
    let rustflags = simulation_cargo_metadata("CARGO_ENCODED_RUSTFLAGS");
    let source_scope = "Rust workspace sources: root Cargo.toml, Cargo.lock, rust-toolchain.toml, build.rs; all regular files under src and crates; Cargo.toml and src under immediate extensions packages; support/simulation/oracle_cases.json. Excludes target, .git, node_modules, .codex, pages, editor assets, external runtime assets and private corpora. This manifest does not claim external build inputs or editor/platform coverage; test_executable_sha256 identifies the actual compiled test artifact.";
    let mut generated = format!(
        "pub(crate) const SIMULATION_SOURCE_SCOPE: &str = {source_scope:?};\npub(crate) const SIMULATION_SOURCE_SHA256: &str = {source_sha256:?};\npub(crate) const SIMULATION_PROFILE: &str = {profile:?};\npub(crate) const SIMULATION_TARGET: &str = {target:?};\npub(crate) const SIMULATION_RUSTC_VERSION: &str = {:?};\npub(crate) const SIMULATION_CARGO_FEATURE_NAMES: &[&str] = &{feature_names:?};\npub(crate) const SIMULATION_CARGO_ENCODED_RUSTFLAGS: &str = {rustflags:?};\npub(crate) const SIMULATION_SOURCE_FILES: &[(&str, &str)] = &[\n",
        rustc_version.trim()
    );
    for (path, sha256) in records {
        generated.push_str(&format!("    ({path:?}, {sha256:?}),\n"));
    }
    generated.push_str("];\n");
    fs::write(out_dir.join("simulation_build_identity.rs"), generated).expect(
        "INVARIANT VIOLATED: simulation build metadata cannot be written. This is a bug because replay artifacts must embed the source manifest captured during compilation. Fix: restore writable access to Cargo's OUT_DIR.",
    );
}

fn simulation_cargo_metadata(name: &str) -> String {
    println!("cargo:rerun-if-env-changed={name}");
    env::var(name).unwrap_or_else(|error| {
        panic!(
            "INVARIANT VIOLATED: Cargo metadata {name} is unavailable: {error}. This is a bug because simulation replay metadata must preserve the actual build configuration. Fix: invoke this build through Cargo with UTF-8 build metadata."
        )
    })
}

fn collect_simulation_inputs(path: &Path, output: &mut Vec<PathBuf>) {
    if excluded_simulation_input(path) {
        return;
    }
    let metadata = fs::symlink_metadata(path).unwrap_or_else(|error| {
        panic!(
            "INVARIANT VIOLATED: simulation source input {} cannot be inspected: {error}. This is a bug because replay identity cannot silently omit a declared input. Fix: restore the source path or update the declared identity scope.",
            path.display()
        )
    });
    assert!(
        !metadata.file_type().is_symlink(),
        "INVARIANT VIOLATED: simulation source input {} is a symlink. This is a bug because checkout-dependent link targets make replay identity ambiguous. Fix: keep declared source inputs as regular workspace files.",
        path.display()
    );
    if metadata.is_dir() {
        for entry in fs::read_dir(path).unwrap_or_else(|error| {
            panic!(
                "INVARIANT VIOLATED: simulation source tree {} cannot be enumerated: {error}. This is a bug because replay identity cannot use a partial directory. Fix: restore readable source directory permissions.",
                path.display()
            )
        }) {
            let entry = entry.expect(
                "INVARIANT VIOLATED: a simulation source directory entry cannot be read. This is a bug because replay identity cannot skip unreadable source entries. Fix: repair source directory permissions.",
            );
            collect_simulation_inputs(&entry.path(), output);
        }
    } else {
        assert!(
            metadata.is_file(),
            "INVARIANT VIOLATED: simulation source input {} is not a regular file. This is a bug because device or socket contents do not identify deterministic source. Fix: keep only regular files in declared workspace source roots.",
            path.display()
        );
        output.push(path.to_path_buf());
    }
}

fn excluded_simulation_input(path: &Path) -> bool {
    path.file_name().is_some_and(|name| {
        matches!(
            name.to_str(),
            Some("target" | ".git" | "node_modules" | ".codex" | "pages")
        )
    })
}

fn collect_regular_files(directory: &Path, output: &mut Vec<PathBuf>) {
    let mut entries = fs::read_dir(directory)
        .unwrap_or_else(|error| {
            panic!(
                "INVARIANT VIOLATED: failed to enumerate gem fact producer tree {}: {error}. This is a bug because cache identity cannot omit analyzer sources. Fix: restore the declared producer tree or update build.rs.",
                directory.display()
            )
        })
        .map(|entry| {
            entry.unwrap_or_else(|error| {
                panic!(
                    "INVARIANT VIOLATED: failed to read an entry under gem fact producer tree {}: {error}. This is a bug because cache identity cannot be computed from a partial source tree. Fix: repair the source tree permissions.",
                    directory.display()
                )
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let file_type = entry.file_type().unwrap_or_else(|error| {
            panic!(
                "INVARIANT VIOLATED: failed to inspect gem fact producer input {}: {error}. This is a bug because cache identity must classify every input deterministically. Fix: repair the source tree entry.",
                entry.path().display()
            )
        });
        assert!(
            !file_type.is_symlink(),
            "INVARIANT VIOLATED: gem fact producer input {} is a symlink. This is a bug because checkout-dependent link targets would make persistent cache identity ambiguous. Fix: keep semantic producer sources as regular workspace files.",
            entry.path().display()
        );
        if file_type.is_dir() {
            collect_regular_files(&entry.path(), output);
        } else if file_type.is_file() {
            output.push(entry.path());
        }
    }
}

fn hash_field(hasher: &mut Sha256, field: &[u8]) {
    hasher.update(
        u64::try_from(field.len())
            .expect(
                "INVARIANT VIOLATED: gem fact producer identity field exceeded u64. This is a bug because one build process cannot hold such a source input. Fix: reject oversized producer inputs before hashing.",
            )
            .to_le_bytes(),
    );
    hasher.update(field);
}
