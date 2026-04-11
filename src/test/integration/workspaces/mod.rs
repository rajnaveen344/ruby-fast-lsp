//! Multi-workspace integration tests.
//!
//! Each workspace folder owns a separate `RubyIndex`. Files are routed to
//! the workspace whose root path is the longest prefix of the file's URI;
//! files outside any workspace fall through to the orphan index.

mod dynamic;
mod multi_root;
mod orphan;
