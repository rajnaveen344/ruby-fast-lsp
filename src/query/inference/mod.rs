//! Unified Type Inference Module
//!
//! This module consolidates type inference logic that was previously scattered
//! across query/method/type_inference.rs and inferrer/return_type.rs.
//!
//! # Architecture
//!
//! - `ReceiverResolver` - Resolves receiver types (variables, method chains)
//!
//! This is the legacy fallback used when an `IndexQuery` has no analysis engine.

mod receiver;

pub use receiver::ReceiverResolver;
