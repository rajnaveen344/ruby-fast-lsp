//! Fixture checks and delivered-diagnostic assertions.

use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString};

use super::FakeEditor;

use crate::test::harness::check::{run_fixture_checks, FixtureFile};
use crate::test::harness::fixture::{parse_fixture, strip_markers};

impl FakeEditor {
    /// Run tag-based assertions against a file's current state.
    ///
    /// The `fixture` contains markers ($0, <def>, <ref>, <hint>, etc.) embedded
    /// in the expected file content. The clean content extracted from the fixture
    /// must match the file's current buffer content.
    pub async fn check(&self, filename: &str, fixture: &str) {
        let (buffer_content, _) = self.buffers.get(filename).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: File '{}' is not open. Call open() before check().",
                filename
            )
        });
        let fixture = parse_fixture(fixture);
        assert!(
            *buffer_content == fixture.source,
            "fixture for '{filename}' does not match the open buffer; marker positions would be wrong.\n\
             Buffer:\n{buffer_content}\n\nFixture (cleaned):\n{}",
            fixture.source
        );
        let file = FixtureFile {
            uri: Self::filename_to_uri(filename),
            fixture,
        };
        run_fixture_checks(&self.server, std::slice::from_ref(&file)).await;
    }

    /// Open the marker-bearing fixture and immediately run its assertions.
    pub async fn open_and_check_fixture(&mut self, filename: &str, fixture: &str) {
        let content = strip_markers(fixture);
        self.open(filename, &content).await;
        self.check(filename, fixture).await;
    }

    /// Replace an open buffer with a marker-bearing fixture through didChange,
    /// then run the assertions against the new semantic generation.
    pub async fn set_and_check_fixture(&mut self, filename: &str, fixture: &str) {
        let content = strip_markers(fixture);
        self.set(filename, &content).await;
        self.check(filename, fixture).await;
    }

    /// Observe diagnostics delivered by the production queue, sender, and client.
    /// The submission boundary supplies the expected latest value only; it can
    /// never substitute for a missing notification or repair semantic state.
    pub async fn diagnostics(&self, filename: &str) -> Vec<Diagnostic> {
        self.assert_open(filename, "diagnostics");
        let uri = Self::filename_to_uri(filename);
        let submitted = self.server.last_diagnostic_publication(&uri)
            .expect("INVARIANT VIOLATED: open document has no diagnostic publication. This is a bug because a missing notification must not count as an empty diagnostic result. Fix: inspect the document publication lifecycle; do not recompute diagnostics in the observer.");
        self.client_messages.diagnostics(&uri, &submitted).await
    }

    /// Assert the file has zero ERROR-severity diagnostics.
    ///
    /// Panics with a list of all errors if any are found. WARNING/INFO/HINT
    /// diagnostics are ignored — use `assert_no_diagnostics()` for stricter check.
    pub async fn assert_no_errors(&self, filename: &str) {
        let diags = self.diagnostics(filename).await;
        let errors: Vec<&Diagnostic> = diags
            .iter()
            .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
            .collect();
        assert!(
            errors.is_empty(),
            "Expected no errors in '{}', got {}: {:?}",
            filename,
            errors.len(),
            errors.iter().map(|e| describe(e)).collect::<Vec<_>>()
        );
    }

    /// Assert at least one ERROR diagnostic with the given code exists.
    /// Returns the matched diagnostic for further inspection.
    pub async fn assert_error_code(&self, filename: &str, code: &str) -> Diagnostic {
        let diags = self.diagnostics(filename).await;
        let found = diags.iter().find(|d| {
            d.severity == Some(DiagnosticSeverity::ERROR)
                && matches!(&d.code, Some(NumberOrString::String(s)) if s == code)
        });
        match found {
            Some(d) => d.clone(),
            None => panic!(
                "Expected error with code '{}' in '{}'. Actual diagnostics: {:?}",
                code,
                filename,
                diags.iter().map(describe).collect::<Vec<_>>()
            ),
        }
    }
}

/// Pretty-print a diagnostic for assertion failure messages.
fn describe(d: &Diagnostic) -> String {
    let code = match &d.code {
        Some(NumberOrString::String(s)) => s.clone(),
        Some(NumberOrString::Number(n)) => n.to_string(),
        None => "<no-code>".to_string(),
    };
    let sev = match d.severity {
        Some(DiagnosticSeverity::ERROR) => "ERROR",
        Some(DiagnosticSeverity::WARNING) => "WARNING",
        Some(DiagnosticSeverity::INFORMATION) => "INFO",
        Some(DiagnosticSeverity::HINT) => "HINT",
        _ => "?",
    };
    format!(
        "{}:{}-{}:{} [{} {}] {:?}",
        d.range.start.line,
        d.range.start.character,
        d.range.end.line,
        d.range.end.character,
        sev,
        code,
        d.message
    )
}
