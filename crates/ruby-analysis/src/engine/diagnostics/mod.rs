//! Reference-candidate resolution and diagnostic derivation: shared pass
//! caches and the modules that own each resolve pass and diagnostic family.

mod call_outcomes;
mod candidates;
mod file_pass;
mod grouped_methods;
pub(in crate::engine) mod helpers;
mod method_absence;
mod method_checks;
mod workspace_pass;

use std::collections::HashMap;

use crate::core::names::fqn_id::ConstLookupId;
use crate::core::names::fqn_id::FqnId;
use crate::core::{
    FullyQualifiedName, NamespaceKind, RubyConstant, RubyMethod, RubyType, TextRange,
};

type MethodReferenceCacheKey = (ConstLookupId, NamespaceKind, RubyMethod, bool);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CachedMethodVisibility {
    Public,
    Protected(FqnId),
    Private,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum AmbiguousMethodReturnAccess {
    Private,
    Public,
    Protected(FqnId),
}

#[derive(Default)]
struct MethodCallOutcomeCaches {
    returns: HashMap<(FqnId, TextRange), Option<RubyType>>,
    visibilities: HashMap<MethodReferenceCacheKey, CachedMethodVisibility>,
    ambiguous_returns:
        HashMap<(MethodReferenceCacheKey, AmbiguousMethodReturnAccess), Option<RubyType>>,
    return_hits: usize,
    return_misses: usize,
    visibility_hits: usize,
    visibility_misses: usize,
    ambiguous_return_hits: usize,
    ambiguous_return_misses: usize,
}

#[derive(Default)]
struct MethodChainCompletenessCache {
    results: HashMap<FullyQualifiedName, bool>,
    dynamic_mixin_hooks: HashMap<FullyQualifiedName, bool>,
    ambiguous_superclasses: HashMap<FullyQualifiedName, bool>,
}

fn constant_name(parts: &[RubyConstant]) -> String {
    parts
        .iter()
        .map(RubyConstant::as_str)
        .collect::<Vec<_>>()
        .join("::")
}
