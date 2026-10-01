//! Per-source memo of engine method lookups bound to one engine identity.

use std::collections::HashMap;
use std::sync::Arc;

use crate::core::{FullyQualifiedName, MethodFact, ResolvedMethodCallee, RubyMethod, RubyType};
use parking_lot::Mutex;

use super::thread_memo::{thread_method_return_get, thread_method_return_insert};

const MAX_RESOLVED_METHOD_CACHE_ENTRIES_PER_SOURCE: usize = 256;
const MAX_METHOD_SIGNATURE_FACT_CACHE_ENTRIES_PER_SOURCE: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(in crate::engine) enum MethodReturnQueryAccess {
    Private,
    Public,
    Protected(FullyQualifiedName),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct MethodReturnQueryKey {
    pub(super) namespace: FullyQualifiedName,
    pub(super) method: RubyMethod,
    pub(super) access: MethodReturnQueryAccess,
}

#[derive(Debug, Default)]
struct AnalysisQueryCacheState {
    engine_identity: Option<(u64, u64)>,
    method_returns: HashMap<MethodReturnQueryKey, Option<RubyType>>,
    method_callees: HashMap<MethodReturnQueryKey, Option<Vec<ResolvedMethodCallee>>>,
    method_signature_facts: HashMap<MethodReturnQueryKey, Arc<Vec<MethodFact>>>,
}

impl AnalysisQueryCacheState {
    fn bind_engine_identity(&mut self, engine_identity: (u64, u64)) {
        if self.engine_identity != Some(engine_identity) {
            self.engine_identity = Some(engine_identity);
            self.method_returns.clear();
            self.method_callees.clear();
            self.method_signature_facts.clear();
        }
    }
}

/// Bounded-lifetime memoization for repeated semantic queries while collecting
/// one source product.
///
/// The cache binds itself to one exact engine instance and semantic revision.
/// Replacements and engine clones receive different identities, so callers
/// cannot accidentally reuse results across isolated project engines.
/// Engine method-return lookups additionally reuse a thread-local 8192-entry
/// FIFO keyed by that same identity so parallel file collection can share
/// identical receiver/method results without one `AnalysisQueryCache` mutex
/// spanning the batch. Explicit `foo.bar` lookups reuse the public-access
/// entry when the receiver chain has no private/protected method of that
/// name, so caller namespace is not part of the hot key. Hits are O(1). Do
/// not raise the cap further without an RSS measurement.
#[derive(Debug, Default)]
pub struct AnalysisQueryCache {
    state: Mutex<AnalysisQueryCacheState>,
}

impl AnalysisQueryCache {
    pub(super) fn method_return(
        &self,
        engine_identity: (u64, u64),
        key: MethodReturnQueryKey,
        compute: impl FnOnce() -> Option<RubyType>,
    ) -> Option<RubyType> {
        if let Some(hit) = thread_method_return_get(engine_identity, &key) {
            return hit;
        }

        {
            let mut state = self.state.lock();
            state.bind_engine_identity(engine_identity);
            if let Some(cached) = state.method_returns.get(&key) {
                let result = cached.clone();
                drop(state);
                thread_method_return_insert(engine_identity, key, result.clone());
                return result;
            }
        }

        let result = compute();
        thread_method_return_insert(engine_identity, key.clone(), result.clone());
        let mut state = self.state.lock();
        if state.engine_identity == Some(engine_identity) {
            state.method_returns.insert(key, result.clone());
        }
        result
    }

    pub(in crate::engine) fn method_callees(
        &self,
        engine_identity: (u64, u64),
        namespace: &FullyQualifiedName,
        method: RubyMethod,
        access: MethodReturnQueryAccess,
        compute: impl FnOnce() -> Option<Vec<ResolvedMethodCallee>>,
    ) -> Option<Vec<ResolvedMethodCallee>> {
        let key = MethodReturnQueryKey {
            namespace: namespace.clone(),
            method,
            access,
        };
        {
            let mut state = self.state.lock();
            state.bind_engine_identity(engine_identity);
            if let Some(cached) = state.method_callees.get(&key) {
                return cached.clone();
            }
        }

        let result = compute();
        let mut state = self.state.lock();
        if state.engine_identity == Some(engine_identity)
            && state.method_callees.len() < MAX_RESOLVED_METHOD_CACHE_ENTRIES_PER_SOURCE
        {
            state.method_callees.insert(key, result.clone());
        }
        result
    }

    pub(in crate::engine) fn method_signature_facts(
        &self,
        engine_identity: (u64, u64),
        namespace: &FullyQualifiedName,
        method: RubyMethod,
        access: MethodReturnQueryAccess,
        compute: impl FnOnce() -> Vec<MethodFact>,
    ) -> Arc<Vec<MethodFact>> {
        let key = MethodReturnQueryKey {
            namespace: namespace.clone(),
            method,
            access,
        };
        {
            let mut state = self.state.lock();
            state.bind_engine_identity(engine_identity);
            if let Some(cached) = state.method_signature_facts.get(&key) {
                return cached.clone();
            }
        }

        let result = Arc::new(compute());
        let mut state = self.state.lock();
        if state.engine_identity == Some(engine_identity)
            && state.method_signature_facts.len()
                < MAX_METHOD_SIGNATURE_FACT_CACHE_ENTRIES_PER_SOURCE
        {
            state.method_signature_facts.insert(key, result.clone());
        }
        result
    }

    #[cfg(test)]
    pub(crate) fn valid_entry_counts_for_test(&self) -> (usize, usize, usize) {
        let state = self.state.lock();
        (
            state.method_returns.len(),
            state.method_callees.len(),
            state.method_signature_facts.len(),
        )
    }
}
