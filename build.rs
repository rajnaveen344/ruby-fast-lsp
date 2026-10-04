use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const GEM_FACT_PRODUCER_TREES: &[&str] = &[
    "crates/ruby-analysis/src",
    "crates/rbs-parser/src",
    "crates/rbs-parser/rbs_types",
    "crates/extension-api/src",
    "crates/jruby-support/src",
    "crates/jvm-metadata/src",
    "src/loader/file_processor",
];
const GEM_FACT_PRODUCER_FILES: &[&str] = &[
    "src/environment/runtime/jruby/imports/call_host.rs",
    "src/environment/runtime/jruby/imports/declarations.rs",
    "src/environment/runtime/jruby/imports/java_methods.rs",
    "src/environment/runtime/jruby/imports/java_types.rs",
    "src/environment/runtime/jruby/imports/mod.rs",
    "src/environment/runtime/jruby/imports/navigation.rs",
    "src/environment/runtime/jruby/java_catalog/catalog.rs",
    "src/environment/runtime/jruby/java_catalog/mod.rs",
    "src/environment/runtime/jruby/java_catalog/product.rs",
];

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect(
        "invariant violated: Cargo did not provide CARGO_MANIFEST_DIR — bug: the build script cannot identify semantic producer sources without the package root — fix: invoke the build through Cargo",
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
            "invariant violated: a gem fact producer input escaped the package root — bug: cache identity must be reproducible across checkouts — fix: list only workspace-contained producer sources",
        );
        let normalized = relative.to_str().expect(
            "invariant violated: a gem fact producer path is not UTF-8 — bug: cache identity must be portable across supported platforms — fix: use UTF-8 source paths",
        ).replace('\\', "/");
        let content = fs::read(&absolute).unwrap_or_else(|error| {
            panic!(
                "invariant violated: failed to read gem fact producer source {}: {error} — bug: a partially hashed producer could reuse semantically stale facts — fix: restore the declared source input or update build.rs",
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
        "invariant violated: Cargo did not provide OUT_DIR — bug: the build script cannot publish the semantic producer identity — fix: invoke the build through Cargo",
    ));
    fs::write(out_dir.join("gem_fact_producer_identity.rs"), generated).expect(
        "invariant violated: failed to write the gem fact producer identity — bug: the binary must embed the exact analyzer source identity — fix: make Cargo's OUT_DIR writable",
    );
}

fn collect_regular_files(directory: &Path, output: &mut Vec<PathBuf>) {
    let mut entries = fs::read_dir(directory)
        .unwrap_or_else(|error| {
            panic!(
                "invariant violated: failed to enumerate gem fact producer tree {}: {error} — bug: cache identity cannot omit analyzer sources — fix: restore the declared producer tree or update build.rs",
                directory.display()
            )
        })
        .map(|entry| {
            entry.unwrap_or_else(|error| {
                panic!(
                    "invariant violated: failed to read an entry under gem fact producer tree {}: {error} — bug: cache identity cannot be computed from a partial source tree — fix: repair the source tree permissions",
                    directory.display()
                )
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let file_type = entry.file_type().unwrap_or_else(|error| {
            panic!(
                "invariant violated: failed to inspect gem fact producer input {}: {error} — bug: cache identity must classify every input deterministically — fix: repair the source tree entry",
                entry.path().display()
            )
        });
        assert!(
            !file_type.is_symlink(),
            "invariant violated: gem fact producer input {} is a symlink — bug: checkout-dependent link targets would make persistent cache identity ambiguous — fix: keep semantic producer sources as regular workspace files",
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
                "invariant violated: gem fact producer identity field exceeded u64 — bug: one build process cannot hold such a source input — fix: reject oversized producer inputs before hashing",
            )
            .to_le_bytes(),
    );
    hasher.update(field);
}
