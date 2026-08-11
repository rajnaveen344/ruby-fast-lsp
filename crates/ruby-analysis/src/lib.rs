//! Unified Ruby analysis API.

pub mod core;
pub mod engine;
pub mod indexer;
pub mod inference;

pub use core::*;
pub use engine::*;
pub use indexer::*;
pub use inference::{control_flow, r#type, rbs, type_tracker};

#[cfg(test)]
mod architecture_tests {
    use std::path::Path;

    #[test]
    fn reusable_analysis_crate_has_no_editor_protocol_dependency() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let protocol_crate = ["tower", "-lsp"].concat();
        let manifest = std::fs::read_to_string(manifest_dir.join("Cargo.toml")).unwrap_or_else(
            |error| {
                panic!(
                    "INVARIANT VIOLATED: ruby-analysis Cargo.toml could not be read: {error}. This is a bug because the architecture boundary test must inspect the crate's direct dependencies. Fix: restore the crate manifest before running tests."
                )
            },
        );
        assert!(
            !manifest.contains(&protocol_crate),
            "INVARIANT VIOLATED: ruby-analysis directly depends on the editor protocol crate. This is a bug because the reusable checker and LSP must share analysis without an LSP data model. Fix: replace protocol coordinates and response records with domain byte ranges and adapter-owned projection."
        );

        let protocol_module = ["tower", "_lsp"].concat();
        let protocol_types = ["lsp", "_types"].concat();
        let mut pending = vec![manifest_dir.join("src")];
        while let Some(directory) = pending.pop() {
            let entries = std::fs::read_dir(&directory).unwrap_or_else(|error| {
                panic!(
                    "INVARIANT VIOLATED: ruby-analysis source directory `{}` could not be read: {error}. This is a bug because skipping a directory could hide an editor-protocol dependency. Fix: restore a readable source tree or deliberately update the architecture boundary root.",
                    directory.display(),
                )
            });
            for entry in entries {
                let entry = entry.unwrap_or_else(|error| {
                    panic!(
                        "INVARIANT VIOLATED: a ruby-analysis source entry could not be read: {error}. This is a bug because the boundary audit must inspect every Rust module. Fix: repair the source tree before running architecture tests."
                    )
                });
                let path = entry.path();
                if path.is_dir() {
                    pending.push(path);
                    continue;
                }
                if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                    continue;
                }
                let source = std::fs::read_to_string(&path).unwrap_or_else(|error| {
                    panic!(
                        "INVARIANT VIOLATED: ruby-analysis source `{}` could not be decoded as UTF-8: {error}. This is a bug because Rust source must be readable by the architecture audit. Fix: restore valid Rust source text.",
                        path.display(),
                    )
                });
                assert!(
                    !source.contains(&protocol_module) && !source.contains(&protocol_types),
                    "INVARIANT VIOLATED: ruby-analysis source `{}` imports editor protocol types. This is a bug because reusable analysis must expose SourceFileId, TextRange, byte offsets, and domain records. Fix: move protocol projection to the root adapter.",
                    path.display(),
                );
            }
        }
    }
}
