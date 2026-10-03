//! Robustness checks over real Ruby sources.
//!
//! Each check opens a corpus through `FakeEditor`, so every request takes the
//! production handler path. The checks compare the server with itself rather
//! than with a hand-written model:
//!
//! - no request panics on any file or on any partially typed buffer,
//! - incremental edits end in the same observations as a fresh index,
//! - the order in which files are opened does not change any observation,
//! - code that runs cleanly under Ruby produces no diagnostics.
//!
//! The built-in corpus lives in `src/test/fixtures/robustness/`. Set
//! `ROBUSTNESS_CORPUS=<dir>` to run the first three checks against another
//! Ruby tree, for example an installed gem or the Ruby standard library.

mod corpus;
mod edits;
mod observe;
mod tests;
mod workspace;
