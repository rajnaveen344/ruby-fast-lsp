// External function declaration for tree-sitter-ruby

use log::info;

// Import the tree_sitter_ruby function from the tree-sitter-ruby crate
// This is the correct way to import the function
use tree_sitter_ruby::language as tree_sitter_ruby;

pub fn ruby_language() -> Option<::tree_sitter::Language> {
    // Try to load the Ruby language
    match tree_sitter_ruby() {
        lang => {
            info!("Successfully loaded tree-sitter-ruby language");
            Some(lang)
        }
    }
}
