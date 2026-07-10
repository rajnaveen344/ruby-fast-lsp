//! Method resolution and signatures.
//!
//! This module handles method lookup, resolution, and signature extraction.

pub mod return_type;
pub mod signature;

pub use return_type::{
    method_call_return_type, method_call_return_type_with_private,
    method_call_return_type_with_visibility, rbs_class_exists_for_type, rbs_method_exists_for_type,
    rbs_method_signatures_for_type,
};
pub use signature::{MethodSignature, MethodSignatureContext, MethodVisibility, Parameter};
