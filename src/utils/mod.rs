pub mod ast;
pub mod file_ops;
pub mod parser;
pub mod ruby_environment;
pub mod stub_loader;

// Re-export commonly used functions for convenience
pub use ast::find_def_node_at_line;
pub use file_ops::{collect_ruby_files, find_ruby_files, is_project_file, should_index_file};
pub use parser::position_to_offset;
pub use ruby_environment::detect_system_ruby_version;
pub use stub_loader::{find_stubs_directory, get_stub_files};
