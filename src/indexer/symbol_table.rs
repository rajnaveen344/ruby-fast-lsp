//! SymbolTable - Read-only symbol-resolution interface.
//!
//! Narrow read-only view over the project's indexed symbols. Carves out the
//! surface that `ReferenceVisitor`, `MethodResolver`, and analyzers need from
//! `RubyIndex` without depending on the concrete type.
//!
//! ISP: each method is one a consumer actually needs.
//! DIP: consumers depend on this trait, not on `RubyIndex`.

use crate::indexer::entry::Entry;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_method::RubyMethod;

pub trait SymbolTable {
    /// True if this FQN has at least one *definition* (not just references).
    fn contains_fqn(&self, fqn: &FullyQualifiedName) -> bool;

    /// True if any method with this name exists anywhere in the index.
    fn contains_method(&self, method: &RubyMethod) -> bool;

    /// MRO (ancestor chain) for an Instance or Singleton FQN. Memoized internally.
    /// Panics if `fqn` is not a `Namespace` variant.
    fn get_ancestor_chain(&self, fqn: &FullyQualifiedName) -> Vec<FullyQualifiedName>;

    /// Method entries defined directly on `owner` (no ancestor walk).
    /// `owner` must carry the correct namespace-kind.
    fn methods_on_owner(&self, owner: &FullyQualifiedName) -> Vec<&Entry>;

    /// All method entries across the whole index with the given name.
    fn get_methods_by_name(&self, method: &RubyMethod) -> Option<Vec<&Entry>>;

    /// Definitions for `fqn` (filters out Reference entries). None if empty.
    fn get(&self, fqn: &FullyQualifiedName) -> Option<Vec<&Entry>>;

    /// Classes that include the given module, each paired with the
    /// `via` chain (modules through which the inclusion flows).
    fn including_classes(
        &self,
        module_fqn: &FullyQualifiedName,
    ) -> Vec<(FullyQualifiedName, Vec<FullyQualifiedName>)>;
}
