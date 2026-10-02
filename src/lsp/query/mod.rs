//! LSP query adapters over analysis facts, per feature family. The shared
//! cursor context (`EngineQuery`) lives in `crate::features::cursor`.

mod debug;
pub mod diagnostics;
pub mod editing;

pub use editing::signature_help::{SignatureData, SignatureHelpData, SignatureParameterData};
