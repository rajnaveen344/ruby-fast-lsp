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
mod client_messages;
mod fake_editor;
mod fixture;
mod inlay_hints;
mod paths;
mod process;

pub use check::{check, check_multi_file, check_project};
pub use fake_editor::FakeEditor;
pub use paths::{fixture_path, fixture_uri, fixture_uri_path};

pub use fixture::{parse_fixture, strip_markers, Fixture, Tag, TagKind, CURSOR_MARKER};
pub use inlay_hints::{get_hint_label, get_hint_tooltip};
pub(crate) use process::isolate_decompiler_budget;
#[cfg(unix)]
pub(crate) use process::{wait_for_process_ready, with_process_clock};
