//! JRuby compatibility and Java interop policy.
//!
//! This crate intentionally has no LSP, editor, workspace, filesystem, or
//! `ruby-analysis` dependencies. Its static Ruby scans read Prism syntax trees
//! directly and never consult a Java catalog.

#[macro_use]
#[allow(unused_macros)]
#[path = "../../ruby-analysis/src/invariant.rs"]
mod invariant;

mod names;
mod signatures;
mod source_hint;
mod static_scan;
pub mod syntax;
mod version;

pub use names::{JavaClassName, JavaNameError};
pub use signatures::{
    generate_ruby_signature, ruby_parameter_name, ruby_type_for_jvm_type, SignatureError,
};
pub use source_hint::StaticJavaSourceHint;
pub use static_scan::{
    source_semantics_depend_on_jruby_catalog, static_java_dependencies, static_java_import_names,
    static_java_proxy_references, static_navigation_scan, StaticJavaDependency,
    StaticNavigationScan,
};
pub use version::{
    JrubyRuntimeIdentity, JrubySeries, JrubyVersion, RubyCompatibilityVersion, VersionError,
};
