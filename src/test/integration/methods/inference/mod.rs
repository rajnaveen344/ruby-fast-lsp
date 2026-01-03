//! Method return type inference tests.
//!
//! Tests for inferring method return types including:
//! - Same-file inference (literals, method calls)
//! - Cross-file inference (method in one file calling method in another)
//! - Chained cross-file inference (A -> B -> C)
//! - Cross-module mixin inference (module A calls method from module B, both included in class C)

mod chained;
mod cross_file;
mod mixin_cross_module;
mod same_file;
