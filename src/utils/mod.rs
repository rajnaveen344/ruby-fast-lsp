pub mod file_ops;
pub mod parser;
pub mod ruby_environment;

// Re-export commonly used functions for convenience
pub use file_ops::{collect_ruby_files, find_ruby_files, is_project_file, should_index_file};
pub use parser::position_to_offset;
pub use ruby_environment::detect_system_ruby_version;
