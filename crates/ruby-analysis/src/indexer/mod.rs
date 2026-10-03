//! Ruby AST to analysis facts.
//!
//! This module parses Ruby source with Prism and emits facts consumed by
//! [`crate::engine`]. Declaration collection starts at [`AnalysisIndexer`];
//! [`fact_collector::FactCollector`] adds body, reference, diagnostic, and
//! extension evidence. The caller composes the resulting file facts and owns
//! scheduling and publication. This module does not own project truth.
//!
//! The indexer depends on `core` and `inference` only. It reads project state
//! through [`crate::inference::semantics::Semantics`], which a caller supplies
//! as a `View` or the shared engine; outside tests it names no engine type
//! (enforced by `architecture_tests::analysis_layers_depend_only_downward`).

pub(crate) mod documents;
pub mod fact_collector;
pub(crate) mod identifiers;
pub mod inlay_hints;
pub(crate) mod lowering;
pub(crate) mod queries;
pub mod yard;

pub use documents::erb::{is_erb_path, mask_erb, EmbeddedRuby};
pub use documents::ruby_document::RubyDocument;
pub use documents::scope_tracker::{
    build_constant_path_name, collect_namespaces, constant_path_is_absolute, mixin_ref_from_node,
    utf8_str, LocalScopeKind, MixinRef, ScopeFrame, ScopeTracker,
};
pub use documents::source_document::{mask_shebang, SourceDocument};
pub use documents::variable_scopes::{
    LVScopeId, LVScopeKind, RenameTarget, RenameTargetKind, ScopeNode, TypeAssignment,
    VariableNode, VariableScopes,
};
pub use identifiers::types::Identifier;
pub use identifiers::{IdentifierType, IdentifierVisitor};
pub use lowering::analysis_indexer::AnalysisIndexer;
pub use lowering::rbs_indexer::index_rbs;
pub use queries::code_lens::{module_definitions_for_lens, ModuleDefinitionForLens};
pub use queries::document_symbols::{
    DocumentSymbolKind, DocumentSymbolsVisitor, RubySymbolContext,
};
pub use queries::hover::{identifier_to_hover_target, HoverTarget};
pub use queries::receivers::{
    resolve_receiver_to_namespace, resolve_receiver_type, ReceiverResolutionContext,
};
pub use queries::rename::RenameVisitor;
pub use queries::selection_ranges::selection_range_chains;
pub use queries::semantic_tokens::{
    SemanticTokenData, SemanticTokenKind, SemanticTokenModifierKind, TokenVisitor, TOKEN_MODIFIERS,
    TOKEN_TYPES,
};
pub use queries::syntax::call_reference_location;
pub use queries::{
    CompletionReceiverTarget, RubyPrismAnalyzer, ShapeKeyCompletionTarget, ShapeKeySyntax,
    SignatureHelpTarget,
};

pub fn is_framework_instance_block_call_name(name: &[u8]) -> bool {
    matches!(
        name,
        b"get" | b"post" | b"put" | b"patch" | b"delete" | b"options" | b"head"
    )
}
