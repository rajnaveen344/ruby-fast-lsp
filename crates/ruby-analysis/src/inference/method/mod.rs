//! Method resolution and signatures.
//!
//! This module handles method lookup, resolution, and signature extraction.

pub mod signature;

pub use signature::{MethodSignature, MethodSignatureContext, MethodVisibility, Parameter};
