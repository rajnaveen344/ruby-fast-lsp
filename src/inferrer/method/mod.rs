//! Method resolution and signatures.
//!
//! This module handles method lookup, resolution, and signature extraction.

pub mod resolver;
pub mod signature;

pub use resolver::MethodResolver;
pub use signature::{MethodSignature, MethodSignatureContext, MethodVisibility, Parameter};
