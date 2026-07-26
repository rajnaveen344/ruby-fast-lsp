//! JRuby compatibility and Java interop policy.
//!
//! This crate intentionally has no LSP, editor, workspace, filesystem, or
//! `ruby-analysis` dependencies.

mod names;
mod signatures;
mod version;

pub use names::{JavaClassName, JavaNameError};
pub use signatures::{
    generate_ruby_signature, ruby_parameter_name, ruby_type_for_jvm_type, SignatureError,
};
pub use version::{
    JrubyRuntimeIdentity, JrubySeries, JrubyVersion, RubyCompatibilityVersion, VersionError,
};
