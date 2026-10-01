//! Published diagnostics for one file.

use crate::invariant::ExpectInvariant;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Url};

use super::ranges_overlap;
use crate::server::RubyLanguageServer;
use crate::test::harness::fixture::Tag;

/// `<err>`/`<warn>` match one diagnostic each with exactly the tagged range,
/// the `code` if given, and a message containing `message` if given.
/// `<err none>`/`<warn none>` forbid that severity (optionally of `code`) in the range.
pub(super) fn check_diagnostics(
    server: &RubyLanguageServer,
    uri: &Url,
    err_tags: &[&Tag],
    warn_tags: &[&Tag],
) {
    let published = server.last_diagnostic_publication(uri).expect_invariant(
        "tagged diagnostic assertion has no published result",
        "observing an open document must not fabricate an empty clear",
        "inspect the publication lifecycle; never collect or replace facts in an assertion",
    );
    check_severity("error", DiagnosticSeverity::ERROR, &published, err_tags);
    check_severity(
        "warning",
        DiagnosticSeverity::WARNING,
        &published,
        warn_tags,
    );
}

fn check_severity(
    label: &str,
    severity: DiagnosticSeverity,
    published: &[Diagnostic],
    tags: &[&Tag],
) {
    let mut unmatched: Vec<&Diagnostic> = published
        .iter()
        .filter(|diagnostic| diagnostic.severity == Some(severity))
        .collect();
    let all = unmatched.clone();

    for tag in tags.iter().filter(|tag| tag.none) {
        let code = tag.attr("code");
        let found: Vec<String> = all
            .iter()
            .filter(|diagnostic| ranges_overlap(&diagnostic.range, &tag.range))
            .filter(|diagnostic| code.is_none_or(|code| code_of(diagnostic) == Some(code)))
            .map(|diagnostic| describe(diagnostic))
            .collect();
        assert!(
            found.is_empty(),
            "expected no {label}s{} in {:?}, got: {found:?}",
            code.map(|code| format!(" [code={code}]"))
                .unwrap_or_default(),
            tag.range
        );
    }

    for tag in tags.iter().filter(|tag| !tag.none) {
        let index = unmatched
            .iter()
            .position(|diagnostic| matches_tag(diagnostic, tag))
            .unwrap_or_else(|| {
                panic!(
                    "expected {label} at {:?}{} not found. Actual {label}s: {:?}",
                    tag.range,
                    describe_expectation(tag),
                    all.iter()
                        .map(|diagnostic| describe(diagnostic))
                        .collect::<Vec<_>>()
                )
            });
        unmatched.remove(index);
    }
}

fn matches_tag(diagnostic: &Diagnostic, tag: &Tag) -> bool {
    diagnostic.range == tag.range
        && tag
            .attr("code")
            .is_none_or(|code| code_of(diagnostic) == Some(code))
        && tag
            .attr("message")
            .is_none_or(|message| diagnostic.message.contains(message))
}

fn code_of(diagnostic: &Diagnostic) -> Option<&str> {
    match &diagnostic.code {
        Some(NumberOrString::String(code)) => Some(code),
        Some(NumberOrString::Number(_)) | None => None,
    }
}

fn describe_expectation(tag: &Tag) -> String {
    let mut parts = Vec::new();
    if let Some(code) = tag.attr("code") {
        parts.push(format!("code={code}"));
    }
    if let Some(message) = tag.attr("message") {
        parts.push(format!("message~={message:?}"));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!(" [{}]", parts.join(", "))
    }
}

fn describe(diagnostic: &Diagnostic) -> String {
    let range = diagnostic.range;
    format!(
        "{}:{}-{}:{} [{}] {:?}",
        range.start.line,
        range.start.character,
        range.end.line,
        range.end.character,
        code_of(diagnostic).unwrap_or("<no-code>"),
        diagnostic.message
    )
}
