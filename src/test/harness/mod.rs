//! Inline test harness for rust-analyzer style tests.
//!
//! This module provides:
//! - Marker extraction utilities (`$0` cursor, `<def>`, `<ref>`, `<type>` tags)
//! - Check functions for goto definition, references, type inference
//! - Check functions for inlay hints, diagnostics, code lens
//!
//! # Example
//!
//! ```ignore
//! use crate::test::harness::check_goto;
//!
//! #[tokio::test]
//! async fn goto_class_definition() {
//!     check_goto(r#"
//! <def>class Foo
//! end</def>
//!
//! Foo$0.new
//! "#).await;
//! }
//! ```

mod code_lens;
mod diagnostics;
mod fixture;
mod goto;
mod inlay_hints;
mod references;
mod types;

// Re-export check functions
pub use code_lens::{check_code_lens, check_no_code_lens};
pub use diagnostics::check_diagnostics;
pub use goto::check_goto;
pub use inlay_hints::{
    check_inlay_hints, check_no_inlay_hints, check_no_inlay_hints_containing, get_hint_label,
};
pub use references::check_references;
pub use types::check_type;

// Re-export core utilities for tests
pub use fixture::{
    extract_tags, extract_tags_with_attributes, parse_fixture, setup_with_fixture, InlineFixture,
    Tag, CURSOR_MARKER,
};

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
