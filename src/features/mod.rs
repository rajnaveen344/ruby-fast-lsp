//! Editor features. Each feature owns its request body, protocol conversion,
//! and query adapter over the analysis engine, and exposes `handle(server,
//! params)` to the `lsp` service. Features read server state and loader
//! products but never name `lsp`.

pub mod cursor;
pub mod debug;
pub mod diagnostics;
pub mod editing;
pub mod navigation;
pub mod presentation;
