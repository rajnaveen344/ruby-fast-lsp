//! Type Inference for Method Receivers
//!
//! **DEPRECATED**: This module has been consolidated into `crate::query::inference`.
//!
//! All functionality has been moved to:
//! - `ReceiverResolver` in `crate::query::inference::receiver`
//!
//! This module is kept only for re-exports during migration.

// Re-export from the new unified location
pub use crate::query::inference::ReceiverResolver;
