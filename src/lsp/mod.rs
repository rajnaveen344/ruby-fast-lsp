//! The protocol layer: the `tower-lsp` service facade that routes requests to
//! feature `handle` functions, the protocol and document lifecycle, and the
//! `check` CLI report.

pub mod check;
pub mod lifecycle;
pub mod service;
