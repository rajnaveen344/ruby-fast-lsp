//! Performance testing & benchmarking support.
//!
//! - `corpus` — fixture loader for OSS/synthetic Ruby projects used by
//!   `bench_references` and `#[ignore]` perf tests.
//! - `file_open` — library-private document instrumentation for the standalone
//!   file-open profiler; the binary owns its allocator and CLI entry point.

pub mod corpus;
pub mod metrics;

mod file_open;

/// Run the standalone file-open measurement with the current command-line arguments.
/// The caller must install the DHAT allocator and start its heap profiler.
#[doc(hidden)]
pub fn profile_file_open() {
    file_open::run();
}
