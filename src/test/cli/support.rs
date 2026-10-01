//! Shared helpers for CLI `check` and LSP parity tests.

use crate::lsp::check::{CheckDiagnostic, CheckReport};
use tower_lsp::lsp_types::NumberOrString;

pub(super) fn hover_text(hover: tower_lsp::lsp_types::Hover) -> String {
    match hover.contents {
        tower_lsp::lsp_types::HoverContents::Scalar(marked) => match marked {
            tower_lsp::lsp_types::MarkedString::String(value) => value,
            tower_lsp::lsp_types::MarkedString::LanguageString(value) => value.value,
        },
        tower_lsp::lsp_types::HoverContents::Array(values) => values
            .into_iter()
            .map(|marked| match marked {
                tower_lsp::lsp_types::MarkedString::String(value) => value,
                tower_lsp::lsp_types::MarkedString::LanguageString(value) => value.value,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        tower_lsp::lsp_types::HoverContents::Markup(value) => value.value,
    }
}

pub(super) fn find_cli_diagnostic<'a>(report: &'a CheckReport, code: &str) -> &'a CheckDiagnostic {
    report
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.as_deref() == Some(code))
        .unwrap_or_else(|| {
            panic!(
                "check must return the proven `{code}` diagnostic; got {:?}",
                report.diagnostics
            )
        })
}

pub(super) fn find_lsp_diagnostic<'a>(
    diagnostics: &'a [tower_lsp::lsp_types::Diagnostic],
    code: &str,
) -> &'a tower_lsp::lsp_types::Diagnostic {
    diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code.as_ref().is_some_and(
                |code_value| matches!(code_value, NumberOrString::String(value) if value == code),
            )
        })
        .unwrap_or_else(|| {
            panic!("LSP must return the proven `{code}` diagnostic; got {diagnostics:?}")
        })
}

pub(super) fn assert_diagnostic_parity(
    check_diagnostic: &CheckDiagnostic,
    lsp_diagnostic: &tower_lsp::lsp_types::Diagnostic,
) {
    assert_eq!(check_diagnostic.message, lsp_diagnostic.message);
    invariant_eq!(
        (
            check_diagnostic.range.start.line,
            check_diagnostic.range.start.column,
            check_diagnostic.range.end.line,
            check_diagnostic.range.end.column,
        ),
        (
            lsp_diagnostic.range.start.line + 1,
            lsp_diagnostic.range.start.character + 1,
            lsp_diagnostic.range.end.line + 1,
            lsp_diagnostic.range.end.character + 1,
        ),
        what = "CLI and LSP projected different ranges for one engine-owned diagnostic",
        why = "adapters may change indexing conventions but not semantic locations",
        fix = "keep check range conversion aligned with LSP UTF-16 positions",
    );
}
