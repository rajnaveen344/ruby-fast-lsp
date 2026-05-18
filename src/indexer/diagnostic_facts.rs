use crate::server::RubyLanguageServer;
use crate::types::ruby_document::RubyDocument;
use crate::types::unresolved_index::UnresolvedEntry;
use ruby_analysis_core::{DiagnosticFact, DiagnosticSeverity, TextRange};
use tower_lsp::lsp_types::Range;

pub fn replace_unresolved_diagnostic_facts_for_document<'a>(
    server: &RubyLanguageServer,
    document: &RubyDocument,
    entries: impl IntoIterator<Item = &'a UnresolvedEntry>,
) {
    let facts = entries
        .into_iter()
        .map(|entry| diagnostic_fact_from_unresolved_entry(document, entry))
        .collect::<Vec<_>>();
    server
        .analysis_engine
        .lock()
        .replace_diagnostic_facts_for_file(document.analysis_file_id(), facts);
}

fn diagnostic_fact_from_unresolved_entry(
    document: &RubyDocument,
    entry: &UnresolvedEntry,
) -> DiagnosticFact {
    match entry {
        UnresolvedEntry::Constant { name, location, .. } => DiagnosticFact::new(
            text_range_from_lsp_range(document, location.range, "unresolved constant"),
            DiagnosticSeverity::Error,
            "unresolved-constant",
            format!("Unresolved constant `{}`", name),
        ),
        UnresolvedEntry::Method {
            name,
            receiver_type,
            location,
            suggestion,
        } => {
            if let Some(crate::inferrer::r#type::ruby::RubyType::Unknown) = receiver_type {
                DiagnosticFact::new(
                    text_range_from_lsp_range(document, location.range, "unknown receiver type"),
                    DiagnosticSeverity::Warning,
                    "unknown-receiver-type",
                    format!(
                        "Cannot determine receiver type for method call `{}`. Definition may be imprecise.",
                        name
                    ),
                )
            } else {
                let mut message = match receiver_type {
                    Some(recv) => format!("Unresolved method `{}` on `{}`", name, recv),
                    None => format!("Unresolved method `{}`", name),
                };
                if let Some(suggestion) = suggestion {
                    message.push_str(&format!(". Did you mean `{}`?", suggestion));
                }
                DiagnosticFact::new(
                    text_range_from_lsp_range(document, location.range, "unresolved method"),
                    DiagnosticSeverity::Warning,
                    "unresolved-method",
                    message,
                )
            }
        }
        UnresolvedEntry::UnknownKwarg {
            method,
            kwarg,
            suggestion,
            location,
        } => {
            let mut message = format!("Unknown keyword argument `{}:` for `{}`", kwarg, method);
            if let Some(suggestion) = suggestion {
                message.push_str(&format!(". Did you mean `{}:`?", suggestion));
            }
            DiagnosticFact::new(
                text_range_from_lsp_range(document, location.range, "unknown keyword argument"),
                DiagnosticSeverity::Warning,
                "unknown-kwarg",
                message,
            )
        }
        UnresolvedEntry::WrongArity {
            name,
            expected_min,
            expected_max,
            actual,
            location,
        } => {
            let expected = match expected_max {
                Some(max) if max == expected_min => format!("{}", expected_min),
                Some(max) => format!("{}..{}", expected_min, max),
                None => format!("{}+", expected_min),
            };
            DiagnosticFact::new(
                text_range_from_lsp_range(document, location.range, "wrong arity"),
                DiagnosticSeverity::Warning,
                "wrong-arity",
                format!(
                    "Wrong number of arguments for `{}` (expected {}, got {})",
                    name, expected, actual
                ),
            )
        }
        UnresolvedEntry::MissingKwarg {
            method,
            missing,
            location,
        } => {
            let kw_list = missing
                .iter()
                .map(|kwarg| format!("`{}:`", kwarg))
                .collect::<Vec<_>>()
                .join(", ");
            DiagnosticFact::new(
                text_range_from_lsp_range(document, location.range, "missing keyword argument"),
                DiagnosticSeverity::Warning,
                "missing-kwarg",
                format!(
                    "Missing required keyword argument(s) for `{}`: {}",
                    method, kw_list
                ),
            )
        }
        UnresolvedEntry::RaiseNonException { arg_repr, location } => DiagnosticFact::new(
            text_range_from_lsp_range(document, location.range, "raise non exception"),
            DiagnosticSeverity::Warning,
            "raise-non-exception",
            format!(
                "`raise` argument `{}` is not an Exception subclass",
                arg_repr
            ),
        ),
        UnresolvedEntry::BadSplat {
            operator,
            arg_repr,
            expected,
            location,
        } => DiagnosticFact::new(
            text_range_from_lsp_range(document, location.range, "bad splat"),
            DiagnosticSeverity::Warning,
            "bad-splat",
            format!(
                "`{}{}` expected {} but got non-{} value",
                operator, arg_repr, expected, expected
            ),
        ),
    }
}

fn text_range_from_lsp_range(
    document: &RubyDocument,
    range: Range,
    diagnostic_kind: &str,
) -> TextRange {
    let start_byte = u32::try_from(document.position_to_offset(range.start)).unwrap_or_else(|_| {
        panic!(
            "INVARIANT VIOLATED: {diagnostic_kind} diagnostic start offset exceeded u32. \
             This is a bug because ruby-analysis-core TextRange currently stores u32 offsets. \
             Fix: widen TextRange offsets before indexing files larger than u32::MAX bytes."
        )
    });
    let end_byte = u32::try_from(document.position_to_offset(range.end)).unwrap_or_else(|_| {
        panic!(
            "INVARIANT VIOLATED: {diagnostic_kind} diagnostic end offset exceeded u32. \
             This is a bug because ruby-analysis-core TextRange currently stores u32 offsets. \
             Fix: widen TextRange offsets before indexing files larger than u32::MAX bytes."
        )
    });
    TextRange::new(document.analysis_file_id(), start_byte, end_byte)
}
