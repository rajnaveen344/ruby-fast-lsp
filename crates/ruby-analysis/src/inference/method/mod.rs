//! Method resolution and return types.
//!
//! This module handles method lookup, resolution, and return-type derivation.

pub(crate) mod arguments;
pub mod constructor;
pub(crate) mod receiver_return;
pub(crate) mod recursive;
pub mod return_type;
