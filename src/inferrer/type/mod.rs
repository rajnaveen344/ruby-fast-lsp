//! Type analysis and representation.
//!
//! This module contains the core RubyType representation and analyzers
//! for inferring types from literals and collections.

pub mod collection;
pub mod literal;
pub mod ruby;

pub use collection::{ArrayTypeInfo, CollectionAnalyzer, HashTypeInfo};
pub use literal::LiteralAnalyzer;
pub use ruby::*;
