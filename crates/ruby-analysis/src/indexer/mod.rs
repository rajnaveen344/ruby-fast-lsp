//! Ruby AST to analysis facts.
//!
//! This crate is editor-agnostic. It parses Ruby source with Prism and emits
//! facts consumed by `ruby-analysis::engine`.

pub mod analyzer;
mod analysis_indexer;
#[cfg(test)]
mod analyzer_tests;
pub mod analyzer_utils;
pub mod document_symbols;
pub mod fact_collector;
pub mod identifier;
pub mod identifier_visitor;
pub mod rename;
mod ruby_document;
mod scope_tracker;
pub mod semantic_tokens;
mod source_document;
mod variable_scopes;
pub mod yard;

pub use analysis_indexer::{AnalysisIndex, AnalysisIndexer};
pub use analyzer::RubyPrismAnalyzer;
pub use document_symbols::{DocumentSymbolsVisitor, MethodVisibility, RubySymbolContext};
pub use identifier::{Identifier, MethodReceiver};
pub use identifier_visitor::{IdentifierType, IdentifierVisitor};
pub use rename::RenameVisitor;
pub use ruby_document::RubyDocument;
pub use scope_tracker::{
    build_constant_path_name, collect_namespaces, get_method_namespace_kind, mixin_ref_from_node,
    utf8_str, LocalScopeKind, MixinRef, ScopeFrame, ScopeTracker,
};
pub use semantic_tokens::{TokenVisitor, TOKEN_MODIFIERS, TOKEN_TYPES};
pub use source_document::SourceDocument;
pub use variable_scopes::{
    CaptureRef, LVScopeId, LVScopeKind, RenameTarget, RenameTargetKind, ScopeNode, TypeAssignment,
    VariableNode, VariableScopes,
};
