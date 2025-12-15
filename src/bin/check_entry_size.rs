use ruby_fast_lsp::indexer::entry::EntryKind;
use std::mem;

fn main() {
    println!("Size of EntryKind: {} bytes", mem::size_of::<EntryKind>());
}
