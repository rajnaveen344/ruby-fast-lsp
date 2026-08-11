//! Editor-agnostic Ruby type inference.
//!
//! This crate owns type inference helpers, RBS lookup, literal/collection
//! analysis, and forward local type tracking.

pub mod completion;
pub mod control_flow;
pub mod method;
pub mod rbs;
pub mod r#type;
pub mod type_query;
pub mod type_tracker;

pub use crate::core::RubyType;
pub use method::{MethodSignature, MethodSignatureContext, MethodVisibility, Parameter};
pub use r#type::*;
pub use rbs::{get_rbs_method_return_type, has_rbs_class, rbs_declaration_count, rbs_method_count};
pub use type_query::TypeQuery;

#[cfg(test)]
mod architecture_tests {
    use std::path::Path;

    #[test]
    fn inference_layer_does_not_depend_on_editor_protocol_types() {
        let inference_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/inference");
        let mut pending = vec![inference_dir];
        while let Some(directory) = pending.pop() {
            let entries = std::fs::read_dir(&directory).unwrap_or_else(|error| {
                panic!(
                    "INVARIANT VIOLATED: inference source directory `{}` could not be read: {error}. This is a bug because the architecture boundary test must inspect every inference module. Fix: keep inference sources under crates/ruby-analysis/src/inference or update the boundary root deliberately.",
                    directory.display(),
                )
            });
            for entry in entries {
                let entry = entry.unwrap_or_else(|error| {
                    panic!(
                        "INVARIANT VIOLATED: an inference source entry could not be read: {error}. This is a bug because skipping a source file could hide an editor-protocol dependency. Fix: repair the source tree before running architecture tests."
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
                        "INVARIANT VIOLATED: inference source `{}` could not be decoded as UTF-8: {error}. This is a bug because Rust source must be UTF-8 and the boundary test cannot inspect unreadable code. Fix: restore valid Rust source text.",
                        path.display(),
                    )
                });
                let tower_protocol = ["tower", "_lsp"].concat();
                let protocol_types = ["lsp", "_types"].concat();
                assert!(
                    !source.contains(&tower_protocol) && !source.contains(&protocol_types),
                    "INVARIANT VIOLATED: inference source `{}` imports editor protocol types. This is a bug because ruby-analysis inference must be reusable by the standalone checker without an LSP data model. Fix: accept SourceFileId, TextRange, or byte offsets and convert protocol positions in the root adapter.",
                    path.display(),
                );
            }
        }
    }
}
