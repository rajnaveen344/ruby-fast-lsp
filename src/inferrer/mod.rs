//! Type inference for Ruby code.
//!
//! This module provides type inference capabilities including:
//! - Simple forward type tracking via TypeTracker
//! - Type representation and analysis
//! - Method resolution and signatures
//! - RBS type definitions
//! - Return type inference

pub mod method;
pub mod rbs;
pub mod return_type;
pub mod r#type;
pub mod type_tracker;

pub use method::{
    MethodResolver, MethodSignature, MethodSignatureContext, MethodVisibility, Parameter,
};
pub use r#type::*;
pub use rbs::{get_rbs_method_return_type, has_rbs_class, rbs_declaration_count, rbs_method_count};
pub use return_type::infer_method_return_type;
