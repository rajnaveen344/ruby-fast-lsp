//! Profile memory usage when opening a file
//!
//! Usage: cargo run --release --bin profile_file_open -- <workspace_path> <file_to_open>
//!
//! This simulates:
//! 1. Indexing the workspace
//! 2. Opening a specific file (like did_open)
//! 3. Measuring memory before/after

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn main() {
    let _profiler = dhat::Profiler::new_heap();
    ruby_fast_lsp::perf::profile_file_open();
}
