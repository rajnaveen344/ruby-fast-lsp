#[macro_use]
#[allow(unused_macros)]
#[path = "../crates/ruby-analysis/src/invariant.rs"]
mod invariant;

pub mod environment;
pub mod loader;
pub mod lsp;
pub mod server;
#[cfg(test)]
pub mod test;
pub mod utils;

/// Server package version, for tools that report which server they measured.
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");
