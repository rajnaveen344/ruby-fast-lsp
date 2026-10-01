//! Inline fixture parsing: the `$0` cursor and `<tag attr="...">` markers.
//!
//! Positions are LSP positions: zero-based lines and UTF-16 columns in the
//! clean source. Every tag is validated against [`TagKind::spec`], so a typo in
//! a tag or attribute name fails the test instead of silently checking nothing.

use crate::invariant::ExpectInvariant;
use std::collections::BTreeMap;

use tower_lsp::lsp_types::{Position, Range};

/// Cursor marker indicating where a cursor-based request is issued.
pub const CURSOR_MARKER: &str = "$0";

/// Every assertion tag understood by `check()`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TagKind {
    Def,
    Ref,
    Impl,
    Incoming,
    Outgoing,
    Rename,
    Type,
    Hint,
    Lens,
    Hover,
    Err,
    Warn,
    Th,
    Complete,
}

/// Whether a tag wraps text, marks a point, or may do either.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    /// `<tag>text</tag>`.
    Range,
    /// `<tag attr="...">`.
    Point,
    /// Closed tags wrap text; unclosed tags mark a point.
    Either,
}

/// Static description of one tag kind.
pub struct TagSpec {
    pub kind: TagKind,
    pub name: &'static str,
    shape: Shape,
    /// Attributes the tag accepts.
    attributes: &'static [&'static str],
    /// Attributes every non-`none` tag must have.
    required: &'static [&'static str],
    /// Shape of `<tag none>`, if the tag accepts the `none` keyword: a range
    /// for scoped absence, or a point for "the request returns nothing".
    none_shape: Option<Shape>,
    /// Whether the check issues its request at the fixture cursor.
    pub needs_cursor: bool,
}

const SPECS: &[TagSpec] = &[
    location_spec(TagKind::Def, "def"),
    location_spec(TagKind::Ref, "ref"),
    location_spec(TagKind::Impl, "impl"),
    location_spec(TagKind::Incoming, "incoming"),
    location_spec(TagKind::Outgoing, "outgoing"),
    TagSpec {
        kind: TagKind::Rename,
        name: "rename",
        shape: Shape::Range,
        attributes: &["to"],
        required: &[],
        none_shape: None,
        needs_cursor: false,
    },
    TagSpec {
        kind: TagKind::Type,
        name: "type",
        shape: Shape::Point,
        attributes: &["label", "kind"],
        required: &["label"],
        none_shape: None,
        needs_cursor: false,
    },
    TagSpec {
        kind: TagKind::Hint,
        name: "hint",
        shape: Shape::Point,
        attributes: &["label", "tooltip"],
        required: &["label"],
        none_shape: Some(Shape::Range),
        needs_cursor: false,
    },
    TagSpec {
        kind: TagKind::Lens,
        name: "lens",
        shape: Shape::Point,
        attributes: &["title"],
        required: &["title"],
        none_shape: Some(Shape::Range),
        needs_cursor: false,
    },
    TagSpec {
        kind: TagKind::Hover,
        name: "hover",
        shape: Shape::Point,
        attributes: &["label", "contains"],
        required: &[],
        none_shape: None,
        needs_cursor: false,
    },
    TagSpec {
        kind: TagKind::Err,
        name: "err",
        shape: Shape::Range,
        attributes: &["code", "message"],
        required: &[],
        none_shape: Some(Shape::Range),
        needs_cursor: false,
    },
    TagSpec {
        kind: TagKind::Warn,
        name: "warn",
        shape: Shape::Range,
        attributes: &["code", "message"],
        required: &[],
        none_shape: Some(Shape::Range),
        needs_cursor: false,
    },
    TagSpec {
        kind: TagKind::Th,
        name: "th",
        shape: Shape::Either,
        attributes: &["supertypes", "subtypes"],
        required: &[],
        none_shape: None,
        needs_cursor: true,
    },
    TagSpec {
        kind: TagKind::Complete,
        name: "complete",
        shape: Shape::Point,
        attributes: &["items", "excludes"],
        required: &[],
        none_shape: None,
        needs_cursor: true,
    },
];

const fn location_spec(kind: TagKind, name: &'static str) -> TagSpec {
    TagSpec {
        kind,
        name,
        shape: Shape::Range,
        attributes: &[],
        required: &[],
        none_shape: Some(Shape::Point),
        needs_cursor: true,
    }
}

impl TagKind {
    pub fn spec(self) -> &'static TagSpec {
        SPECS
            .iter()
            .find(|spec| spec.kind == self)
            .expect_invariant(
                "TagKind has no TagSpec",
                "every tag kind must be parseable",
                "add the kind to SPECS",
            )
    }

    fn from_name(name: &str) -> Option<Self> {
        SPECS
            .iter()
            .find(|spec| spec.name == name)
            .map(|spec| spec.kind)
    }
}

/// One parsed assertion tag.
#[derive(Debug, Clone)]
pub struct Tag {
    pub kind: TagKind,
    pub range: Range,
    pub attributes: BTreeMap<String, String>,
    /// `<tag none>`: assert the absence of results inside the range.
    pub none: bool,
}

impl Tag {
    fn shape(&self) -> Shape {
        let spec = self.kind.spec();
        if self.none {
            spec.none_shape.expect_invariant(
                "`none` parsed for a tag without none_shape",
                "parse_attributes accepts `none` only for such tags",
                "reject `none` in parse_attributes",
            )
        } else {
            spec.shape
        }
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        invariant!(
            self.kind.spec().attributes.contains(&name),
            what = "harness read undeclared attribute `{name}` of <{}>",
            why = "parse_fixture rejects undeclared attributes",
            fix = "declare it in the tag's TagSpec",
            self.kind.spec().name,
            name = name,
        );
        self.attributes.get(name).map(String::as_str)
    }

    /// Comma-separated attribute values with surrounding whitespace removed.
    pub fn list(&self, name: &str) -> Vec<&str> {
        self.attr(name)
            .map(|value| {
                value
                    .split(',')
                    .map(str::trim)
                    .filter(|item| !item.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// A parsed inline fixture.
#[derive(Debug, Clone)]
pub struct Fixture {
    /// Ruby source with every marker removed.
    pub source: String,
    pub cursor: Option<Position>,
    pub tags: Vec<Tag>,
}

impl Fixture {
    pub fn has_markers(&self) -> bool {
        self.cursor.is_some() || !self.tags.is_empty()
    }

    pub fn tags_of(&self, kind: TagKind) -> impl Iterator<Item = &Tag> {
        self.tags.iter().filter(move |tag| tag.kind == kind)
    }
}

/// Remove every marker, returning the source the editor should see.
pub fn strip_markers(text: &str) -> String {
    parse_fixture(text).source
}

/// Parse a fixture, panicking on malformed or unknown markers.
pub fn parse_fixture(text: &str) -> Fixture {
    let mut source = String::with_capacity(text.len());
    let mut position = Position::new(0, 0);
    let mut cursor = None;
    let mut tags = Vec::new();
    let mut open: Vec<Tag> = Vec::new();
    let mut rest = text;

    while let Some(ch) = rest.chars().next() {
        if let Some(after) = rest.strip_prefix(CURSOR_MARKER) {
            assert!(
                cursor.is_none(),
                "fixture has more than one `{CURSOR_MARKER}` cursor marker"
            );
            cursor = Some(position);
            rest = after;
            continue;
        }
        if ch == '<' {
            if let Some((kind, after)) = parse_close_tag(rest) {
                let index = open
                    .iter()
                    .rposition(|tag| tag.kind == kind)
                    .unwrap_or_else(|| {
                        panic!(
                            "fixture has </{}> without a matching opening tag",
                            kind.spec().name
                        )
                    });
                let mut tag = open.remove(index);
                tag.range.end = position;
                tags.push(tag);
                rest = after;
                continue;
            }
            if let Some((tag, after)) = parse_open_tag(rest, position) {
                let closes = match tag.shape() {
                    Shape::Range | Shape::Either => true,
                    Shape::Point => false,
                };
                if closes {
                    open.push(tag);
                } else {
                    tags.push(tag);
                }
                rest = after;
                continue;
            }
        }
        source.push(ch);
        advance(&mut position, ch);
        rest = &rest[ch.len_utf8()..];
    }

    for tag in open {
        let spec = tag.kind.spec();
        assert!(
            tag.shape() == Shape::Either,
            "fixture has an unclosed <{}> tag at {:?}",
            spec.name,
            tag.range.start
        );
        tags.push(tag);
    }
    tags.sort_by_key(|tag| (tag.range.start.line, tag.range.start.character));

    Fixture {
        source,
        cursor,
        tags,
    }
}

fn advance(position: &mut Position, ch: char) {
    if ch == '\n' {
        position.line += 1;
        position.character = 0;
    } else {
        position.character += ch.len_utf16() as u32;
    }
}

/// `</name>` for a known tag.
fn parse_close_tag(text: &str) -> Option<(TagKind, &str)> {
    let body = text.strip_prefix("</")?;
    let end = body.find('>')?;
    let name = &body[..end];
    if let Some(kind) = TagKind::from_name(name) {
        return Some((kind, &body[end + 1..]));
    }
    reject_unknown_tag(name);
    None
}

/// `<name attr="value" none>` for a known tag.
fn parse_open_tag(text: &str, position: Position) -> Option<(Tag, &str)> {
    let body = text.strip_prefix('<')?;
    let name_len = body
        .find(|ch: char| !(ch.is_ascii_lowercase() || ch == '_'))
        .unwrap_or(body.len());
    let name = &body[..name_len];
    let after_name = &body[name_len..];
    if !(after_name.starts_with('>') || after_name.starts_with(' ')) {
        return None;
    }
    let Some(kind) = TagKind::from_name(name) else {
        if looks_like_tag(after_name) {
            reject_unknown_tag(name);
        }
        return None;
    };
    let end = tag_end(after_name)
        .unwrap_or_else(|| panic!("fixture has an unterminated <{name} tag at {position:?}"));
    let (attributes, none) = parse_attributes(kind, &after_name[..end]);
    let tag = Tag {
        kind,
        range: Range::new(position, position),
        attributes,
        none,
    };
    Some((tag, &after_name[end + 1..]))
}

/// Byte index of the `>` that ends a tag, ignoring `>` inside quotes.
fn tag_end(text: &str) -> Option<usize> {
    let mut quoted = false;
    for (index, ch) in text.char_indices() {
        match ch {
            '"' => quoted = !quoted,
            '>' if !quoted => return Some(index),
            '\n' if !quoted => return None,
            _ => {}
        }
    }
    None
}

/// `>` directly, or a first ` key="` attribute, as an assertion tag body would be.
fn looks_like_tag(after_name: &str) -> bool {
    let Some(end) = tag_end(after_name) else {
        return false;
    };
    let body = after_name[..end].trim_start();
    end == 0
        || body == "none"
        || body.split_once("=\"").is_some_and(|(key, _)| {
            !key.is_empty()
                && key
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
        })
}

/// Names that look like assertion tags but are not supported fail loudly.
/// Ruby code rarely contains `<lowercase>`; HTML templates are not fixtures.
fn reject_unknown_tag(name: &str) {
    assert!(
        name.is_empty(),
        "fixture contains unknown tag <{name}>; supported tags: {}",
        SPECS
            .iter()
            .map(|spec| spec.name)
            .collect::<Vec<_>>()
            .join(", ")
    );
}

fn parse_attributes(kind: TagKind, text: &str) -> (BTreeMap<String, String>, bool) {
    let spec = kind.spec();
    let mut attributes = BTreeMap::new();
    let mut none = false;
    let mut rest = text.trim_start();
    while !rest.is_empty() {
        let key_len = rest
            .find(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
            .unwrap_or(rest.len());
        let key = &rest[..key_len];
        assert!(
            !key.is_empty(),
            "malformed attributes in <{}>: {text:?}",
            spec.name
        );
        rest = &rest[key_len..];
        if let Some(value_start) = rest.strip_prefix("=\"") {
            let value_end = value_start
                .find('"')
                .unwrap_or_else(|| panic!("unterminated attribute `{key}` in <{}>", spec.name));
            assert!(
                spec.attributes.contains(&key),
                "<{}> does not support attribute `{key}`; supported: {:?}",
                spec.name,
                spec.attributes
            );
            let previous = attributes.insert(key.to_string(), value_start[..value_end].to_string());
            assert!(
                previous.is_none(),
                "<{}> repeats attribute `{key}`",
                spec.name
            );
            rest = &value_start[value_end + 1..];
        } else {
            assert!(
                key == "none" && spec.none_shape.is_some(),
                "<{}> does not support keyword `{key}`",
                spec.name
            );
            none = true;
        }
        rest = rest.trim_start();
    }
    if !none {
        for required in spec.required {
            assert!(
                attributes.contains_key(*required),
                "<{}> requires attribute `{required}`",
                spec.name
            );
        }
    }
    (attributes, none)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_and_range_tags_use_clean_coordinates() {
        let fixture = parse_fixture("class <def>Foo</def>\nFoo$0.new");
        assert_eq!(fixture.source, "class Foo\nFoo.new");
        assert_eq!(fixture.cursor, Some(Position::new(1, 3)));
        assert_eq!(fixture.tags.len(), 1);
        assert_eq!(fixture.tags[0].kind, TagKind::Def);
        assert_eq!(
            fixture.tags[0].range,
            Range::new(Position::new(0, 6), Position::new(0, 9))
        );
    }

    #[test]
    fn multiline_ranges_end_on_their_closing_line() {
        let fixture = parse_fixture("<def>class Foo\nend</def>");
        assert_eq!(
            fixture.tags[0].range,
            Range::new(Position::new(0, 0), Position::new(1, 3))
        );
    }

    #[test]
    fn columns_count_utf16_code_units() {
        // "é" is one UTF-16 unit and two bytes; "😀" is two UTF-16 units and four bytes.
        let fixture = parse_fixture("s = \"é😀\"; <def>x</def> = 1$0");
        assert_eq!(
            fixture.tags[0].range,
            Range::new(Position::new(0, 11), Position::new(0, 12))
        );
        assert_eq!(fixture.cursor, Some(Position::new(0, 16)));
    }

    #[test]
    fn point_tags_do_not_consume_later_closing_tags() {
        let fixture = parse_fixture("x<hint label=\"Integer\"> = 1\n<hint none>y = z</hint>");
        assert_eq!(fixture.source, "x = 1\ny = z");
        let ranges: Vec<_> = fixture
            .tags
            .iter()
            .map(|tag| (tag.none, tag.range))
            .collect();
        assert_eq!(
            ranges,
            vec![
                (false, Range::new(Position::new(0, 1), Position::new(0, 1))),
                (true, Range::new(Position::new(1, 0), Position::new(1, 5))),
            ]
        );
    }

    #[test]
    fn quoted_attributes_may_contain_angle_brackets_and_newlines() {
        let fixture = parse_fixture("x<hint label=\": Array<String>\" tooltip=\"a\nb\"> = []");
        assert_eq!(fixture.source, "x = []");
        assert_eq!(fixture.tags[0].attr("label"), Some(": Array<String>"));
        assert_eq!(fixture.tags[0].attr("tooltip"), Some("a\nb"));
    }

    #[test]
    fn ruby_comparisons_are_not_tags() {
        let fixture = parse_fixture("class Dog < Animal; end\na <b if c > d\nx = y <=> z");
        assert!(fixture.tags.is_empty());
        assert_eq!(
            fixture.source,
            "class Dog < Animal; end\na <b if c > d\nx = y <=> z"
        );
    }

    #[test]
    fn location_none_tags_are_points() {
        let fixture = parse_fixture("class Foo$0<impl none>\nend");
        assert_eq!(fixture.source, "class Foo\nend");
        assert!(fixture.tags[0].none);
        assert_eq!(fixture.tags[0].range.start, fixture.tags[0].range.end);
    }

    #[test]
    #[should_panic(expected = "unknown tag <defn>")]
    fn misspelled_tags_fail() {
        parse_fixture("<defn>Foo</defn>");
    }

    #[test]
    #[should_panic(expected = "does not support attribute `lable`")]
    fn misspelled_attributes_fail() {
        parse_fixture("x<hint lable=\"Integer\"> = 1");
    }

    #[test]
    #[should_panic(expected = "unclosed <err>")]
    fn unclosed_range_tags_fail() {
        parse_fixture("<err>Foo");
    }

    #[test]
    #[should_panic(expected = "more than one")]
    fn duplicate_cursors_fail() {
        parse_fixture("a$0 b$0");
    }
}
