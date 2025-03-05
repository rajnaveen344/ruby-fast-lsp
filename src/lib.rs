pub mod analysis;
pub mod capabilities;
pub mod parser;
pub mod server;
pub mod workspace;

// Re-export the main components for easier access in tests
pub use analysis::RubyAnalyzer;
pub use parser::RubyParser;
pub use server::RubyLanguageServer;
pub use workspace::WorkspaceManager;
