//! Multi-workspace integration tests.
//!
//! Each workspace folder owns separate analysis state. Files are routed to
//! the workspace whose root path is the longest prefix of the file's URI;
//! files outside any workspace fall through to orphan analysis state.

mod dynamic;
mod multi_root;
mod orphan;
