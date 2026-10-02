//! Thread-local FIFO memo of method-return lookups shared by one worker.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

use crate::core::{FullyQualifiedName, RubyMethod};
use crate::engine::lookup::MethodAnswer;

use super::memo::MethodMemoKey;

const MAX_THREAD_METHOD_RETURN_CACHE_ENTRIES: usize = 8192;

/// Thread-local memo of engine method-return lookup answers for one engine
/// identity, keyed like the per-source memo.
///
/// Parallel project collection keeps this cache on the worker thread so identical
/// `Integer#to_s` / `User#new` lookups reuse the same bounded result without
/// sharing `AnalysisQueryCache` across a file batch. Explicit calls use the
/// public cache when the receiver chain has no private/protected method of that
/// name, so caller namespace is not part of the hot key. Identity changes drop
/// the entries. A 256-entry cap left ~52k misses and ~4s of engine return
/// lookup on the JRuby project visitor; 8192 holds one worker's unique
/// receiver/method set. Hits are O(1); eviction is insertion-order FIFO. Do
/// not raise the cap further without an RSS measurement; do not key this
/// cache only on method name.
struct ThreadMethodReturnCache {
    identity: Option<(u64, u64)>,
    entries: HashMap<MethodMemoKey, MethodAnswer>,
    order: VecDeque<MethodMemoKey>,
    non_public: HashMap<(FullyQualifiedName, RubyMethod), bool>,
    non_public_order: VecDeque<(FullyQualifiedName, RubyMethod)>,
}

impl Default for ThreadMethodReturnCache {
    fn default() -> Self {
        Self {
            identity: None,
            entries: HashMap::new(),
            order: VecDeque::new(),
            non_public: HashMap::new(),
            non_public_order: VecDeque::new(),
        }
    }
}

impl ThreadMethodReturnCache {
    fn bind(&mut self, identity: (u64, u64)) {
        if self.identity != Some(identity) {
            self.identity = Some(identity);
            self.entries.clear();
            self.order.clear();
            self.non_public.clear();
            self.non_public_order.clear();
        }
    }

    fn get(&mut self, key: &MethodMemoKey) -> Option<MethodAnswer> {
        self.entries.get(key).cloned()
    }

    fn insert(&mut self, key: MethodMemoKey, value: MethodAnswer) {
        if self.entries.contains_key(&key) {
            self.entries.insert(key, value);
            return;
        }
        while self.entries.len() >= MAX_THREAD_METHOD_RETURN_CACHE_ENTRIES {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            self.entries.remove(&oldest);
        }
        self.order.push_back(key.clone());
        self.entries.insert(key, value);
    }

    fn get_non_public(&mut self, key: &(FullyQualifiedName, RubyMethod)) -> Option<bool> {
        self.non_public.get(key).copied()
    }

    fn insert_non_public(&mut self, key: (FullyQualifiedName, RubyMethod), value: bool) {
        if self.non_public.contains_key(&key) {
            self.non_public.insert(key, value);
            return;
        }
        while self.non_public.len() >= MAX_THREAD_METHOD_RETURN_CACHE_ENTRIES {
            let Some(oldest) = self.non_public_order.pop_front() else {
                break;
            };
            self.non_public.remove(&oldest);
        }
        self.non_public_order.push_back(key.clone());
        self.non_public.insert(key, value);
    }
}

thread_local! {
    static THREAD_METHOD_RETURN_CACHE: RefCell<ThreadMethodReturnCache> =
        RefCell::new(ThreadMethodReturnCache::default());
}

pub(super) fn thread_method_get(identity: (u64, u64), key: &MethodMemoKey) -> Option<MethodAnswer> {
    THREAD_METHOD_RETURN_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        cache.bind(identity);
        cache.get(key)
    })
}

pub(super) fn thread_method_insert(identity: (u64, u64), key: MethodMemoKey, value: MethodAnswer) {
    THREAD_METHOD_RETURN_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        cache.bind(identity);
        cache.insert(key, value);
    });
}

pub(super) fn thread_protected_return_may_differ(
    identity: (u64, u64),
    namespace: &FullyQualifiedName,
    method: RubyMethod,
    compute: impl FnOnce() -> bool,
) -> bool {
    let key = (namespace.clone(), method);
    if let Some(hit) = THREAD_METHOD_RETURN_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        cache.bind(identity);
        cache.get_non_public(&key)
    }) {
        return hit;
    }

    let value = compute();
    THREAD_METHOD_RETURN_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        cache.bind(identity);
        cache.insert_non_public(key, value);
    });
    value
}
