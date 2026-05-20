//! Type inference facade.
//!
//! The implementation lives in `ruby-analysis-inference`; the LSP crate keeps
//! this module as a compatibility import path while callers migrate to the
//! crate directly.

pub use ruby_analysis_inference::{
    get_rbs_method_return_type, has_rbs_class, method, r#type, rbs, rbs_declaration_count,
    rbs_method_count, type_tracker, MethodSignature, MethodSignatureContext, MethodVisibility,
    Parameter, RubyType,
};
