//! `Names`: the engine's interner for fully qualified names and constant
//! lookups. Stored facts keep the compact ids; reads expand them back.

use crate::invariant::ExpectInvariant;
use std::mem::size_of;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::core::names::fqn_id::{ConstLookupId, FqnId};
use crate::core::storage::memory_estimate::fqn_heap_bytes;
use crate::core::storage::reference_store::ConstLookup;
use crate::core::{ConstantPath, FullyQualifiedName, RubyConstant};
use indexmap::IndexSet;

#[derive(Debug, Default)]
pub(in crate::engine) struct Names {
    fqns: IndexSet<FullyQualifiedName>,
    const_lookups: IndexSet<ConstLookup>,
    #[cfg(test)]
    fqn_lookup_count: AtomicUsize,
}

impl Clone for Names {
    fn clone(&self) -> Self {
        Self {
            fqns: self.fqns.clone(),
            const_lookups: self.const_lookups.clone(),
            #[cfg(test)]
            fqn_lookup_count: AtomicUsize::new(self.fqn_lookup_count.load(Ordering::Relaxed)),
        }
    }
}

impl Names {
    pub(in crate::engine) fn intern_fqn(&mut self, fqn: FullyQualifiedName) -> FqnId {
        let (index, _) = self.fqns.insert_full(fqn);
        FqnId(u32::try_from(index).expect_invariant(
            "FQN interner exceeded u32 ids",
            "FqnId stores u32",
            "widen FqnId before interning more than u32::MAX names",
        ))
    }

    pub(in crate::engine) fn fqn_id(&self, fqn: &FullyQualifiedName) -> Option<FqnId> {
        #[cfg(test)]
        self.fqn_lookup_count.fetch_add(1, Ordering::Relaxed);
        self.fqns.get_index_of(fqn).map(|index| {
            FqnId(u32::try_from(index).expect_invariant(
                "FQN interner returned an index above u32",
                "every inserted index is validated before becoming an FqnId",
                "widen FqnId and its insertion boundary together",
            ))
        })
    }

    pub(in crate::engine) fn fqn(&self, id: FqnId) -> Option<&FullyQualifiedName> {
        self.fqns.get_index(id.0 as usize)
    }

    /// Expand a graph edge's interned FQN id, which must exist.
    pub(in crate::engine) fn expand_interned_fqn(&self, id: FqnId) -> FullyQualifiedName {
        self.fqn(id)
            .expect_invariant(
                "graph edge points to missing FQN id",
                "graph edges must only store interned FQN ids",
                "intern graph edge FQNs before inserting facts",
            )
            .clone()
    }

    pub(in crate::engine) fn intern_const_lookup(&mut self, lookup: ConstLookup) -> ConstLookupId {
        let (index, _) = self.const_lookups.insert_full(lookup);
        ConstLookupId(u32::try_from(index).expect_invariant(
            "constant lookup interner exceeded u32 ids",
            "ConstLookupId stores u32",
            "widen ConstLookupId before interning more than u32::MAX lookups",
        ))
    }

    pub(in crate::engine) fn const_lookup(&self, id: ConstLookupId) -> Option<&ConstLookup> {
        self.const_lookups.get_index(id.0 as usize)
    }

    #[cfg(test)]
    pub(super) fn reset_fqn_lookup_count_for_test(&self) {
        self.fqn_lookup_count.store(0, Ordering::Relaxed);
    }

    #[cfg(test)]
    pub(super) fn fqn_lookup_count_for_test(&self) -> usize {
        self.fqn_lookup_count.load(Ordering::Relaxed)
    }

    pub(super) fn estimated_heap_bytes(&self) -> usize {
        self.fqns.capacity() * (size_of::<FullyQualifiedName>() + size_of::<usize>() + 1)
            + self.fqns.iter().map(fqn_heap_bytes).sum::<usize>()
            + self.const_lookups.capacity() * (size_of::<ConstLookup>() + size_of::<usize>() + 1)
            + self
                .const_lookups
                .iter()
                .map(const_lookup_heap_bytes)
                .sum::<usize>()
    }

    pub(super) fn shrink_to_fit(&mut self) {
        self.fqns.shrink_to_fit();
        self.const_lookups.shrink_to_fit();
    }
}

fn constant_path_heap_bytes(path: &ConstantPath) -> usize {
    if path.spilled() {
        path.capacity() * size_of::<RubyConstant>()
    } else {
        0
    }
}

fn const_lookup_heap_bytes(lookup: &ConstLookup) -> usize {
    constant_path_heap_bytes(&lookup.path)
}
