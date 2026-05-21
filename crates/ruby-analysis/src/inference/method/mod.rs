//! Method resolution and signatures.
//!
//! This module handles method lookup, resolution, and signature extraction.

pub mod return_type;
pub mod signature;

pub use return_type::method_call_return_type;
pub use signature::{MethodSignature, MethodSignatureContext, MethodVisibility, Parameter};
