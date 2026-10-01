//! Type derivation for literals and structural shapes.
//!
//! The resulting representation is [`crate::core::RubyType`].

pub mod literal;
pub(crate) mod shape;

pub use literal::LiteralAnalyzer;
