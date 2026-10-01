//! Thread-local memo of higher-order prepare results and its cached entry point.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};

use super::higher_order::prepare_higher_order_call_with_fallbacks_uncached;
use crate::core::{FullyQualifiedName, RubyType};
use crate::engine::{AnalysisQueryCache, View};
use crate::inference::higher_order::PreparedCallableSet;

const MAX_HIGHER_ORDER_PREPARE_CACHE_ENTRIES: usize = 256;

#[derive(Clone, PartialEq, Eq, Hash)]
struct HigherOrderPrepareKey {
    receiver_type: Option<RubyType>,
    implicit_namespace: Option<FullyQualifiedName>,
    method_name: String,
    argument_types: Vec<RubyType>,
}

/// Thread-local memo of higher-order prepare results for one engine identity.
///
/// Parallel project collection keeps this cache on the worker thread so identical
/// `Array#each` / `Hash#map` / implicit-self prepares reuse the same bounded
/// result without sharing `AnalysisQueryCache` across a file batch. Identity
/// changes (fact replacement, engine clone) drop the entries. Do not raise the
/// cap without an RSS measurement; do not key this cache only on method name.
struct HigherOrderPrepareCache {
    identity: Option<(u64, u64)>,
    entries:
        HashMap<HigherOrderPrepareKey, Result<PreparedCallableSet, crate::core::UnknownReason>>,
    order: VecDeque<HigherOrderPrepareKey>,
}

impl Default for HigherOrderPrepareCache {
    fn default() -> Self {
        Self {
            identity: None,
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }
}

impl HigherOrderPrepareCache {
    fn bind(&mut self, identity: (u64, u64)) {
        if self.identity != Some(identity) {
            self.identity = Some(identity);
            self.entries.clear();
            self.order.clear();
        }
    }

    fn get(
        &mut self,
        key: &HigherOrderPrepareKey,
    ) -> Option<Result<PreparedCallableSet, crate::core::UnknownReason>> {
        if !self.entries.contains_key(key) {
            return None;
        }
        if let Some(position) = self.order.iter().position(|cached| cached == key) {
            self.order.remove(position);
        }
        self.order.push_back(key.clone());
        self.entries.get(key).cloned()
    }

    fn insert(
        &mut self,
        key: HigherOrderPrepareKey,
        value: Result<PreparedCallableSet, crate::core::UnknownReason>,
    ) {
        if self.entries.contains_key(&key) {
            self.entries.insert(key, value);
            return;
        }
        while self.entries.len() >= MAX_HIGHER_ORDER_PREPARE_CACHE_ENTRIES {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            self.entries.remove(&oldest);
        }
        self.order.push_back(key.clone());
        self.entries.insert(key, value);
    }
}

thread_local! {
    static HIGHER_ORDER_PREPARE_CACHE: RefCell<HigherOrderPrepareCache> =
        RefCell::new(HigherOrderPrepareCache::default());
}

fn cached_higher_order_prepare(
    identity: (u64, u64),
    key: HigherOrderPrepareKey,
    compute: impl FnOnce() -> Result<PreparedCallableSet, crate::core::UnknownReason>,
) -> Result<PreparedCallableSet, crate::core::UnknownReason> {
    let cached = HIGHER_ORDER_PREPARE_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        cache.bind(identity);
        cache.get(&key)
    });
    if let Some(cached) = cached {
        return cached;
    }

    let computed = compute();
    HIGHER_ORDER_PREPARE_CACHE.with(|cell| {
        let mut cache = cell.borrow_mut();
        cache.bind(identity);
        cache.insert(key, computed.clone());
    });
    computed
}

/// One engine signature lookup shared by callable, forwarded-block, and
/// direct-yield preparation. Collection receivers skip the engine and use the
/// same embedded RBS path as ordinary Array/Hash method returns.
pub(crate) fn prepare_higher_order_call_with_fallbacks(
    query: Option<&View<'_>>,
    cache: Option<&AnalysisQueryCache>,
    receiver_type: Option<&RubyType>,
    implicit_namespace: Option<&FullyQualifiedName>,
    method_name: &str,
    argument_types: &[RubyType],
) -> Result<PreparedCallableSet, crate::core::UnknownReason> {
    let identity = query.map(View::query_cache_identity).unwrap_or((0, 0));
    let key = HigherOrderPrepareKey {
        receiver_type: receiver_type.cloned(),
        implicit_namespace: implicit_namespace.cloned(),
        method_name: method_name.to_string(),
        argument_types: argument_types.to_vec(),
    };
    cached_higher_order_prepare(identity, key, || {
        prepare_higher_order_call_with_fallbacks_uncached(
            query,
            cache,
            receiver_type,
            implicit_namespace,
            method_name,
            argument_types,
        )
    })
}
