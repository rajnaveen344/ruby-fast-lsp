//! Diagnostics: the engine's diagnostics for a document, and the external
//! linter and formatter runner.
//!
//! The engine-to-LSP projection itself is server-owned (`server::diagnostics`)
//! so that publication and queries share one conversion. AST-only diagnostics
//! (syntax errors and warnings) live in
//! `loader/file_processor/syntax_diagnostics.rs`.

pub mod linter;

pub use crate::server::engine_diagnostics;
