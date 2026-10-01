//! Namespace Tree Query — LSP adapter over analysis-engine namespace tree.

use crate::invariant::ExpectInvariant;
use ruby_analysis::engine::AnalysisQuery;
use serde::{Deserialize, Serialize};

use crate::lsp::query::EngineQuery;

pub use ruby_analysis::engine::{
    IncluderInfo, LibraryNamespaceTree, LibraryPackageTree, LibrarySectionId, LocationInfo,
    MixinInfo, NamespaceNode, NamespaceTreeResponse, ViaModuleInfo,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct NamespaceTreeParams {
    #[serde(default, rename = "uri", alias = "workspace_uri")]
    pub workspace_uri: Option<String>,
    #[serde(default)]
    pub show_external_types: bool,
}

impl EngineQuery {
    pub fn compute_namespace_tree_hash(&self, show_external_types: bool) -> u64 {
        let engine_ref = self.analysis_engine().expect_invariant(
            "namespace tree query requires analysis engine",
            "namespace tree is derived from graph facts",
            "construct EngineQuery with with_engine()",
        );
        let engine = engine_ref.read();
        AnalysisQuery::new(&engine).namespace_tree_hash(show_external_types)
    }

    pub fn compute_namespace_tree(&self, show_external_types: bool) -> NamespaceTreeResponse {
        let engine_ref = self.analysis_engine().expect_invariant(
            "namespace tree query requires analysis engine",
            "namespace tree is derived from graph facts",
            "construct EngineQuery with with_engine()",
        );
        let engine = engine_ref.read();
        AnalysisQuery::new(&engine).namespace_tree(show_external_types)
    }
}
