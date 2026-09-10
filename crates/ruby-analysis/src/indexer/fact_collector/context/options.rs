use crate::indexer::fact_collector::FactCollector;

pub(in crate::indexer::fact_collector) struct CollectionOptions {
    pub(in crate::indexer::fact_collector) record_local_read_unknown_reasons: bool,
    pub(in crate::indexer::fact_collector) resolve_analysis_method_returns: bool,
    pub(in crate::indexer::fact_collector) infer_expression_receivers: bool,
    pub(in crate::indexer::fact_collector) infer_call_outcomes: bool,
    pub(in crate::indexer::fact_collector) diagnostics_enabled: bool,
}

impl Default for CollectionOptions {
    fn default() -> Self {
        Self {
            record_local_read_unknown_reasons: true,
            resolve_analysis_method_returns: true,
            infer_expression_receivers: true,
            infer_call_outcomes: true,
            diagnostics_enabled: true,
        }
    }
}

impl FactCollector {
    pub fn without_analysis_method_return_resolution(mut self) -> Self {
        self.options.resolve_analysis_method_returns = false;
        self
    }

    /// Skip local-read proof-failure evidence when the owning source is an
    /// immutable dependency. Local scopes are still collected for semantic
    /// traversal, but dependency-local hover evidence is not retained by the
    /// engine and must not add work to cold indexing.
    pub fn without_local_read_unknown_reasons(mut self) -> Self {
        self.options.record_local_read_unknown_reasons = false;
        self
    }

    pub fn without_expression_receiver_inference(mut self) -> Self {
        self.options.infer_expression_receivers = false;
        self
    }

    /// Declaration-shaped collection for immutable dependency sources.
    ///
    /// RBS/YARD/constructor returns stay. TypeTracker, higher-order prepare,
    /// call-expression outcomes, expression-receiver inference, and local-read
    /// Unknown evidence do not: project-neutral templates strip those anyway.
    pub fn without_body_inference(mut self) -> Self {
        self.options.record_local_read_unknown_reasons = false;
        self.options.infer_expression_receivers = false;
        self.options.resolve_analysis_method_returns = false;
        self.options.infer_call_outcomes = false;
        self
    }

    pub fn without_diagnostics(mut self) -> Self {
        self.options.diagnostics_enabled = false;
        self
    }
}
