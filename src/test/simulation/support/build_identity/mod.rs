//! Scoped source and executable identity retained with simulation replays.
//!
//! `build.rs` imports only `hashing`; runtime metadata and the source manifest
//! are compiled only for tests and the opt-in simulation executable.

mod hashing;

pub(super) use runtime::write_build_identity;

mod runtime {
    use super::hashing::{manifest_sha256, reader_sha256};
    use serde::Serialize;
    use std::fs::{self, File};
    use std::path::Path;
    use std::sync::OnceLock;

    pub(super) mod compiled {
        include!(concat!(env!("OUT_DIR"), "/simulation_build_identity.rs"));
    }

    #[derive(Serialize)]
    pub(super) struct SourceRecord {
        path: &'static str,
        sha256: &'static str,
    }

    #[derive(Serialize)]
    pub(super) struct BuildIdentity {
        schema_version: u32,
        source_scope: &'static str,
        package_version: &'static str,
        source_sha256: &'static str,
        source_files: Vec<SourceRecord>,
        profile: &'static str,
        target: &'static str,
        rustc_version: &'static str,
        /// Exact Cargo-provided `CARGO_FEATURE_*` names; no lossy name decoding.
        cargo_feature_names: &'static [&'static str],
        cargo_encoded_rustflags: &'static str,
        test_executable_sha256: &'static str,
    }

    pub(super) fn build_identity() -> BuildIdentity {
        assert_eq!(manifest_sha256(compiled::SIMULATION_SOURCE_FILES.iter().copied()), compiled::SIMULATION_SOURCE_SHA256,
            "INVARIANT VIOLATED: compiled simulation manifest and digest disagree. This is a bug because replay evidence must identify the compiled sources. Fix: generate both from the same source inventory.");
        static TEST_EXECUTABLE_SHA256: OnceLock<String> = OnceLock::new();
        let test_executable_sha256 = TEST_EXECUTABLE_SHA256.get_or_init(|| {
            let executable = std::env::current_exe().expect(
                "INVARIANT VIOLATED: the running simulation test executable cannot be located. This is a bug because replay identity must include the exact compiled artifact. Fix: restore access to the current executable before running simulations.",
            );
            let file = File::open(&executable).unwrap_or_else(|error| {
                panic!(
                    "INVARIANT VIOLATED: the running simulation executable {} cannot be opened: {error}. This is a bug because source metadata alone cannot identify the actual compiled artifact. Fix: retain the test executable until the simulation exits.",
                    executable.display()
                )
            });
            reader_sha256(file).unwrap_or_else(|error| {
                panic!(
                    "INVARIANT VIOLATED: the running simulation executable {} cannot be hashed: {error}. This is a bug because replay evidence cannot use a partial binary identity. Fix: restore readable access to the complete test executable.",
                    executable.display()
                )
            })
        });
        BuildIdentity {
            schema_version: 1,
            source_scope: compiled::SIMULATION_SOURCE_SCOPE,
            package_version: env!("CARGO_PKG_VERSION"),
            source_sha256: compiled::SIMULATION_SOURCE_SHA256,
            source_files: compiled::SIMULATION_SOURCE_FILES
                .iter()
                .map(|(path, sha256)| SourceRecord { path, sha256 })
                .collect(),
            profile: compiled::SIMULATION_PROFILE,
            target: compiled::SIMULATION_TARGET,
            rustc_version: compiled::SIMULATION_RUSTC_VERSION,
            cargo_feature_names: compiled::SIMULATION_CARGO_FEATURE_NAMES,
            cargo_encoded_rustflags: compiled::SIMULATION_CARGO_ENCODED_RUSTFLAGS,
            test_executable_sha256,
        }
    }

    pub(crate) fn write_build_identity(artifact: &Path) {
        let json = serde_json::to_vec_pretty(&build_identity()).expect(
            "INVARIANT VIOLATED: the simulation build identity cannot be serialized. This is a bug because every replay must retain complete build metadata. Fix: keep the build identity serializable.",
        );
        let path = artifact.join("build.json");
        fs::write(&path, json).unwrap_or_else(|error| {
            panic!(
                "INVARIANT VIOLATED: simulation build identity {} cannot be written: {error}. This is a bug because a seed alone cannot identify the tested implementation. Fix: restore write access to the replay artifact directory.",
                path.display()
            )
        });
    }
}

#[cfg(test)]
mod tests {
    use super::hashing::{fields_sha256, manifest_sha256, reader_sha256};
    use super::runtime::{build_identity, compiled};
    use crate::test::simulation::seeded::{seeded_script, write_seed_artifact};
    use sha2::{Digest, Sha256};
    use std::io::{self, Read};

    fn assert_sha256(value: &str) {
        assert!(
            value.len() == 64
                && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "INVARIANT VIOLATED: simulation identity is not a lowercase SHA-256: {value}. This is a bug because replay identities must use one comparable encoding. Fix: retain all 32 digest bytes as lowercase hexadecimal."
        );
    }

    #[test]
    fn streaming_sha256_handles_short_reads_and_buffer_boundaries() {
        struct ShortReader<'a>(&'a [u8]);
        impl Read for ShortReader<'_> {
            fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
                let length = self.0.len().min(output.len()).min(7);
                output[..length].copy_from_slice(&self.0[..length]);
                self.0 = &self.0[length..];
                Ok(length)
            }
        }

        assert_eq!(
            reader_sha256(ShortReader(b"abc")).expect("hash the known SHA-256 input"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let bytes = vec![0xa5; 131_073];
        assert_eq!(
            reader_sha256(ShortReader(&bytes)).expect("hash a multi-buffer source"),
            format!("{:x}", Sha256::digest(&bytes))
        );
        assert_eq!(
            reader_sha256(io::empty()).expect("hash an empty source"),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn manifest_identity_sorts_paths_and_detects_path_or_content_changes() {
        let first = reader_sha256(&b"first source"[..]).expect("hash first fixture source");
        let second = reader_sha256(&b"second source"[..]).expect("hash second fixture source");
        let changed = reader_sha256(&b"changed source"[..]).expect("hash changed fixture source");
        let expected =
            manifest_sha256([("src/a.rs", first.as_str()), ("src/b.rs", second.as_str())]);
        assert_eq!(
            expected,
            manifest_sha256([("src/b.rs", second.as_str()), ("src/a.rs", first.as_str())])
        );
        assert_ne!(
            expected,
            manifest_sha256([("src/c.rs", first.as_str()), ("src/b.rs", second.as_str())])
        );
        assert_ne!(
            expected,
            manifest_sha256([
                ("src/a.rs", changed.as_str()),
                ("src/b.rs", second.as_str())
            ])
        );
    }

    #[test]
    fn length_prefixes_distinguish_ambiguous_field_concatenations() {
        assert_ne!(
            fields_sha256([&b"ab"[..], &b"c"[..]]),
            fields_sha256([&b"a"[..], &b"bc"[..]])
        );
    }

    #[test]
    fn runtime_identity_is_stable_and_has_explicit_json_metadata() {
        let first = serde_json::to_value(build_identity()).expect("serialize first build identity");
        let second =
            serde_json::to_value(build_identity()).expect("serialize repeated build identity");
        assert_eq!(first, second);
        assert_eq!(first["schema_version"], 1);
        assert_eq!(first["package_version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(first["source_scope"], compiled::SIMULATION_SOURCE_SCOPE);
        assert_eq!(first["source_sha256"], compiled::SIMULATION_SOURCE_SHA256);
        assert_sha256(
            first["source_sha256"]
                .as_str()
                .expect("source digest string"),
        );
        assert_sha256(
            first["test_executable_sha256"]
                .as_str()
                .expect("executable digest string"),
        );
        assert!(!first["profile"]
            .as_str()
            .expect("Cargo profile string")
            .is_empty());
        assert!(!first["target"]
            .as_str()
            .expect("Cargo target string")
            .is_empty());
        assert!(first["rustc_version"]
            .as_str()
            .expect("rustc version string")
            .starts_with("rustc "));
        assert!(first["cargo_feature_names"].is_array());
        assert!(first["cargo_encoded_rustflags"].is_string());
        let source_files = first["source_files"]
            .as_array()
            .expect("ordered source manifest records");
        assert_eq!(source_files.len(), compiled::SIMULATION_SOURCE_FILES.len());
        for (record, (path, sha256)) in source_files.iter().zip(compiled::SIMULATION_SOURCE_FILES) {
            assert_eq!(record["path"], *path);
            assert_eq!(record["sha256"], *sha256);
        }
        assert_eq!(
            manifest_sha256(compiled::SIMULATION_SOURCE_FILES.iter().copied()),
            compiled::SIMULATION_SOURCE_SHA256
        );
    }

    #[test]
    fn compiled_manifest_covers_workspace_sources_and_excludes_outside_scope() {
        let files = compiled::SIMULATION_SOURCE_FILES;
        for required in [
            "src/test/simulation/support/seeded.rs",
            "src/server.rs",
            "crates/ruby-analysis/src/indexer/fact_collector/nodes/calls/nil_call.rs",
            "Cargo.lock",
            "build.rs",
        ] {
            assert!(
                files.iter().any(|(path, _)| *path == required),
                "manifest omitted {required}"
            );
        }
        assert!(files.windows(2).all(|pair| pair[0].0 < pair[1].0));
        for (path, sha256) in files {
            assert!(
                !path.starts_with('/'),
                "manifest path must be relative: {path}"
            );
            assert!(
                !path.contains('\\'),
                "manifest path must use forward slashes: {path}"
            );
            assert!(
                path.split('/').all(|part| !matches!(
                    part,
                    "target" | ".git" | ".codex" | "pages" | "node_modules"
                )),
                "manifest included an excluded path: {path}"
            );
            assert_sha256(sha256);
        }
    }

    #[test]
    fn seed_artifact_retains_the_exact_compiled_build_identity() {
        let artifact = write_seed_artifact(&seeded_script(42));
        let retained: serde_json::Value = serde_json::from_slice(
            &std::fs::read(artifact.join("build.json"))
                .expect("read build identity from seeded replay"),
        )
        .expect("parse retained build identity");
        assert_eq!(
            retained,
            serde_json::to_value(build_identity()).expect("serialize current compiled identity")
        );
        assert_eq!(
            retained["source_sha256"],
            compiled::SIMULATION_SOURCE_SHA256
        );
        assert!(artifact.join("script.txt").is_file());
        assert!(artifact.join("README.txt").is_file());
        assert!(artifact.join("files").is_dir());
        eprintln!(
            "compiled simulation identity artifact: {}",
            artifact.display()
        );
    }
}
