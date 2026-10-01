//! Constructor resolution for `Receiver.new` through the engine-owned
//! singleton lookup chain.

use crate::core::{FullyQualifiedName, NamespaceKind};
use crate::inference::method::constructor::{declared_singleton_new, ConstructorResult};

use super::AnalysisQuery;
use crate::engine::resolution::method_lookup_chain;

impl AnalysisQuery<'_> {
    /// Resolve what `class.new` returns.
    ///
    /// The nearest singleton ancestor that declares its own singleton `new`
    /// decides the result; otherwise the call reaches the default `Class#new`
    /// and returns an instance. Instance-side entries of the chain (`Class`,
    /// `Module`, `Object`, ...) are the default constructor path, so only
    /// singleton entries are consulted.
    pub fn constructor_result(&self, class: &FullyQualifiedName) -> ConstructorResult {
        let singleton = FullyQualifiedName::namespace_with_kind(
            class.namespace_parts(),
            NamespaceKind::Singleton,
        );
        method_lookup_chain(self.engine, &singleton)
            .iter()
            .filter(|ancestor| ancestor.namespace_kind() == Some(NamespaceKind::Singleton))
            .find_map(|ancestor| declared_singleton_new(ancestor.namespace_parts_slice()))
            .unwrap_or(ConstructorResult::Instance)
    }
}
