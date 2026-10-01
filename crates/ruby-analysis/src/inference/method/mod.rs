//! Method resolution and return types.
//!
//! This module handles method lookup, resolution, and return-type derivation.

pub mod constructor;
pub(crate) mod recursive;
pub mod return_type;

pub use return_type::{
    method_call_return_type, method_call_return_type_with_visibility, method_call_type_outcome,
    method_call_type_outcome_with_private, method_call_type_outcome_with_visibility,
    rbs_class_exists_for_type, rbs_method_exists_for_type, rbs_method_signatures_for_type,
};
