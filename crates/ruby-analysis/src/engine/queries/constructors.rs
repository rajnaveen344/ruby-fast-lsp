//! Constructor resolution for `Receiver.new` through the engine-owned
//! singleton lookup chain.

use crate::core::{
    FullyQualifiedName, NamespaceKind, RubyMethod, SourceKind, TypeInferenceOutcome, UnknownReason,
};
use crate::inference::method::constructor::{declared_singleton_new, ConstructorResult};
use crate::invariant::ExpectInvariant;

use super::View;
use crate::engine::resolution::method_lookup_chain;

impl View<'_> {
    /// Resolve what `class.new` returns.
    ///
    /// The nearest singleton ancestor that declares its own singleton `new`
    /// decides the result; otherwise the call reaches the default `Class#new`
    /// and returns an instance. Instance-side entries of the chain (`Class`,
    /// `Module`, `Object`, ...) are the default constructor path, so only
    /// singleton entries are consulted.
    ///
    /// Ruby code that defines its own singleton `new` may hand back any
    /// object, so its result stays Unknown instead of an assumed instance.
    pub fn constructor_result(&self, class: &FullyQualifiedName) -> ConstructorResult {
        let new = RubyMethod::new("new").expect_invariant(
            "the constructor name `new` was rejected as a method name",
            "`new` is a valid Ruby method identifier",
            "keep RubyMethod validation accepting plain identifiers",
        );
        let singleton = FullyQualifiedName::namespace_with_kind(
            class.namespace_parts(),
            NamespaceKind::Singleton,
        );
        method_lookup_chain(self.engine, &singleton)
            .iter()
            .filter(|ancestor| ancestor.namespace_kind() == Some(NamespaceKind::Singleton))
            .find_map(|ancestor| {
                if self.ruby_code_defines(ancestor, &new) {
                    return Some(ConstructorResult::Declared(TypeInferenceOutcome::unknown(
                        UnknownReason::UnresolvedMethodReturn,
                    )));
                }
                declared_singleton_new(ancestor.namespace_parts_slice())
            })
            .unwrap_or(ConstructorResult::Instance)
    }

    /// Whether Ruby source defines `method` directly on `owner`. A `new`
    /// standing for an instance `initialize` keeps the language constructor.
    /// Stubs,
    /// signatures, and external runtime metadata (such as an imported Java
    /// constructor) declare a constructor instead of replacing it.
    fn ruby_code_defines(&self, owner: &FullyQualifiedName, method: &RubyMethod) -> bool {
        self.method_facts_matching_owner_name(owner, method)
            .iter()
            .any(|fact| {
                !fact.is_normalized_from_initialize()
                    && self.file(fact.range.file_id).is_none_or(|file| {
                        !matches!(
                            file.kind,
                            SourceKind::Stub
                                | SourceKind::Signature
                                | SourceKind::Stdlib
                                | SourceKind::External
                        )
                    })
            })
    }
}
