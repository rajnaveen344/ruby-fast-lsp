//! Gem dependency products and the identity of the semantic producer inputs
//! they are keyed by. The on-disk cache that stores them lives in
//! `crate::utils::persistent_cache`.

pub(crate) mod dependency_product;

#[cfg(test)]
mod persistence_tests;
