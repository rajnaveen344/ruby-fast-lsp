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
mod erb;
pub mod fact_collector;
pub mod hover;
pub mod identifier;
pub mod identifier_visitor;
pub mod inlay_hints;
mod rbs_indexer;
pub mod receiver_resolution;
pub mod rename;
mod ruby_document;
mod scope_tracker;
mod selection_ranges;
pub mod semantic_tokens;
mod source_document;
mod variable_scopes;
pub mod yard;

pub use analysis_indexer::{AnalysisIndex, AnalysisIndexer};
pub use analyzer::{RubyPrismAnalyzer, SignatureHelpTarget};
pub use code_lens::{module_definitions_for_lens, ModuleDefinitionForLens};
pub use document_symbols::{
    DocumentSymbolKind, DocumentSymbolsVisitor, MethodVisibility, RubySymbolContext,
};
pub use erb::{is_erb_path, mask_erb, EmbeddedRuby};
pub use hover::{identifier_to_hover_target, HoverTarget};
pub use identifier::{Identifier, MethodReceiver};
pub use identifier_visitor::{IdentifierType, IdentifierVisitor};
pub use rbs_indexer::index_rbs;
pub use receiver_resolution::{
    resolve_receiver_to_namespace, resolve_receiver_type, ReceiverResolutionContext,
};
pub use rename::RenameVisitor;
pub use ruby_document::RubyDocument;
pub use scope_tracker::{
    build_constant_path_name, collect_namespaces, constant_path_is_absolute,
    get_method_namespace_kind, mixin_ref_from_node, utf8_str, LocalScopeKind, MixinRef, ScopeFrame,
    ScopeTracker,
};
pub use selection_ranges::selection_range_chains;
pub use semantic_tokens::{
    SemanticTokenData, SemanticTokenKind, SemanticTokenModifierKind, TokenVisitor, TOKEN_MODIFIERS,
    TOKEN_TYPES,
};
pub use source_document::{mask_shebang, SourceDocument};
pub use variable_scopes::{
    CaptureRef, LVScopeId, LVScopeKind, RenameTarget, RenameTargetKind, ScopeNode, TypeAssignment,
    VariableNode, VariableScopes,
};

pub fn is_framework_instance_block_call_name(name: &[u8]) -> bool {
    matches!(
        name,
        b"get" | b"post" | b"put" | b"patch" | b"delete" | b"options" | b"head"
    )
}
