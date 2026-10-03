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
    use std::path::{Path, PathBuf};

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

    /// Layers in dependency order: each may name only the layers before it.
    /// `core` names none of them.
    const LAYER_RULES: [(&str, &[&str]); 2] = [
        ("inference", &["engine", "indexer"]),
        ("indexer", &["engine"]),
    ];

    #[test]
    fn analysis_layers_depend_only_downward() {
        let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut violations = Vec::new();
        for (layer, upward) in LAYER_RULES {
            for path in non_test_rust_sources(&source_root.join(layer)) {
                let source = std::fs::read_to_string(&path).unwrap_or_else(|error| {
                    unreachable_invariant!(
                        what = "ruby-analysis source `{}` could not be decoded as UTF-8: {error}",
                        why = "rust source must be readable by the layer audit",
                        fix = "restore valid Rust source text",
                        path.display(),
                        error = error,
                    )
                });
                for (line_number, line) in non_test_code_lines(&source) {
                    for module in upward {
                        if names_module(line, module) {
                            violations.push(format!(
                                "{}:{line_number}: `{layer}` names `{module}`",
                                path.display()
                            ));
                        }
                    }
                }
            }
        }
        invariant!(
            violations.is_empty(),
            what = "analysis layers name a higher layer outside tests:\n{}",
            why = "dependencies point one way: core <- inference <- indexer <- engine",
            fix = "read project state through `inference::semantics::Semantics`, or move the code to the layer that owns it",
            violations.join("\n"),
        );
    }

    /// Whether `line` paths into `module` from the crate root or through
    /// `super`.
    fn names_module(line: &str, module: &str) -> bool {
        ["crate::", "super::"].iter().any(|prefix| {
            let needle = format!("{prefix}{module}");
            line.match_indices(&needle).any(|(start, _)| {
                !line[start + needle.len()..]
                    .chars()
                    .next()
                    .is_some_and(|next| next.is_alphanumeric() || next == '_')
            })
        })
    }

    /// The numbered lines a file compiles outside tests: everything before its
    /// first `#[cfg(test)]`, without comment lines (doc links may name any
    /// layer).
    fn non_test_code_lines(source: &str) -> impl Iterator<Item = (usize, &str)> {
        source
            .lines()
            .take_while(|line| line.trim() != "#[cfg(test)]")
            .enumerate()
            .map(|(index, line)| (index + 1, line))
            .filter(|(_, line)| !line.trim_start().starts_with("//"))
    }

    /// Every Rust file under `root` except test modules (`tests/` folders,
    /// `tests.rs`, and `*_tests.rs`).
    fn non_test_rust_sources(root: &Path) -> Vec<PathBuf> {
        let mut sources = Vec::new();
        let mut pending = vec![root.to_path_buf()];
        while let Some(directory) = pending.pop() {
            let entries = std::fs::read_dir(&directory).unwrap_or_else(|error| {
                unreachable_invariant!(
                    what = "ruby-analysis source directory `{}` could not be read: {error}",
                    why = "skipping a directory could hide an upward layer edge",
                    fix = "restore a readable source tree or deliberately update the layer rules",
                    directory.display(),
                    error = error,
                )
            });
            for entry in entries {
                let path = entry
                    .unwrap_or_else(|error| {
                        unreachable_invariant!(
                            what = "a ruby-analysis source entry could not be read: {error}",
                            why = "the layer audit must inspect every Rust module",
                            fix = "repair the source tree before running architecture tests",
                            error = error,
                        )
                    })
                    .path();
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default();
                if path.is_dir() {
                    if name != "tests" {
                        pending.push(path);
                    }
                } else if name.ends_with(".rs")
                    && name != "tests.rs"
                    && !name.ends_with("_tests.rs")
                {
                    sources.push(path);
                }
            }
        }
        sources
    }
}
