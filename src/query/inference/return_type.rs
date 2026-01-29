//! Return Type Resolution
//!
//! Resolves return types for methods given a receiver type.

use crate::indexer::index_ref::{Index, Unlocked};
use crate::inferrer::method::resolver::MethodResolver;
use crate::inferrer::r#type::ruby::RubyType;

/// Resolves return types for methods
pub struct ReturnTypeResolver<'a> {
    index: &'a Index<Unlocked>,
}

impl<'a> ReturnTypeResolver<'a> {
    /// Create a new ReturnTypeResolver
    pub fn new(index: &'a Index<Unlocked>) -> Self {
        Self { index }
    }

    /// Resolve the return type of a method called on a receiver type
    pub fn resolve(&self, receiver_type: &RubyType, method_name: &str) -> Option<RubyType> {
        let index = self.index.lock();
        MethodResolver::resolve_method_return_type(&index, receiver_type, method_name)
    }
}
