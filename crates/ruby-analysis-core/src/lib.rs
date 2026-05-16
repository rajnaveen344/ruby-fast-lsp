//! Core Ruby analysis data types.
//!
//! This crate intentionally contains no LSP, parser, indexer, or editor
//! dependencies. It is the shared contract for future editor and agent
//! consumers.

pub mod fully_qualified_name;
pub mod ruby_method;
pub mod ruby_namespace;
pub mod ruby_type;
pub mod symbol_store;
pub mod type_store;

pub use fully_qualified_name::{FullyQualifiedName, NamespaceKind};
pub use ruby_method::RubyMethod;
pub use ruby_namespace::RubyConstant;
pub use ruby_type::RubyType;
pub use symbol_store::{SymbolFact, SymbolKind, SymbolStore};
pub use type_store::{
    SourceFileId, TextRange, TypeFact, TypeProvenance, TypeResolution, TypeStore, TypeSubject,
};
