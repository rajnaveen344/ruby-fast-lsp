//! The protocol layer: the `tower-lsp` service facade that routes requests to
//! feature `handle` functions, the protocol and document lifecycle, the stdio
//! client input, and the `check` CLI report.

pub mod check;
pub mod lifecycle;
pub mod service;
pub mod stdio;
