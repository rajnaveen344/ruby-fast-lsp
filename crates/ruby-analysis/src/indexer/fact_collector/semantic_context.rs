use super::FactCollector;
use crate::core::FullyQualifiedName;
use crate::engine::{AnalysisEngine, AnalysisQueryCache};
use crate::indexer::RubyDocument;
use parking_lot::RwLock;
use std::collections::HashSet;
use std::sync::Arc;

pub(super) struct SemanticContext {
    pub(super) engine: Arc<RwLock<AnalysisEngine>>,
    pub(super) query_cache: Arc<AnalysisQueryCache>,
    pub(super) method_candidates: Arc<HashSet<FullyQualifiedName>>,
    /// Same-pass method identities whose complete collected declaration set is
    /// currently public and available. `Arc::make_mut` keeps updates O(1) in
    /// the ordinary traversal because each method tracker releases its clone
    /// before the next declaration is installed.
    pub(super) public_method_candidates: Arc<HashSet<FullyQualifiedName>>,
    pub(super) known_namespaces: HashSet<FullyQualifiedName>,
    pub(super) shared_known_namespaces: Option<Arc<HashSet<FullyQualifiedName>>>,
}

impl SemanticContext {
    pub(super) fn new(document: &RubyDocument, engine: Arc<RwLock<AnalysisEngine>>) -> Self {
        let method_candidates = Arc::new(
            engine
                .read()
                .method_facts_in_file(document.analysis_file_id())
                .into_iter()
                .map(|fact| fact.fqn)
                .collect(),
        );
        Self {
            engine,
            query_cache: Arc::new(AnalysisQueryCache::default()),
            method_candidates,
            public_method_candidates: Arc::new(HashSet::new()),
            known_namespaces: HashSet::new(),
            shared_known_namespaces: None,
        }
    }
}

impl FactCollector {
    pub fn analysis_query_cache(&self) -> &AnalysisQueryCache {
        self.semantics.query_cache.as_ref()
    }

    pub fn with_direct_known_namespaces(
        mut self,
        known_namespaces: HashSet<FullyQualifiedName>,
    ) -> Self {
        self.semantics.known_namespaces = known_namespaces;
        self
    }

    pub fn with_shared_direct_known_namespaces(
        mut self,
        known_namespaces: Arc<HashSet<FullyQualifiedName>>,
    ) -> Self {
        self.semantics.shared_known_namespaces = Some(known_namespaces);
        self
    }

    pub fn extend_direct_known_namespaces(
        &mut self,
        known_namespaces: impl IntoIterator<Item = FullyQualifiedName>,
    ) {
        self.semantics.known_namespaces.extend(known_namespaces);
    }

    pub(super) fn direct_namespace_is_known(&self, fqn: &FullyQualifiedName) -> bool {
        self.semantics.known_namespaces.contains(fqn)
            || self
                .semantics
                .shared_known_namespaces
                .as_ref()
                .is_some_and(|known| known.contains(fqn))
    }
}
