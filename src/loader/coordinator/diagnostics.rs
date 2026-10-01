//! Publication of open-document diagnostics from current engine facts.

use super::IndexingCoordinator;
use crate::loader::context::LoadContext;
use crate::server::RubyLanguageServer;
use anyhow::Result;
use ruby_analysis::core::{
    DiagnosticFact, DiagnosticSeverity as AnalysisDiagnosticSeverity, TextRange,
};
use ruby_analysis::engine::SourceFile;
use std::sync::Arc;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};

fn diagnostic_from_fact_fast(file: &SourceFile, fact: &DiagnosticFact) -> Option<Diagnostic> {
    Some(Diagnostic {
        range: lsp_range_for_text_range_fast(file, fact.range)?,
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

fn lsp_range_for_text_range_fast(file: &SourceFile, range: TextRange) -> Option<Range> {
    let (start_line, start_character) = file.byte_offset_to_line_character(range.start_byte)?;
    let (end_line, end_character) = file.byte_offset_to_line_character(range.end_byte)?;
    Some(Range::new(
        Position::new(start_line, start_character),
        Position::new(end_line, end_character),
    ))
}

impl IndexingCoordinator {
    /// Publish a current, complete diagnostic projection for open project files.
    pub(super) async fn publish_open_project_diagnostics(
        &self,
        ctx: &LoadContext,
        server: &RubyLanguageServer,
    ) -> Result<()> {
        self.indexing_checkpoint(ctx)?;
        let analysis_engine = self.analysis_engine(ctx);
        let mut open_uris = ctx.sources.open_uris();
        open_uris.retain(|uri| Arc::ptr_eq(&analysis_engine, &server.analysis_engine_for_uri(uri)));
        open_uris.sort_unstable_by(|left, right| left.as_str().cmp(right.as_str()));

        for uri in open_uris {
            self.indexing_checkpoint(ctx)?;
            #[cfg(test)]
            let publication_path = uri
                .to_file_path()
                .expect("coordinator diagnostics must target a file URI");
            #[cfg(test)]
            server
                .indexing
                .schedule
                .checkpoint(
                    crate::loader::scheduling::test_schedule::Point::ColdDiagnosticsPending,
                    &publication_path,
                )
                .await;

            {
                // Edits and close may complete while this producer waits. Read
                // diagnostics only after acquiring the same document lock used
                // by those handlers, and recheck the indexing generation then.
                let semantic_lock = server.document_semantic_lock(&uri);
                let _semantic_guard = semantic_lock.lock().await;
                self.indexing_checkpoint(ctx)?;
                if !Arc::ptr_eq(&analysis_engine, &server.analysis_engine_for_uri(&uri)) {
                    continue;
                }
                let Some(document) = ctx.sources.open_document(&uri) else {
                    continue;
                };
                let Ok(path) = uri.to_file_path() else {
                    continue;
                };
                let mut diagnostics = {
                    let parse = document.parse();
                    crate::loader::file_processor::syntax_diagnostics::generate_diagnostics(
                        &parse, &document,
                    )
                };
                let engine = analysis_engine.read();
                let Some(file) = engine.file_id(&path).and_then(|id| engine.file(id)) else {
                    continue;
                };
                if !file.kind.contributes_project_diagnostics()
                    || !engine.file_content_matches(file.id, &document.content)
                {
                    continue;
                }
                diagnostics.extend(
                    engine
                        .diagnostic_facts_in_file(file.id)
                        .iter()
                        .filter_map(|fact| diagnostic_from_fact_fast(file, fact)),
                );
                server.append_external_linter_diagnostics_for_snapshot(
                    &uri,
                    engine.source_snapshot_for_path(&path),
                    &mut diagnostics,
                );
                self.indexing_checkpoint(ctx)?;
                // Keep the engine read lock through the synchronous enqueue:
                // cross-file resolution cannot invalidate this projection in
                // between. Empty results must clear errors resolved at startup.
                server.queue_diagnostics(uri, diagnostics);
            }
            #[cfg(test)]
            server
                .indexing
                .schedule
                .checkpoint(
                    crate::loader::scheduling::test_schedule::Point::ColdDiagnosticsAttempted,
                    &publication_path,
                )
                .await;
        }
        Ok(())
    }
}
