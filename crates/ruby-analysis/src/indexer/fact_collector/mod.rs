//! Collect one source file through Prism's ordinary recursive traversal.
//!
//! State owners live beside their behavior; `traversal` defines visit order and
//! node modules translate Ruby syntax into facts. Call [`FactCollector::finish`]
//! after visiting to hand the completed collection to the file composer.

use crate::core::MethodReceiver;
use crate::core::{FullyQualifiedName, ResolvedMethodCallee, RubyMethod, TypeFact, TypeSubject};
use crate::indexer::{RubyDocument, ScopeTracker};
use crate::inference::semantics::Semantics;
use std::sync::Arc;

mod collection;
mod context;
mod inference;
mod nodes;
mod traversal;

#[cfg(test)]
mod tests;

pub use collection::facts::FactCollectorOutput;
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
        project_semantics: Arc<dyn Semantics>,
    ) -> Self {
        // Each mid-walk read goes through the walk handle; the shared engine
        // takes its own short read guard per read.
        let semantics = SemanticContext::new(&document, project_semantics);
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

    /// Callees an extension sees for a call in the current scope.
    pub fn extension_call_callees(
        &self,
        receiver: &MethodReceiver,
        method: &RubyMethod,
    ) -> Vec<ResolvedMethodCallee> {
        self.semantics.project.extension_call_callees(
            receiver,
            method,
            &self.scope_tracker.get_ns_stack(),
            self.scope_tracker.current_method_context(),
        )
    }

    /// Whether the project engine has a class or module node for `namespace`.
    pub fn project_namespace_exists(&self, namespace: &FullyQualifiedName) -> bool {
        self.semantics
            .project
            .namespace_node_kind(namespace)
            .is_some()
    }

    /// Type facts the project engine has installed for `subject`.
    pub fn project_type_facts_for(&self, subject: &TypeSubject) -> Vec<TypeFact> {
        self.semantics.project.type_facts_for(subject)
    }
}
