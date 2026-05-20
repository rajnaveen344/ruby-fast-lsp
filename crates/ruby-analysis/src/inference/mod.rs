//! Editor-agnostic Ruby type inference.
//!
//! This crate owns type inference helpers, RBS lookup, literal/collection
//! analysis, and forward local type tracking.

pub mod control_flow;
pub mod method;
pub mod rbs;
pub mod r#type;
pub mod type_tracker;

pub use method::{MethodSignature, MethodSignatureContext, MethodVisibility, Parameter};
pub use r#type::*;
pub use rbs::{get_rbs_method_return_type, has_rbs_class, rbs_declaration_count, rbs_method_count};
pub use crate::core::RubyType;
