use anyhow::anyhow;
use log::{warn, info};

// External function declaration for tree-sitter-ruby
extern "C" {
    fn tree_sitter_ruby() -> ::tree_sitter::Language;
}

pub fn ruby_language() -> Option<::tree_sitter::Language> {
    // Try to load the Ruby language
    match unsafe { tree_sitter_ruby() } {
        lang => {
            info!("Successfully loaded tree-sitter-ruby language");
            Some(lang)
        }
    }
}
