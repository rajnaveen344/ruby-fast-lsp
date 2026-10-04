//! Interners whose values can be shared between engine clones.
//!
//! `SharedInterner` keeps a frozen, shared prefix of values and appends new
//! values after it, so ids stay dense and stable. Cloning shares the prefix;
//! `freeze` moves the appended values into it before an engine is published
//! as a template, so every engine seeded from that template shares its
//! interned values instead of copying them.

use std::hash::{BuildHasher, Hash};
use std::mem::size_of;
use std::sync::Arc;

use indexmap::map::raw_entry_v1::{RawEntryApiV1, RawEntryMut};
use indexmap::{Equivalent, IndexMap};

type Table<T> = IndexMap<T, ()>;

/// Both tables use one hasher, so a value is hashed once per lookup.
#[derive(Debug)]
pub(crate) struct SharedInterner<T> {
    shared: Arc<Table<T>>,
    own: Table<T>,
}

impl<T> Default for SharedInterner<T> {
    fn default() -> Self {
        let shared = Table::default();
        let own = Table::with_hasher(shared.hasher().clone());
        Self {
            shared: Arc::new(shared),
            own,
        }
    }
}

impl<T: Clone> Clone for SharedInterner<T> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
            own: self.own.clone(),
        }
    }
}

impl<T: Hash + Eq> SharedInterner<T> {
    /// The id of `value`, interning it first when it is new.
    pub(crate) fn intern(&mut self, value: T) -> usize {
        let hash = self.shared.hasher().hash_one(&value);
        if let Some(index) = self
            .shared
            .raw_entry_v1()
            .index_from_hash(hash, |key| *key == value)
        {
            return index;
        }
        let offset = self.shared.len();
        match self
            .own
            .raw_entry_mut_v1()
            .from_hash(hash, |key| *key == value)
        {
            RawEntryMut::Occupied(entry) => offset + entry.index(),
            RawEntryMut::Vacant(entry) => {
                let index = entry.index();
                entry.insert_hashed_nocheck(hash, value, ());
                offset + index
            }
        }
    }

    pub(crate) fn get_index_of<Q: ?Sized + Hash + Equivalent<T>>(
        &self,
        value: &Q,
    ) -> Option<usize> {
        let hash = self.shared.hasher().hash_one(value);
        let is_match = |key: &T| value.equivalent(key);
        self.shared
            .raw_entry_v1()
            .index_from_hash(hash, is_match)
            .or_else(|| {
                self.own
                    .raw_entry_v1()
                    .index_from_hash(hash, is_match)
                    .map(|index| self.shared.len() + index)
            })
    }

    pub(crate) fn get_index(&self, index: usize) -> Option<&T> {
        match index.checked_sub(self.shared.len()) {
            None => self.shared.get_index(index),
            Some(own) => self.own.get_index(own),
        }
        .map(|(value, _)| value)
    }

    /// Every value in id order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &T> {
        self.shared.keys().chain(self.own.keys())
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.shared.len() + self.own.len()
    }

    /// Move the appended values into the shared prefix. Ids do not change.
    pub(crate) fn freeze(&mut self)
    where
        T: Clone,
    {
        if self.own.is_empty() {
            return;
        }
        let empty = Table::with_hasher(self.shared.hasher().clone());
        let mut own = std::mem::replace(&mut self.own, empty);
        if self.shared.is_empty() {
            own.shrink_to_fit();
            self.shared = Arc::new(own);
            return;
        }
        let shared = Arc::make_mut(&mut self.shared);
        shared.extend(own);
        shared.shrink_to_fit();
    }

    /// Table storage plus each value's own heap. A prefix shared with other
    /// engines is counted by each owner.
    pub(crate) fn estimated_heap_bytes(&self, value_heap_bytes: impl Fn(&T) -> usize) -> usize {
        (self.shared.capacity() + self.own.capacity())
            * (size_of::<T>() + 2 * size_of::<usize>() + 1)
            + self.iter().map(value_heap_bytes).sum::<usize>()
    }

    pub(crate) fn shrink_to_fit(&mut self) {
        self.own.shrink_to_fit();
    }
}

#[cfg(test)]
mod tests {
    use super::SharedInterner;

    #[test]
    fn frozen_values_keep_their_ids_and_are_shared_by_clones() {
        let mut names = SharedInterner::default();
        assert_eq!(names.intern("Object".to_string()), 0);
        assert_eq!(names.intern("Kernel".to_string()), 1);
        names.freeze();

        let mut copy = names.clone();
        assert_eq!(copy.intern("Kernel".to_string()), 1);
        assert_eq!(copy.intern("App".to_string()), 2);
        assert!(
            std::ptr::eq(names.get_index(0).unwrap(), copy.get_index(0).unwrap()),
            "a clone must share frozen values instead of copying them"
        );
        assert_eq!(copy.get_index_of("App"), Some(2));
        assert_eq!(names.get_index_of("App"), None);
        assert_eq!(copy.iter().collect::<Vec<_>>(), ["Object", "Kernel", "App"]);

        copy.freeze();
        assert_eq!(copy.get_index_of("App"), Some(2));
        assert_eq!(copy.intern("Object".to_string()), 0);
        assert_eq!((names.len(), copy.len()), (2, 3));
    }

    #[test]
    fn a_lookup_hashes_its_value_once_across_shared_and_own_values() {
        use std::cell::Cell;
        use std::hash::{Hash, Hasher};

        thread_local!(static HASHES: Cell<usize> = const { Cell::new(0) });
        #[derive(Clone, PartialEq, Eq)]
        struct Counted(u32);
        impl Hash for Counted {
            fn hash<H: Hasher>(&self, state: &mut H) {
                HASHES.with(|count| count.set(count.get() + 1));
                self.0.hash(state);
            }
        }
        fn hashes(run: impl FnOnce()) -> usize {
            HASHES.with(|count| count.set(0));
            run();
            HASHES.with(Cell::get)
        }

        let mut values = SharedInterner::default();
        values.intern(Counted(1));
        values.freeze();
        let mut copy = values.clone();

        assert_eq!(hashes(|| assert_eq!(copy.intern(Counted(2)), 1)), 1);
        assert_eq!(hashes(|| assert_eq!(copy.intern(Counted(2)), 1)), 1);
        assert_eq!(
            hashes(|| assert_eq!(copy.get_index_of(&Counted(2)), Some(1))),
            1
        );
        assert_eq!(
            hashes(|| assert_eq!(copy.get_index_of(&Counted(3)), None)),
            1
        );
        copy.freeze();
        assert_eq!(hashes(|| assert_eq!(copy.intern(Counted(3)), 2)), 1);
    }
}
