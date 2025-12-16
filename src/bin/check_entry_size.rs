use ruby_fast_lsp::{
    indexer::entry::{entry_kind::ReferenceData, Entry, EntryKind},
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
    println!(
        "Size of ReferenceData: {} bytes",
        mem::size_of::<ReferenceData>()
    );

    println!("\n=== Memory per Reference Entry ===");
    let entry_size = mem::size_of::<Entry>();
    let ref_data_size = mem::size_of::<ReferenceData>();
    println!(
        "Entry struct: {} bytes + ReferenceData heap alloc: {} bytes",
        entry_size, ref_data_size
    );
    println!("Total per reference: ~{} bytes", entry_size + ref_data_size);

    println!("\n=== With 1M references ===");
    let total_mb = (1_000_000 * (entry_size + ref_data_size)) as f64 / 1_000_000.0;
    println!("Estimated memory: {:.1} MB", total_mb);
}
