use crate::test::simulation::graph::NamespaceKind;
use crate::test::simulation::ruby_gen::SourcePos;
use tower_lsp::lsp_types::{
    Diagnostic, DiagnosticSeverity, GotoDefinitionResponse, Hover, HoverContents, Location,
    MarkedString, NumberOrString, Position,
};

pub(super) fn assert_definition_precedence(
    locations: &[Location],
    before: &SourcePos,
    after: &SourcePos,
) {
    let before_index = locations
        .iter()
        .position(|location| location_matches(location, before))
        .expect("precedence guard requires the independently expected higher-priority target");
    let after_index = locations
        .iter()
        .position(|location| location_matches(location, after))
        .expect("precedence guard requires the independently expected lower-priority target");
    assert!(
        before_index < after_index,
        "simulation definition lookup precedence was reversed: {:?} must precede {:?}; got {:?}",
        before,
        after,
        locations
    );
}

// Compare semantic destinations independently of the lookup-precedence assertions above.
// Preserve response kind, complete ranges (including links' origins), and duplicates.
pub(super) fn definition_observation(response: Option<GotoDefinitionResponse>) -> Vec<String> {
    let (kind, values) = match response {
        None => ("absent", Vec::new()),
        Some(GotoDefinitionResponse::Scalar(location)) => {
            ("scalar", sorted_lsp_values(vec![location]))
        }
        Some(GotoDefinitionResponse::Array(locations)) => ("array", sorted_lsp_values(locations)),
        Some(GotoDefinitionResponse::Link(links)) => ("links", sorted_lsp_values(links)),
    };
    std::iter::once(kind.to_owned()).chain(values).collect()
}

pub(super) fn sorted_lsp_values<T: serde::Serialize>(values: Vec<T>) -> Vec<String> {
    let mut serialized = values
        .into_iter()
        .map(|value| {
            serde_json::to_string(&value).expect(
                "INVARIANT VIOLATED: a public LSP observation could not be serialized. This is a bug because simulation compares JSON-compatible protocol values. Fix: inspect the response serialization.",
            )
        })
        .collect::<Vec<_>>();
    serialized.sort();
    serialized
}

pub(super) fn location_matches(loc: &Location, pos: &SourcePos) -> bool {
    loc.uri == crate::test::harness::fixture_uri(&pos.file) && loc.range.start.line == pos.line
}

pub(super) fn diagnostic_is_unresolved_method(diagnostic: &Diagnostic) -> bool {
    diagnostic.severity == Some(DiagnosticSeverity::WARNING)
        && matches!(
            &diagnostic.code,
            Some(NumberOrString::String(code)) if code == "unresolved-method"
        )
}

pub(super) fn diagnostic_is_unresolved_constant(diagnostic: &Diagnostic) -> bool {
    diagnostic.severity == Some(DiagnosticSeverity::ERROR)
        && matches!(
            &diagnostic.code,
            Some(NumberOrString::String(code)) if code == "unresolved-constant"
        )
}

pub(super) fn constant_name(fqn: &str) -> &str {
    fqn.rsplit("::").next().unwrap_or_else(|| {
        panic!(
            "INVARIANT VIOLATED: constant FQN `{}` has no name segment. This is a bug because generated constants must be fully-qualified. Fix: validate constant refs in the project model.",
            fqn
        )
    })
}

pub(super) fn namespace_hover_label(fqn: &str, kind: NamespaceKind) -> String {
    match kind {
        NamespaceKind::Class | NamespaceKind::Module => namespace_name(fqn).to_string(),
    }
}

fn namespace_name(fqn: &str) -> &str {
    fqn.rsplit("::").next().unwrap_or_else(|| {
        panic!(
            "INVARIANT VIOLATED: namespace FQN `{}` has no name segment. This is a bug because generated namespaces must be fully-qualified. Fix: validate namespace refs in the project model.",
            fqn
        )
    })
}

pub(super) fn hover_text(hover: &Hover) -> String {
    match &hover.contents {
        HoverContents::Scalar(text) => marked_string_text(text),
        HoverContents::Array(items) => items
            .iter()
            .map(marked_string_text)
            .collect::<Vec<_>>()
            .join("\n"),
        HoverContents::Markup(markup) => markup.value.clone(),
    }
}

fn marked_string_text(text: &MarkedString) -> String {
    match text {
        MarkedString::String(value) => value.clone(),
        MarkedString::LanguageString(value) => value.value.clone(),
    }
}

pub(super) fn diagnostic_text(content: &str, diagnostic: &Diagnostic) -> String {
    let start = position_to_byte_offset(content, diagnostic.range.start);
    let end = position_to_byte_offset(content, diagnostic.range.end);
    content[start..end].to_string()
}

fn position_to_byte_offset(content: &str, position: Position) -> usize {
    let mut offset = 0;
    for (line_idx, line) in content.lines().enumerate() {
        if line_idx as u32 == position.line {
            return offset + position.character as usize;
        }
        offset += line.len() + 1;
    }
    content.len()
}
