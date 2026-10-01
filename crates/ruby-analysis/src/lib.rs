#![doc = include_str!("../README.md")]

#[macro_use]
#[allow(unused_macros)]
mod invariant;
#[cfg(test)]
mod invariant_tests;

pub mod core;
pub mod engine;
pub mod indexer;
pub mod inference;
pub mod stats;

#[cfg(test)]
mod architecture_tests {
    use std::path::Path;

    #[test]
    fn reusable_analysis_crate_has_no_editor_protocol_dependency() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let protocol_crate = ["tower", "-lsp"].concat();
        let manifest = std::fs::read_to_string(manifest_dir.join("Cargo.toml")).unwrap_or_else(
            |error| {
                unreachable_invariant!(
                    what = "ruby-analysis Cargo.toml could not be read: {error}",
                    why = "the architecture boundary test must inspect the crate's direct dependencies",
                    fix = "restore the crate manifest before running tests",
                    error = error,
                )
            },
        );
        invariant!(
            !manifest.contains(&protocol_crate),
            what = "ruby-analysis depends on the editor protocol crate",
            why = "checker and LSP share analysis without an LSP data model",
            fix = "use domain byte ranges; project to protocol types in the adapter",
        );

        let protocol_module = ["tower", "_lsp"].concat();
        let protocol_types = ["lsp", "_types"].concat();
        let mut pending = vec![manifest_dir.join("src")];
        while let Some(directory) = pending.pop() {
            let entries = std::fs::read_dir(&directory).unwrap_or_else(|error| {
                unreachable_invariant!(
                    what = "ruby-analysis source directory `{}` could not be read: {error}",
                    why = "skipping a directory could hide an editor-protocol dependency",
                    fix = "restore a readable source tree or deliberately update the architecture boundary root",
                    directory.display(),
                    error = error,
                )
            });
            for entry in entries {
                let entry = entry.unwrap_or_else(|error| {
                    unreachable_invariant!(
                        what = "a ruby-analysis source entry could not be read: {error}",
                        why = "the boundary audit must inspect every Rust module",
                        fix = "repair the source tree before running architecture tests",
                        error = error,
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
                    unreachable_invariant!(
                        what = "ruby-analysis source `{}` could not be decoded as UTF-8: {error}",
                        why = "rust source must be readable by the architecture audit",
                        fix = "restore valid Rust source text",
                        path.display(),
                        error = error,
                    )
                });
                invariant!(
                    !source.contains(&protocol_module) && !source.contains(&protocol_types),
                    what = "ruby-analysis source `{}` imports editor protocol types",
                    why = "reusable analysis exposes SourceFileId, TextRange, and domain records",
                    fix = "move protocol projection to the root adapter",
                    path.display(),
                );
            }
        }
    }
}
