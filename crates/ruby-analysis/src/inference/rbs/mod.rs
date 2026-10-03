//! RBS Type Index
//!
//! Provides access to RBS type definitions for built-in Ruby classes.
//! The RBS definitions are embedded in the binary at compile time.

mod catalog;
mod conversion;
mod embedded_callable;
mod higher_order;
mod prepare_cache;
mod signatures;

use once_cell::sync::Lazy;
use parking_lot::RwLock;
use rbs_parser::Loader;

pub use catalog::{get_rbs_class_methods, rbs_class_method_exists};
pub use conversion::rbs_type_to_ruby_type;
pub(crate) use prepare_cache::prepare_higher_order_call_with_fallbacks;
pub use signatures::{
    get_rbs_method_return_type, get_rbs_method_return_type_as_ruby_type,
    get_rbs_method_return_type_with_type_args, get_rbs_method_signatures, RbsMethodSignature,
    RbsSignatureParameter,
};

/// Global RBS loader with embedded core types
static RBS_LOADER: Lazy<RwLock<Loader>> = Lazy::new(|| {
    let mut loader = Loader::new();
    if let Err(e) = loader.load_embedded_core() {
        log::warn!("Failed to load embedded RBS core types: {:?}", e);
    }
    log::info!(
        "Loaded {} RBS declarations with {} methods",
        loader.declaration_count(),
        loader.method_count()
    );
    RwLock::new(loader)
});

/// Check if a class exists in RBS definitions
pub fn has_rbs_class(class_name: &str) -> bool {
    let loader = RBS_LOADER.read();
    loader.get_class(class_name).is_some()
}

#[cfg(test)]
mod tests;
