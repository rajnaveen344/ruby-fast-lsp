//! Inline test harness for rust-analyzer style tests.
//!
//! This module provides a unified `check()` function that auto-detects
//! what to verify based on markers in the fixture.
//!
//! # Example
//!
//! ```ignore
//! use crate::test::harness::check;
//!
//! #[tokio::test]
//! async fn goto_class_definition() {
//!     check(r#"
//! <def>class Foo
//! end</def>
//!
//! Foo$0.new
//! "#).await;
//! }
//! ```

mod check;
mod fixture;
mod inlay_hints;

// Re-export unified check functions (the only API)
pub use check::{check, check_multi_file};

// Re-export core utilities for tests (used internally by check)
pub use fixture::{
    extract_tags, extract_tags_with_attributes, parse_fixture, setup_with_fixture,
    setup_with_multi_file_fixture, InlineFixture, Tag, CURSOR_MARKER,
};
pub use inlay_hints::get_hint_label;

#[cfg(test)]
mod tests {
    use super::fixture::*;

    #[test]
    fn test_extract_cursor_simple() {
        let (pos, clean) = extract_cursor("hello$0 world");
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 5);
        assert_eq!(clean, "hello world");
    }

    #[test]
    fn test_extract_cursor_multiline() {
        let (pos, clean) = extract_cursor("line1\nline2\nfoo$0bar");
        assert_eq!(pos.line, 2);
        assert_eq!(pos.character, 3);
        assert_eq!(clean, "line1\nline2\nfoobar");
    }

    #[test]
    fn test_extract_tags_single() {
        let (ranges, clean) = extract_tags("class <def>Foo</def>\nend", "def");
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start.line, 0);
        assert_eq!(ranges[0].start.character, 6);
        assert_eq!(ranges[0].end.character, 9);
        assert_eq!(clean, "class Foo\nend");
    }

    #[test]
    fn test_extract_tags_multiline() {
        let (ranges, clean) = extract_tags("<def>class Foo\nend</def>", "def");
        assert_eq!(ranges.len(), 1);
        assert_eq!(ranges[0].start.line, 0);
        assert_eq!(ranges[0].end.line, 1);
        assert_eq!(ranges[0].end.character, 3);
        assert_eq!(clean, "class Foo\nend");
    }

    #[tokio::test]
    async fn harness_setup_works() {
        let content = "class Foo\nend";
        let (server, uri) = setup_with_fixture(content).await;
        let docs = server.docs.lock();
        assert!(docs.contains_key(&uri));
    }
}
