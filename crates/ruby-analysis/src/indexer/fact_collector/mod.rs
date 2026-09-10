//! Collect one source file through Prism's ordinary recursive traversal.
//!
//! State owners live beside their behavior; `traversal` defines visit order and
//! node modules translate Ruby syntax into facts. Call [`FactCollector::finish`]
//! after visiting to hand the completed collection to the file composer.

use crate::engine::AnalysisEngine;
use crate::indexer::{RubyDocument, ScopeTracker};
use parking_lot::RwLock;
use std::sync::Arc;

mod alias_method_node;
mod bad_splat;
mod block_node;
mod call_node;
mod callables;
mod class_node;
mod class_variable_write_node;
mod constant_path_node;
mod constant_path_write_node;
mod constant_read_node;
mod constant_write_node;
mod constants;
mod declarations;
mod def_node;
mod expressions;
mod extensions;
mod facts;
mod flow;
mod global_variable_write_node;
mod instance_variable_write_node;
mod local_variable_read_node;
mod local_variable_write_node;
mod method_return;
mod module_node;
mod nil_call;
mod options;
mod parameters_node;
mod semantic_context;
mod singleton_class_node;
mod source;
mod super_node;
mod traversal;
mod variable_read_node;

#[cfg(test)]
mod tests;

pub use extensions::{
    BlockExecutionContext, FactCollectorExtensionHost, NullFactCollectorExtensionHost,
};
pub use facts::CollectedFile;

use constants::ConstantEvidence;
use expressions::ExpressionEvidence;
use extensions::ExtensionState;
use facts::CollectedFacts;
use flow::FlowState;
use method_return::{InferredMethodContext, MethodReturnEvidence};
use options::CollectionOptions;
use semantic_context::SemanticContext;

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
