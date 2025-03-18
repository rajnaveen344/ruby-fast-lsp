use index::RubyIndex;
use tree_sitter::Parser;

mod entry;
pub mod events;
mod index;
mod traverser;

pub struct RubyIndexer {
    // The Ruby index being populated
    index: RubyIndex,

    // The Tree-sitter parser
    parser: Parser,

    // Debug flag for tests
    debug_mode: bool,
}
