//! Per-source memo of engine method lookups bound to one engine identity.

use std::collections::HashMap;

use crate::core::{FullyQualifiedName, RubyMethod};
use crate::engine::lookup::{MethodAnswer, MethodWant};
use crate::inference::semantics::ReceiverAccess;
use parking_lot::Mutex;

use super::thread_memo::{thread_method_get, thread_method_insert};

/// Entries kept per bounded want (callees, signatures) for one source.
const MAX_BOUNDED_METHOD_ENTRIES_PER_SOURCE: usize = 256;

/// One memoized lookup: a namespace receiver, the access it was asked with,
/// and the wanted product. Together with the engine identity the memo binds
/// to, this is every input of the answer.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(in crate::engine) struct MethodMemoKey {
    namespace: FullyQualifiedName,
    method: RubyMethod,
    access: MemoAccess,
    pub(super) want: MethodWant,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum MemoAccess {
    Any,
    Public,
    Protected(FullyQualifiedName),
}

impl MethodMemoKey {
    pub(in crate::engine) fn new(
        namespace: &FullyQualifiedName,
        method: RubyMethod,
        access: ReceiverAccess<'_>,
        want: MethodWant,
    ) -> Self {
        let access = match access {
            ReceiverAccess::Any => MemoAccess::Any,
            ReceiverAccess::Public => MemoAccess::Public,
            ReceiverAccess::Protected { caller } => MemoAccess::Protected(caller.clone()),
        };
        Self {
            namespace: namespace.clone(),
            method,
            access,
            want,
        }
    }
}

/// Whether namespace lookups for `want` are memoized. Facts and references
/// are cheap chain reads and stay unmemoized.
pub(in crate::engine) fn memoizes(want: MethodWant) -> bool {
    match want {
        MethodWant::Callees | MethodWant::Return | MethodWant::Signatures => true,
        MethodWant::Facts | MethodWant::Reference => false,
    }
}

/// The per-source entry cap for `want`. Returns are unbounded here because
/// the thread-local FIFO fronts them and one source asks a bounded set.
fn entry_cap(want: MethodWant) -> Option<usize> {
    match want {
        MethodWant::Callees | MethodWant::Signatures => Some(MAX_BOUNDED_METHOD_ENTRIES_PER_SOURCE),
        MethodWant::Return | MethodWant::Facts | MethodWant::Reference => None,
    }
}

#[derive(Debug, Default)]
struct ViewCacheState {
    engine_identity: Option<(u64, u64)>,
    methods: HashMap<MethodMemoKey, MethodAnswer>,
    entries_per_want: HashMap<MethodWant, usize>,
}

impl ViewCacheState {
    fn bind_engine_identity(&mut self, engine_identity: (u64, u64)) {
        if self.engine_identity != Some(engine_identity) {
            self.engine_identity = Some(engine_identity);
            self.methods.clear();
            self.entries_per_want.clear();
        }
    }

    fn insert(&mut self, key: MethodMemoKey, answer: MethodAnswer) {
        let entries = self.entries_per_want.entry(key.want).or_default();
        if entry_cap(key.want).is_some_and(|cap| *entries >= cap) {
            return;
        }
        if self.methods.insert(key, answer).is_none() {
            *entries += 1;
        }
    }
}

/// Bounded-lifetime memoization for repeated method lookups while collecting
/// one source product.
///
/// The cache binds itself to one exact engine instance and semantic revision.
/// Replacements and engine clones receive different identities, so callers
/// cannot accidentally reuse results across isolated project engines.
/// Return lookups additionally reuse a thread-local 8192-entry FIFO keyed by
/// that same identity so parallel file collection can share identical
/// receiver/method results without one `ViewCache` mutex spanning
/// the batch. Explicit `foo.bar` return lookups reuse the public-access entry
/// when the receiver chain has no private/protected method of that name, so
/// caller namespace is not part of the hot key. Hits are O(1). Do not raise
/// the caps without an RSS measurement.
#[derive(Debug, Default)]
pub struct ViewCache {
    state: Mutex<ViewCacheState>,
}

impl ViewCache {
    /// The memoized answer for `key`, computing it with `compute` on a miss.
    /// `compute` runs without the lock held, so it may itself look up
    /// through this cache.
    pub(in crate::engine) fn method(
        &self,
        engine_identity: (u64, u64),
        key: MethodMemoKey,
        compute: impl FnOnce() -> MethodAnswer,
    ) -> MethodAnswer {
        let thread_shared = key.want == MethodWant::Return;
        if thread_shared {
            if let Some(hit) = thread_method_get(engine_identity, &key) {
                return hit;
            }
        }

        {
            let mut state = self.state.lock();
            state.bind_engine_identity(engine_identity);
            if let Some(cached) = state.methods.get(&key) {
                let hit = cached.clone();
                drop(state);
                if thread_shared {
                    thread_method_insert(engine_identity, key, hit.clone());
                }
                return hit;
            }
        }

        let answer = compute();
        if thread_shared {
            thread_method_insert(engine_identity, key.clone(), answer.clone());
        }
        let mut state = self.state.lock();
        if state.engine_identity == Some(engine_identity) {
            state.insert(key, answer.clone());
        }
        answer
    }

    /// Valid (return, callee, signature) entries.
    #[cfg(test)]
    pub(crate) fn valid_entry_counts_for_test(&self) -> (usize, usize, usize) {
        let state = self.state.lock();
        let count = |want| state.entries_per_want.get(&want).copied().unwrap_or(0);
        (
            count(MethodWant::Return),
            count(MethodWant::Callees),
            count(MethodWant::Signatures),
        )
    }
}
