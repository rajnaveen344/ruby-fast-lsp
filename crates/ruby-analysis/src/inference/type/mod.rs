//! Type derivation for literals, collections, and structural shapes.
//!
//! The resulting representation is [`crate::core::RubyType`].

pub mod collection;
pub mod literal;
pub(crate) mod shape;

pub use collection::{ArrayTypeInfo, CollectionAnalyzer, HashTypeInfo};
pub use literal::LiteralAnalyzer;
