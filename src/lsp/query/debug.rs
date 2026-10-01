//! Debug Query — LSP adapter over analysis-engine inspection commands.

use crate::invariant::ExpectInvariant;
use ruby_analysis::engine::{AnalysisQuery, ExportGraphResponse, LookupResponse};

use super::EngineQuery;

impl EngineQuery {
    pub fn debug_lookup(&self, fqn: &str) -> LookupResponse {
        let engine_ref = self.debug_engine();
        let engine = engine_ref.read();
        AnalysisQuery::new(&engine).debug_lookup(fqn)
    }

    pub fn debug_export_graph(&self) -> ExportGraphResponse {
        let engine_ref = self.debug_engine();
        let engine = engine_ref.read();
        AnalysisQuery::new(&engine).debug_export_graph()
    }

    fn debug_engine(
        &self,
    ) -> &std::sync::Arc<parking_lot::RwLock<ruby_analysis::engine::AnalysisEngine>> {
        self.analysis_engine.as_ref().expect_invariant(
            "debug query requested without analysis engine",
            "debug LSP commands must inspect AnalysisEngine facts",
            "construct EngineQuery with with_engine()",
        )
    }
}
