pub mod ast;
pub mod cache;
pub mod file_ops;
pub mod lsp;
pub mod parser;
pub mod stub_loader;

// Re-export commonly used functions for convenience
pub use ast::find_def_node_at_line;
pub use cache::ruby_fast_lsp_user_cache_root;
pub use file_ops::{
    collect_project_files, collect_project_signature_files, collect_ruby_files, find_ruby_files,
    should_index_file, ProjectFilePolicy,
};
pub use lsp::deduplicate_locations;
pub use parser::{offset_to_line, position_to_offset};
pub use stub_loader::{find_stubs_directory, get_stub_files};
