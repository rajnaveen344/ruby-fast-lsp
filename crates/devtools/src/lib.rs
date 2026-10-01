//! Shared support for the developer binaries: fixture corpora, latency
//! metrics, and the file-open memory measurement.

#[macro_use]
#[allow(unused_macros, dead_code)]
#[path = "../../ruby-analysis/src/invariant.rs"]
mod invariant;

pub mod corpus;
pub mod file_open;
pub mod metrics;

use std::path::{Path, PathBuf};

/// Repository root, baked in at compile time so detached binaries
/// (e.g. `./target/release/bench_references`) still find corpora and git state.
pub fn workspace_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .unwrap_or_else(|| {
            unreachable_invariant!(
                what = "devtools manifest {} is not two levels below the repository root",
                why = "corpus and evidence paths are resolved from the repository root",
                fix = "keep the crate at crates/devtools or update workspace_root",
                manifest_dir.display(),
            )
        })
}
