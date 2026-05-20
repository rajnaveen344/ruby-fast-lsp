//! Diagnostics Query — analysis-engine diagnostic projection.
//!
//! Provides diagnostics for:
//! - Unresolved constants and methods
//!
//! AST-only diagnostics (syntax errors/warnings) remain in `capabilities/diagnostics.rs`.

use ruby_analysis::core::{DiagnosticFact, DiagnosticSeverity as AnalysisDiagnosticSeverity};
use std::path::PathBuf;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Url};

use super::{analysis_location::location_for_range, EngineQuery};

impl EngineQuery {
    /// Get diagnostics for unresolved entries from the analysis engine.
    pub fn get_unresolved_diagnostics(&self, uri: &Url) -> Vec<Diagnostic> {
        let analysis_engine = self.analysis_engine.as_ref().expect(
            "INVARIANT VIOLATED: unresolved diagnostics requested without analysis engine. \
             This is a bug because diagnostics are owned by ruby-analysis::engine. \
             Fix: construct EngineQuery with EngineQuery::with_engine or with_doc_and_engine.",
        );
        let engine = analysis_engine.lock();
        let path = uri
            .to_file_path()
            .unwrap_or_else(|_| PathBuf::from(uri.to_string()));
        let Some(file_id) = engine.file_id(&path) else {
            return Vec::new();
        };

        engine
            .diagnostic_facts_in_file(file_id)
            .into_iter()
            .filter_map(|fact| diagnostic_from_fact(&engine, &fact))
            .collect()
    }
}

fn diagnostic_from_fact(
    engine: &ruby_analysis::engine::AnalysisEngine,
    fact: &DiagnosticFact,
) -> Option<Diagnostic> {
    let location = location_for_range(engine, fact.range)?;
    Some(Diagnostic {
        range: location.range,
        severity: Some(lsp_diagnostic_severity(fact.severity)),
        code: Some(NumberOrString::String(fact.code.clone())),
        code_description: None,
        source: Some("ruby-fast-lsp".to_string()),
        message: fact.message.clone(),
        related_information: None,
        tags: None,
        data: None,
    })
}

fn lsp_diagnostic_severity(severity: AnalysisDiagnosticSeverity) -> DiagnosticSeverity {
    match severity {
        AnalysisDiagnosticSeverity::Error => DiagnosticSeverity::ERROR,
        AnalysisDiagnosticSeverity::Warning => DiagnosticSeverity::WARNING,
        AnalysisDiagnosticSeverity::Information => DiagnosticSeverity::INFORMATION,
        AnalysisDiagnosticSeverity::Hint => DiagnosticSeverity::HINT,
    }
}
