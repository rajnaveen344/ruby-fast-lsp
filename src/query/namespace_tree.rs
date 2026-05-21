//! Namespace Tree Query — LSP adapter over analysis-engine namespace tree.

use ruby_analysis::engine::AnalysisQuery;
use serde::{Deserialize, Serialize};

use super::EngineQuery;

pub use ruby_analysis::engine::{
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
