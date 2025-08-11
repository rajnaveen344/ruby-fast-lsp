/// Ruby core class stubs module
/// Ruby core class stubs module
/// 
/// This module provides pre-packaged Ruby core class stubs for fast, offline
/// completion and type information. Stubs are pre-generated and packaged 
/// with the extension.

pub mod compression;
pub mod integration;
pub mod loader;
pub mod types;
pub mod version;

// Re-export commonly used types and structs
pub use loader::StubLoader;
pub use types::{ClassStub, MethodStub, ConstantStub, VersionStubs};
pub use version::MinorVersion;