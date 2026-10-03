//! Inlay hints, code lenses, and hovers.

use std::collections::BTreeMap;

use tower_lsp::lsp_types::{
    CodeLensParams, HoverContents, HoverParams, InlayHint, InlayHintParams, MarkedString,
    PartialResultParams, Position, Range, TextDocumentIdentifier, Url, WorkDoneProgressParams,
};

use super::{position_in_range, position_params, ranges_overlap};
use crate::features::presentation::{code_lens, hover, inlay_hints};
use crate::server::Server;
use crate::test::harness::fixture::Tag;
use crate::test::harness::{get_hint_label, get_hint_tooltip};

/// `<hint label="..." tooltip="...">`: the hints at exactly the tagged position
/// are exactly the tagged ones. A label may omit the `: `/` -> ` prefix.
/// `<hint none>` forbids hints inside its range.
pub(super) async fn check_hints(server: &Server, uri: &Url, tags: &[&Tag]) {
    let hints = inlay_hints::handle(
        server,
        InlayHintParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            range: Range::new(Position::new(0, 0), Position::new(u32::MAX, 0)),
            work_done_progress_params: WorkDoneProgressParams::default(),
        },
    )
    .await
    .expect("inlay hint request failed")
    .unwrap_or_default();

    for tag in tags.iter().filter(|tag| tag.none) {
        let inside: Vec<String> = hints
            .iter()
            .filter(|hint| position_in_range(hint.position, &tag.range))
            .map(describe_hint)
            .collect();
        assert!(
            inside.is_empty(),
            "expected no inlay hints in {:?}, got: {inside:?}",
            tag.range
        );
    }

    let mut expected_by_position: BTreeMap<(u32, u32), Vec<&Tag>> = BTreeMap::new();
    for tag in tags.iter().filter(|tag| !tag.none) {
        let start = tag.range.start;
        expected_by_position
            .entry((start.line, start.character))
            .or_default()
            .push(tag);
    }
    for ((line, character), expected) in expected_by_position {
        let mut actual: Vec<&InlayHint> = hints
            .iter()
            .filter(|hint| hint.position == Position::new(line, character))
            .collect();
        let describe_actual = actual
            .iter()
            .map(|hint| describe_hint(hint))
            .collect::<Vec<_>>();
        for tag in &expected {
            let index = actual.iter().position(|hint| hint_matches(hint, tag)).unwrap_or_else(|| {
                panic!(
                    "expected inlay hint {:?}{} at {line}:{character}.\nHints there: {describe_actual:?}\nAll hints: {:?}",
                    tag.attr("label").unwrap_or_default(),
                    tag.attr("tooltip").map(|tooltip| format!(" with tooltip {tooltip:?}")).unwrap_or_default(),
                    hints.iter().map(describe_hint).collect::<Vec<_>>()
                )
            });
            actual.remove(index);
        }
        assert!(
            actual.is_empty(),
            "unexpected extra inlay hints at {line}:{character}: {:?}",
            actual
                .iter()
                .map(|hint| describe_hint(hint))
                .collect::<Vec<_>>()
        );
    }
}

fn hint_matches(hint: &InlayHint, tag: &Tag) -> bool {
    let label = get_hint_label(hint);
    let expected = tag.attr("label").unwrap_or_default();
    let label_matches = label == expected || hint_type_text(&label) == expected;
    label_matches
        && tag
            .attr("tooltip")
            .is_none_or(|tooltip| get_hint_tooltip(hint) == Some(tooltip))
}

/// The type text of a `: T` or ` -> T` hint label.
fn hint_type_text(label: &str) -> &str {
    let trimmed = label.trim_start();
    trimmed
        .strip_prefix(':')
        .or_else(|| trimmed.strip_prefix("->"))
        .map(str::trim_start)
        .unwrap_or(label)
}

fn describe_hint(hint: &InlayHint) -> String {
    format!(
        "{}:{} {:?}",
        hint.position.line,
        hint.position.character,
        get_hint_label(hint)
    )
}

/// `<lens title="...">`: a lens on the tagged line has exactly this title.
/// `<lens none>` forbids lenses inside its range.
pub(super) async fn check_lenses(server: &Server, uri: &Url, tags: &[&Tag]) {
    let lenses = code_lens::handle(
        server,
        CodeLensParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    )
    .await
    .expect("code lens request failed")
    .unwrap_or_default();
    let described: Vec<(u32, Option<&str>)> = lenses
        .iter()
        .map(|lens| {
            (
                lens.range.start.line,
                lens.command.as_ref().map(|command| command.title.as_str()),
            )
        })
        .collect();

    for tag in tags.iter().filter(|tag| tag.none) {
        let inside: Vec<_> = lenses
            .iter()
            .filter(|lens| ranges_overlap(&lens.range, &tag.range))
            .map(|lens| lens.command.as_ref().map(|command| command.title.as_str()))
            .collect();
        assert!(
            inside.is_empty(),
            "expected no code lenses in {:?}, got: {inside:?}",
            tag.range
        );
    }

    let mut unmatched = described.clone();
    for tag in tags.iter().filter(|tag| !tag.none) {
        let title = tag.attr("title").unwrap_or_default();
        let line = tag.range.start.line;
        let index = unmatched
            .iter()
            .position(|lens| *lens == (line, Some(title)))
            .unwrap_or_else(|| {
                panic!("expected code lens {title:?} on line {line}.\nLenses: {described:?}")
            });
        unmatched.remove(index);
    }
}

/// `<hover label="T" contains="text">`: the hover at the point shows `label` as
/// a complete type, line, or line prefix, and contains the free text `contains`.
pub(super) async fn check_hover(server: &Server, uri: &Url, tag: &Tag) {
    assert!(
        tag.attr("label").is_some() || tag.attr("contains").is_some(),
        "<hover> needs `label` or `contains`"
    );
    let position = tag.range.start;
    let hover = hover::handle(
        server,
        HoverParams {
            text_document_position_params: position_params(uri, position),
            work_done_progress_params: WorkDoneProgressParams::default(),
        },
    )
    .await
    .expect("hover request failed")
    .unwrap_or_else(|| panic!("expected a hover at {position:?}"));
    let content = hover_text(&hover.contents);

    if let Some(label) = tag.attr("label") {
        assert!(
            hover_shows(&content, label),
            "hover at {position:?} does not show {label:?} as a type or line.\nHover:\n{content}"
        );
    }
    if let Some(text) = tag.attr("contains") {
        assert!(
            content.contains(text),
            "hover at {position:?} does not contain {text:?}.\nHover:\n{content}"
        );
    }
}

fn hover_text(contents: &HoverContents) -> String {
    let marked = |text: &MarkedString| match text {
        MarkedString::String(text) => text.clone(),
        MarkedString::LanguageString(text) => text.value.clone(),
    };
    match contents {
        HoverContents::Scalar(text) => marked(text),
        HoverContents::Array(texts) => texts.iter().map(marked).collect::<Vec<_>>().join("\n"),
        HoverContents::Markup(markup) => markup.value.clone(),
    }
}

/// Whether `label` is a whole unit of the hover: the type of a
/// `name: Type # kind` binding line, the return type of a `def name -> Type`
/// line, a complete line (ignoring indentation and a trailing comma), or the
/// part of a line before its first `: `.
fn hover_shows(content: &str, label: &str) -> bool {
    content.lines().any(|line| {
        let line = line.trim();
        let entry = line.strip_suffix(',').unwrap_or(line);
        let binding_type = line
            .split_once(": ")
            .and_then(|(_, rest)| rest.rsplit_once(" # "))
            .map(|(ty, _)| ty);
        let return_type = line
            .strip_prefix("def ")
            .and_then(|signature| signature.split_once(" -> "))
            .map(|(_, ty)| ty);
        let prefix = line.split_once(": ").map(|(prefix, _)| prefix);
        [Some(line), Some(entry), binding_type, return_type, prefix].contains(&Some(label))
    })
}
