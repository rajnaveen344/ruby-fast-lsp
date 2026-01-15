//! Method return type inference tests.
//!
//! Tests for inferring method return types including:
//! - Same-file inference (literals, method calls)
//! - Cross-module mixin inference (module A calls method from module B, both included in class C)

mod mixin_cross_module;
mod same_file;
