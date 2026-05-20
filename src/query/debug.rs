//! Debug Query — LSP adapter over analysis-engine inspection commands.

use ruby_analysis_engine::{
    AnalysisQuery, AncestorsResponse, ExportGraphResponse, InferenceStatsResponse, LookupResponse,
    MethodsResponse, StatsResponse,
};

use super::EngineQuery;

impl EngineQuery {
    pub fn debug_lookup(&self, fqn: &str) -> LookupResponse {
        let engine_ref = self.debug_engine();
        let engine = engine_ref.lock();
        AnalysisQuery::new(&engine).debug_lookup(fqn)
    }

    pub fn debug_stats(&self, indexing_complete: bool) -> StatsResponse {
        let engine_ref = self.debug_engine();
        let engine = engine_ref.lock();
        AnalysisQuery::new(&engine).debug_stats(indexing_complete)
    }

    pub fn debug_ancestors(&self, class_name: &str) -> AncestorsResponse {
        let engine_ref = self.debug_engine();
        let engine = engine_ref.lock();
        AnalysisQuery::new(&engine).debug_ancestors(class_name)
    }

    pub fn debug_methods(&self, class_name: &str) -> MethodsResponse {
        let engine_ref = self.debug_engine();
        let engine = engine_ref.lock();
        AnalysisQuery::new(&engine).debug_methods(class_name)
    }

    pub fn debug_inference_stats(&self) -> InferenceStatsResponse {
        let engine_ref = self.debug_engine();
        let engine = engine_ref.lock();
        AnalysisQuery::new(&engine).debug_inference_stats()
    }

    pub fn debug_export_graph(&self) -> ExportGraphResponse {
        let engine_ref = self.debug_engine();
        let engine = engine_ref.lock();
        AnalysisQuery::new(&engine).debug_export_graph()
    }

    fn debug_engine(&self) -> &parking_lot::Mutex<ruby_analysis_engine::AnalysisEngine> {
        self.analysis_engine.as_ref().expect(
            "INVARIANT VIOLATED: debug query requested without analysis engine. \
             This is a bug because debug LSP commands must inspect AnalysisEngine facts. \
             Fix: construct EngineQuery with with_engine().",
        )
    }
}
