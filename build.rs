use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

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
