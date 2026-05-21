//! Ruby AST to analysis facts.
//!
//! This crate is editor-agnostic. It parses Ruby source with Prism and emits
//! facts consumed by `ruby-analysis::engine`.

mod analysis_indexer;
pub mod analyzer;
#[cfg(test)]
mod analyzer_tests;
pub mod analyzer_utils;
pub mod code_lens;
pub mod document_symbols;
pub mod fact_collector;
pub mod hover;
pub mod identifier;
pub mod identifier_visitor;
pub mod inlay_hints;
pub mod receiver_resolution;
pub mod rename;
mod ruby_document;
mod scope_tracker;
pub mod semantic_tokens;
mod source_document;
mod variable_scopes;
pub mod yard;

pub use analysis_indexer::{AnalysisIndex, AnalysisIndexer};
pub use analyzer::RubyPrismAnalyzer;
pub use code_lens::{module_definitions_for_lens, ModuleDefinitionForLens};
pub use document_symbols::{DocumentSymbolsVisitor, MethodVisibility, RubySymbolContext};
pub use hover::{identifier_to_hover_target, HoverTarget};
pub use identifier::{Identifier, MethodReceiver};
pub use identifier_visitor::{IdentifierType, IdentifierVisitor};
pub use receiver_resolution::{
    resolve_receiver_to_namespace, resolve_receiver_type, ReceiverResolutionContext,
};
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
