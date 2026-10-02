//! LSP query adapters over analysis facts, per feature family. The shared
//! cursor context (`EngineQuery`) lives in `crate::features::cursor`.

mod debug;
pub mod diagnostics;
pub mod editing;
pub mod navigation;
pub mod presentation;

pub use editing::signature_help::{SignatureData, SignatureHelpData, SignatureParameterData};
pub use presentation::code_lens::CodeLensData;
pub use presentation::hover::HoverInfo;
pub use presentation::inlay_hints::{InlayHintData, InlayHintKind};
