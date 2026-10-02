//! Persisted dependency products: the on-disk cache and the identity of the
//! semantic producer inputs it is keyed by.

pub(crate) mod dependency_product;
pub mod persistent;

#[cfg(test)]
mod persistence_tests;
