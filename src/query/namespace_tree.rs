//! Namespace Tree Query — LSP adapter over analysis-engine namespace tree.

use ruby_analysis_engine::AnalysisQuery;
use serde::{Deserialize, Serialize};

use super::EngineQuery;

pub use ruby_analysis_engine::{
    IncluderInfo, LocationInfo, MixinInfo, NamespaceNode, NamespaceTreeResponse, ViaModuleInfo,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct NamespaceTreeParams {
    pub workspace_uri: Option<String>,
    #[serde(default)]
    pub show_external_types: bool,
}

impl EngineQuery {
    pub fn compute_namespace_tree_hash(&self, show_external_types: bool) -> u64 {
        let engine_ref = self.analysis_engine().expect(
            "INVARIANT VIOLATED: namespace tree query requires analysis engine. \
             This is a bug because namespace tree is derived from graph facts. \
             Fix: construct EngineQuery with with_engine().",
        );
        let engine = engine_ref.lock();
        AnalysisQuery::new(&engine).namespace_tree_hash(show_external_types)
    }

    pub fn compute_namespace_tree(&self, show_external_types: bool) -> NamespaceTreeResponse {
        let engine_ref = self.analysis_engine().expect(
            "INVARIANT VIOLATED: namespace tree query requires analysis engine. \
             This is a bug because namespace tree is derived from graph facts. \
             Fix: construct EngineQuery with with_engine().",
        );
        let engine = engine_ref.lock();
        AnalysisQuery::new(&engine).namespace_tree(show_external_types)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;
    use ruby_analysis_core::{
        FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphNodeFact, GraphNodeKind,
        RubyConstant, SourceKind, TextRange,
    };
    use ruby_analysis_engine::AnalysisEngine;
    use std::sync::Arc;

    fn empty_query(engine: AnalysisEngine) -> EngineQuery {
        EngineQuery::with_engine(Arc::new(Mutex::new(engine)))
    }

    fn constant(name: &str) -> RubyConstant {
        RubyConstant::new(name).unwrap()
    }

    #[test]
    fn analysis_namespace_tree_filters_external_mixins() {
        let mut engine = AnalysisEngine::new();
        let user_file = engine.open_or_update_file_with_kind(
            "/tmp/project/user.rb",
            "class User; include Auth; end",
            SourceKind::Project,
        );
        let auth_file = engine.open_or_update_file_with_kind(
            "/tmp/gems/auth.rb",
            "module Auth; end",
            SourceKind::Gem,
        );
        let user = FullyQualifiedName::namespace(vec![constant("User")]);
        let auth = FullyQualifiedName::namespace(vec![constant("Auth")]);
        engine.add_graph_node_fact(GraphNodeFact::new(
            user.clone(),
            GraphNodeKind::Class,
            TextRange::new(user_file, 0, 10),
        ));
        engine.add_graph_node_fact(GraphNodeFact::new(
            auth.clone(),
            GraphNodeKind::Module,
            TextRange::new(auth_file, 0, 11),
        ));
        engine.add_graph_edge_fact(GraphEdgeFact::new(
            user,
            auth,
            GraphEdgeKind::Include,
            TextRange::new(user_file, 12, 24),
        ));

        let query = empty_query(engine);
        let project_only = query.compute_namespace_tree(false);
        assert_eq!(project_only.modules.len(), 0);
        assert_eq!(project_only.classes.len(), 1);
        assert_eq!(project_only.classes[0].fqn, "User");
        assert_eq!(project_only.classes[0].includes.len(), 0);

        let with_external = query.compute_namespace_tree(true);
        assert_eq!(with_external.modules.len(), 1);
        assert_eq!(with_external.modules[0].fqn, "Auth");
        assert_eq!(with_external.classes[0].includes[0].name, "Auth");
    }
}
