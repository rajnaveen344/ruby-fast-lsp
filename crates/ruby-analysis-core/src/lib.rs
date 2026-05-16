//! Core Ruby analysis data types.
//!
//! This crate intentionally contains no LSP, parser, indexer, or editor
//! dependencies. It is the shared contract for future editor and agent
//! consumers.

pub mod fully_qualified_name;
pub mod ruby_method;
pub mod ruby_namespace;
pub mod ruby_type;

pub use fully_qualified_name::{FullyQualifiedName, NamespaceKind};
pub use ruby_method::RubyMethod;
pub use ruby_namespace::RubyConstant;
pub use ruby_type::RubyType;
