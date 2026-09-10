//! Collect one source file through Prism's ordinary recursive traversal.
//!
//! State owners live beside their behavior; `traversal` defines visit order and
//! node modules translate Ruby syntax into facts. Call [`FactCollector::finish`]
//! after visiting to hand the completed collection to the file composer.

use crate::engine::AnalysisEngine;
use crate::indexer::{RubyDocument, ScopeTracker};
use parking_lot::RwLock;
use std::sync::Arc;

mod collection;
mod context;
mod inference;
mod nodes;
mod traversal;

#[cfg(test)]
mod tests;

pub use collection::facts::CollectedFile;
pub use context::extensions::{
    BlockExecutionContext, FactCollectorExtensionHost, NullFactCollectorExtensionHost,
};

use collection::facts::CollectedFacts;
use context::{
    extensions::ExtensionState, options::CollectionOptions, semantic_context::SemanticContext,
};
use inference::{
    constants::ConstantEvidence, expressions::ExpressionEvidence, flow::FlowState,
    method_return::MethodReturnEvidence,
};

/// A single file pass. Temporary proof and traversal state never outlives it.
pub struct FactCollector {
    document: RubyDocument,
    scope_tracker: ScopeTracker,
    options: CollectionOptions,
    extensions: ExtensionState,
    facts: CollectedFacts,
    flow: FlowState,
    semantics: SemanticContext,
    method_returns: MethodReturnEvidence,
    expressions: ExpressionEvidence,
    constants: ConstantEvidence,
}

impl FactCollector {
    pub fn analysis_only(
        document: RubyDocument,
        extension_host: Arc<dyn FactCollectorExtensionHost>,
        analysis_engine: Arc<RwLock<AnalysisEngine>>,
    ) -> Self {
        let semantics = SemanticContext::new(&document, analysis_engine);
        Self {
            document,
            scope_tracker: ScopeTracker::new(),
            options: CollectionOptions::default(),
            extensions: ExtensionState::new(extension_host),
            facts: CollectedFacts::default(),
            flow: FlowState::default(),
            semantics,
            method_returns: MethodReturnEvidence::default(),
            expressions: ExpressionEvidence::default(),
            constants: ConstantEvidence::default(),
        }
    }

    pub fn document(&self) -> &RubyDocument {
        &self.document
    }

    /// Retain only the updated document when rebuilding local variable scopes.
    /// Unlike full file composition, this projection needs no proof snapshots.
    pub fn into_document(self) -> RubyDocument {
        self.document
    }

    pub fn scope_tracker(&self) -> &ScopeTracker {
        &self.scope_tracker
    }

    pub fn analysis_engine(&self) -> &Arc<RwLock<AnalysisEngine>> {
        &self.semantics.engine
    }
}
