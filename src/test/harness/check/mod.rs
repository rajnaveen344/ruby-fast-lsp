//! Tag-driven assertions for inline fixtures.
//!
//! `check()` opens one fixture; `check_multi_file()` opens several and treats
//! them as one scenario. Each tag kind has one runner; the runner compares the
//! production response exactly. See [`super::fixture`] for the tag syntax.
//!
//! | Tag | Position | Asserts |
//! |-----|----------|---------|
//! | `<def>`, `<ref>`, `<impl>` | range, any file | exact location set for the request at `$0` |
//! | `<def none>`, `<impl none>`, ... | point | the request at `$0` returns nothing |
//! | `<incoming>`, `<outgoing>` | range, any file | exact caller/callee definition set at `$0` |
//! | `<rename to="x">`, `<rename>` | range, any file | exact edit set for renaming at the `to` tag |
//! | `<type label kind>` | point | exact inferred type at the point |
//! | `<hint label tooltip>`, `<hint none>` | point / range | exact hint label (and tooltip) at the point |
//! | `<lens title>`, `<lens none>` | point / range | exact lens title on the line |
//! | `<hover label contains>` | point | hover shows the label as a type or line; `contains` is free text |
//! | `<err code message>`, `<warn ...>`, `<... none>` | range | diagnostic with the exact range |
//! | `<th supertypes subtypes>` | at `$0` | exact supertype/subtype name sets |
//! | `<complete items excludes>` | at `$0` | exact labels present / absent |
//!
//! A fixture must assert something, and `$0` must be used by a cursor tag.

mod completion;
mod diagnostics;
mod hierarchy;
mod navigation;
mod presentation;
mod types;

#[cfg(test)]
mod tests;

use std::collections::BTreeSet;

use tower_lsp::lsp_types::{
    Location, Position, TextDocumentIdentifier, TextDocumentPositionParams, Url,
};

use super::fake_editor::FakeEditor;
use super::fixture::{parse_fixture, Fixture, Tag, TagKind};
use crate::server::RubyLanguageServer;

/// Open a single fixture as `inline_test.rb` and run its assertions.
pub async fn check(fixture_text: &str) {
    check_multi_file(&[("inline_test.rb", fixture_text)]).await;
}

/// Open every fixture, then run the assertions of all of them as one scenario.
///
/// Location tags (`<def>`, `<ref>`, ...) may live in any file; at most one file
/// carries the `$0` cursor.
pub async fn check_multi_file(files: &[(&str, &str)]) {
    assert!(
        !files.is_empty(),
        "check_multi_file requires at least one file"
    );
    let mut editor = FakeEditor::new().await;
    let fixtures: Vec<FixtureFile> = files
        .iter()
        .map(|(name, text)| FixtureFile {
            uri: FakeEditor::filename_to_uri(name),
            fixture: parse_fixture(text),
        })
        .collect();
    for ((name, _), file) in files.iter().zip(&fixtures) {
        editor.open(name, &file.fixture.source).await;
    }
    run_fixture_checks(editor.server(), &fixtures).await;
}

/// A parsed fixture and the URI it was opened at.
pub(super) struct FixtureFile {
    pub uri: Url,
    pub fixture: Fixture,
}

/// Run every assertion in an already-opened set of fixtures.
pub(super) async fn run_fixture_checks(server: &RubyLanguageServer, files: &[FixtureFile]) {
    assert!(
        files.iter().any(|file| file.fixture.has_markers()),
        "fixture asserts nothing: add tags, or assert the expected result directly"
    );
    let scenario = Scenario::new(files);

    let cursor_kinds: BTreeSet<TagKind> = scenario
        .tags()
        .map(|(_, tag)| tag.kind)
        .filter(|kind| kind.spec().needs_cursor)
        .collect();
    match (&scenario.cursor, cursor_kinds.is_empty()) {
        (None, false) => panic!(
            "fixture uses {:?}, which need a `$0` cursor, but has none",
            cursor_kinds
        ),
        (Some(cursor), true) => panic!(
            "fixture has a `$0` cursor at {:?} but no tag that uses it",
            cursor.range.start
        ),
        _ => {}
    }

    if let Some(cursor) = &scenario.cursor {
        for kind in &cursor_kinds {
            match kind {
                TagKind::Def | TagKind::Ref | TagKind::Impl => {
                    navigation::check_locations(server, cursor, *kind, &scenario.locations(*kind))
                        .await
                }
                TagKind::Incoming | TagKind::Outgoing => {
                    navigation::check_calls(server, cursor, *kind, &scenario.locations(*kind)).await
                }
                TagKind::Th => {
                    for (_, tag) in scenario.tags().filter(|(_, tag)| tag.kind == TagKind::Th) {
                        hierarchy::check_type_hierarchy(server, cursor, tag).await;
                    }
                }
                TagKind::Complete => {
                    for (_, tag) in scenario
                        .tags()
                        .filter(|(_, tag)| tag.kind == TagKind::Complete)
                    {
                        completion::check_completion(server, cursor, tag).await;
                    }
                }
                TagKind::Rename
                | TagKind::Type
                | TagKind::Hint
                | TagKind::Lens
                | TagKind::Hover
                | TagKind::Err
                | TagKind::Warn => unreachable!("{kind:?} does not use the cursor"),
            }
        }
    }

    let renames: Vec<(&Url, &Tag)> = scenario
        .tags()
        .filter(|(_, tag)| tag.kind == TagKind::Rename)
        .collect();
    if !renames.is_empty() {
        navigation::check_rename(server, &renames).await;
    }

    for file in files {
        let uri = &file.uri;
        let tags = |kind| file.fixture.tags_of(kind).collect::<Vec<_>>();
        let type_tags = tags(TagKind::Type);
        if !type_tags.is_empty() {
            types::check_types(server, uri, &file.fixture.source, &type_tags);
        }
        let hint_tags = tags(TagKind::Hint);
        if !hint_tags.is_empty() {
            presentation::check_hints(server, uri, &hint_tags).await;
        }
        let lens_tags = tags(TagKind::Lens);
        if !lens_tags.is_empty() {
            presentation::check_lenses(server, uri, &lens_tags).await;
        }
        for tag in tags(TagKind::Hover) {
            presentation::check_hover(server, uri, tag).await;
        }
        let err_tags = tags(TagKind::Err);
        let warn_tags = tags(TagKind::Warn);
        if !err_tags.is_empty() || !warn_tags.is_empty() {
            diagnostics::check_diagnostics(server, uri, &err_tags, &warn_tags);
        }
    }
}

/// All fixtures of one check, viewed together.
struct Scenario<'a> {
    files: &'a [FixtureFile],
    cursor: Option<Location>,
}

impl<'a> Scenario<'a> {
    fn new(files: &'a [FixtureFile]) -> Self {
        let mut cursors = files.iter().filter_map(|file| {
            file.fixture.cursor.map(|position| {
                Location::new(
                    file.uri.clone(),
                    tower_lsp::lsp_types::Range::new(position, position),
                )
            })
        });
        let cursor = cursors.next();
        assert!(
            cursors.next().is_none(),
            "only one fixture file may contain `$0`"
        );
        Self { files, cursor }
    }

    fn tags(&self) -> impl Iterator<Item = (&'a Url, &'a Tag)> {
        self.files
            .iter()
            .flat_map(|file| file.fixture.tags.iter().map(move |tag| (&file.uri, tag)))
    }

    /// Expected locations of a location tag kind; `<kind none>` expects none.
    fn locations(&self, kind: TagKind) -> Vec<Location> {
        let tags: Vec<_> = self.tags().filter(|(_, tag)| tag.kind == kind).collect();
        let none = tags.iter().filter(|(_, tag)| tag.none).count();
        assert!(
            none == 0 || tags.len() == 1,
            "<{} none> expects an empty result and cannot be combined with other <{}> tags",
            kind.spec().name,
            kind.spec().name
        );
        tags.into_iter()
            .filter(|(_, tag)| !tag.none)
            .map(|(uri, tag)| Location::new(uri.clone(), tag.range))
            .collect()
    }
}

fn position_params(uri: &Url, position: Position) -> TextDocumentPositionParams {
    TextDocumentPositionParams {
        text_document: TextDocumentIdentifier { uri: uri.clone() },
        position,
    }
}

/// Assert two location lists are equal as multisets.
fn assert_same_locations(what: &str, mut actual: Vec<Location>, mut expected: Vec<Location>) {
    let key = |location: &Location| {
        (
            location.uri.to_string(),
            location.range.start.line,
            location.range.start.character,
            location.range.end.line,
            location.range.end.character,
        )
    };
    actual.sort_by_key(key);
    expected.sort_by_key(key);
    assert!(
        actual == expected,
        "{what} mismatch.\nExpected: {}\nActual:   {}",
        describe_locations(&expected),
        describe_locations(&actual)
    );
}

fn describe_locations(locations: &[Location]) -> String {
    let items: Vec<String> = locations
        .iter()
        .map(|location| {
            let file = location.uri.path().rsplit('/').next().unwrap_or_default();
            let range = location.range;
            format!(
                "{file}:{}:{}-{}:{}",
                range.start.line, range.start.character, range.end.line, range.end.character
            )
        })
        .collect();
    format!("[{}]", items.join(", "))
}

/// Whether a position lies within a range, both ends inclusive.
fn position_in_range(position: Position, range: &tower_lsp::lsp_types::Range) -> bool {
    range.start <= position && position <= range.end
}

/// Whether two ranges share at least one position.
fn ranges_overlap(a: &tower_lsp::lsp_types::Range, b: &tower_lsp::lsp_types::Range) -> bool {
    a.start <= b.end && b.start <= a.end
}
