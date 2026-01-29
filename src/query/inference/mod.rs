//! Unified Type Inference Module
//!
//! This module consolidates type inference logic that was previously scattered
//! across query/method/type_inference.rs and inferrer/return_type.rs.
//!
//! # Architecture
//!
//! - `ReceiverResolver` - Resolves receiver types (variables, method chains)
//! - `ReturnTypeResolver` - Resolves method return types
//! - `LocalVariableResolver` - Resolves local variable types at positions
//!
//! All resolvers delegate to the core `MethodResolver` in the inferrer layer.

mod local_variable;
mod receiver;
mod return_type;

pub use local_variable::LocalVariableResolver;
pub use receiver::ReceiverResolver;
pub use return_type::ReturnTypeResolver;
