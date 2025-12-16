use ruby_fast_lsp::{
    indexer::entry::{Entry, EntryKind},
    types::fully_qualified_name::FullyQualifiedName,
};
use std::mem;
use tower_lsp::lsp_types::Location;

fn main() {
    println!(
        "Size of FQN: {} bytes",
        mem::size_of::<FullyQualifiedName>()
    );
    println!(
        "Size of Option<FQN>: {} bytes",
        mem::size_of::<Option<FullyQualifiedName>>()
    );
    println!("Size of Location: {} bytes", mem::size_of::<Location>());
    println!("Size of EntryKind: {} bytes", mem::size_of::<EntryKind>());
    println!("Size of Entry: {} bytes", mem::size_of::<Entry>());

    println!("\n=== Memory per Reference Entry ===");
    let entry_size = mem::size_of::<Entry>();
    println!(
        "Entry struct (with unit Reference variant): {} bytes",
        entry_size
    );
    println!("No additional heap allocation for Reference!");

    println!("\n=== With 1M references ===");
    let total_mb = (1_000_000 * entry_size) as f64 / 1_000_000.0;
    println!("Estimated memory: {:.1} MB", total_mb);
    println!("(Previously was ~200 MB with ReferenceData duplication)");
}
