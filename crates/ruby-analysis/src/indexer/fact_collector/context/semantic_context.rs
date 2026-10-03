use crate::core::FullyQualifiedName;
use crate::indexer::fact_collector::FactCollector;
use crate::indexer::RubyDocument;
use crate::inference::semantics::Semantics;
use std::collections::HashSet;
use std::sync::Arc;

pub(in crate::indexer::fact_collector) struct SemanticContext {
    /// Read-only project semantics for mid-walk reads; see [`Semantics`].
    /// It is the walk handle, so every read in this walk, including the
    /// trackers it seeds, shares one method lookup memo.
    pub(in crate::indexer::fact_collector) project: Arc<dyn Semantics>,
    pub(in crate::indexer::fact_collector) method_candidates: Arc<HashSet<FullyQualifiedName>>,
    /// Same-pass method identities whose complete collected declaration set is
    /// currently public and available. `Arc::make_mut` keeps updates O(1) in
    /// the ordinary traversal because each method tracker releases its clone
    /// before the next declaration is installed.
    pub(in crate::indexer::fact_collector) public_method_candidates:
        Arc<HashSet<FullyQualifiedName>>,
    /// Namespaces this walk has declared so far.
    pub(in crate::indexer::fact_collector) known_namespaces: HashSet<FullyQualifiedName>,
    pub(in crate::indexer::fact_collector) shared_known_namespaces:
        Option<Arc<HashSet<FullyQualifiedName>>>,
    /// Namespaces the whole file declares, including those after the current
    /// node. Method bodies run after the file loads and may name them; a
    /// class body runs in order and must not.
    pub(in crate::indexer::fact_collector) file_namespaces: HashSet<FullyQualifiedName>,
}

impl SemanticContext {
    pub(in crate::indexer::fact_collector) fn new(
        document: &RubyDocument,
        project: Arc<dyn Semantics>,
    ) -> Self {
        let project = project.for_walk();
        let method_candidates = Arc::new(
            project
                .method_fqns_in_file(document.analysis_file_id())
                .into_iter()
                .collect(),
        );
        Self {
            project,
            method_candidates,
            public_method_candidates: Arc::new(HashSet::new()),
            known_namespaces: HashSet::new(),
            shared_known_namespaces: None,
            file_namespaces: HashSet::new(),
        }
    }
}

impl FactCollector {
    pub fn with_shared_direct_known_namespaces(
        mut self,
        known_namespaces: Arc<HashSet<FullyQualifiedName>>,
    ) -> Self {
        self.semantics.shared_known_namespaces = Some(known_namespaces);
        self
    }

    /// Namespaces declared anywhere in this file, which method bodies may
    /// name before their declaration. Class-body declarations ignore them.
    pub fn extend_direct_known_namespaces(
        &mut self,
        file_namespaces: impl IntoIterator<Item = FullyQualifiedName>,
    ) {
        self.semantics.file_namespaces.extend(file_namespaces);
    }

    pub(in crate::indexer::fact_collector) fn direct_namespace_is_known(
        &self,
        fqn: &FullyQualifiedName,
    ) -> bool {
        self.direct_namespace_is_declared(fqn) || self.semantics.file_namespaces.contains(fqn)
    }

    /// Whether `fqn` exists at this point of a top-to-bottom load: shared
    /// before this file, or declared above the current node.
    pub(in crate::indexer::fact_collector) fn direct_namespace_is_declared(
        &self,
        fqn: &FullyQualifiedName,
    ) -> bool {
        self.semantics.known_namespaces.contains(fqn)
            || self
                .semantics
                .shared_known_namespaces
                .as_ref()
                .is_some_and(|known| known.contains(fqn))
    }
}
