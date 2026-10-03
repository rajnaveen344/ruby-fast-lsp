//! File-owned rows with stable ids, for stores that also find rows by a key
//! that spans files.
//!
//! `FileArena` keeps each row in one slot and lists each file's slots. A
//! `FileIndex` maps a key to row ids grouped by file in ascending
//! `SourceFileId` order, so one file's ids form a single run that replacement
//! can cut out or splice in without touching other files' ids.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::hash::Hash;

use super::FileRow;
use crate::core::storage::memory_estimate::{map_table_bytes, vec_payload_bytes};
use crate::core::SourceFileId;
use crate::invariant::ExpectInvariant;

/// A slot in one `FileArena`. Ids are reused after their file is removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(in crate::core::storage) struct RowId(u32);

impl RowId {
    fn from_index(index: usize) -> Self {
        Self(u32::try_from(index).expect_invariant(
            "file arena exceeded u32 row ids",
            "file-owned indexes use compact u32 row ids",
            "widen RowId and every index that stores it together",
        ))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone)]
pub(in crate::core::storage) struct FileArena<T> {
    rows: Vec<Option<T>>,
    free: Vec<RowId>,
    by_file: HashMap<SourceFileId, Vec<RowId>>,
}

impl<T> Default for FileArena<T> {
    fn default() -> Self {
        Self {
            rows: Vec::new(),
            free: Vec::new(),
            by_file: HashMap::new(),
        }
    }
}

impl<T> FileArena<T> {
    pub fn get(&self, id: RowId) -> &T {
        self.rows
            .get(id.index())
            .and_then(Option::as_ref)
            .expect_invariant(
                "a file-owned row id points to an empty slot",
                "indexes must drop a file's ids before the arena frees them",
                "unlink every index inside FileArena::remove_file",
            )
    }

    /// Row ids of one file, in the order chosen at insertion.
    pub fn ids_in_file(&self, file_id: SourceFileId) -> &[RowId] {
        self.by_file.get(&file_id).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn rows_in_file(&self, file_id: SourceFileId) -> impl Iterator<Item = &T> {
        self.ids_in_file(file_id).iter().map(|id| self.get(*id))
    }

    /// Every row in slot order. Slot order is deterministic for a given
    /// sequence of replacements.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.rows.iter().flatten()
    }

    pub fn len(&self) -> usize {
        self.rows.len() - self.free.len()
    }

    /// Slot, free-list, and file-list storage. Callers add each row's own heap.
    pub fn estimated_heap_bytes(&self) -> usize {
        vec_payload_bytes(&self.rows)
            + vec_payload_bytes(&self.free)
            + map_table_bytes(&self.by_file)
            + self.by_file.values().map(vec_payload_bytes).sum::<usize>()
    }

    pub fn shrink_to_fit(&mut self) {
        self.rows.shrink_to_fit();
        self.free.shrink_to_fit();
        self.by_file.shrink_to_fit();
        for ids in self.by_file.values_mut() {
            ids.shrink_to_fit();
        }
    }

    /// Free every row of one file. `unlink` sees each row while all of the
    /// file's rows are still present, so indexes can drop their ids first.
    pub fn remove_file(&mut self, file_id: SourceFileId, mut unlink: impl FnMut(&Self, &T)) {
        let Some(ids) = self.by_file.remove(&file_id) else {
            return;
        };
        for id in &ids {
            unlink(self, self.get(*id));
        }
        for id in ids {
            self.rows[id.index()] = None;
            self.free.push(id);
        }
    }

    fn insert(&mut self, row: T) -> RowId {
        let Some(id) = self.free.pop() else {
            let id = RowId::from_index(self.rows.len());
            self.rows.push(Some(row));
            return id;
        };
        let slot = &mut self.rows[id.index()];
        invariant!(
            slot.is_none(),
            what = "the file arena free list points to an occupied slot",
            why = "only FileArena::remove_file frees slots, once each",
            fix = "push a row id to the free list only when its slot is cleared",
        );
        *slot = Some(row);
        id
    }
}

impl<T: FileRow> FileArena<T> {
    /// Install the rows of a file that has none. The file's list is ordered
    /// by a stable sort with `compare`; the returned ids keep input order.
    pub fn insert_file(
        &mut self,
        file_id: SourceFileId,
        rows: impl IntoIterator<Item = T>,
        mut compare: impl FnMut(&T, &T) -> Ordering,
    ) -> Vec<RowId> {
        invariant!(
            !self.by_file.contains_key(&file_id),
            what = "a file's rows were inserted while its old rows remain",
            why = "replacement must drop every stale row of the file",
            fix = "call FileArena::remove_file before insert_file",
        );
        let mut inserted = Vec::new();
        for row in rows {
            invariant!(
                row.file_id() == file_id,
                what = "a replacement row belongs to another file",
                why = "replacing a file must install only that file's rows",
                fix = "partition rows by SourceFileId before replacing a file",
            );
            inserted.push(self.insert(row));
        }
        if inserted.is_empty() {
            return inserted;
        }
        let mut in_file = inserted.clone();
        in_file.sort_by(|left, right| compare(self.get(*left), self.get(*right)));
        self.by_file.insert(file_id, in_file);
        inserted
    }

    fn file_of(&self, id: RowId) -> SourceFileId {
        self.get(id).file_id()
    }
}

/// Row ids by key. Each bucket holds ids grouped by file, files ascending.
#[derive(Debug, Clone)]
pub(in crate::core::storage) struct FileIndex<K> {
    buckets: HashMap<K, Vec<RowId>>,
}

impl<K> Default for FileIndex<K> {
    fn default() -> Self {
        Self {
            buckets: HashMap::new(),
        }
    }
}

impl<K: Copy + Eq + Hash> FileIndex<K> {
    pub fn get(&self, key: &K) -> &[RowId] {
        self.buckets.get(key).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Drop the ids of one file from one key's bucket.
    pub fn unlink<T: FileRow>(&mut self, key: K, file_id: SourceFileId, arena: &FileArena<T>) {
        let Some(ids) = self.buckets.get_mut(&key) else {
            return;
        };
        let start = ids.partition_point(|id| arena.file_of(*id) < file_id);
        let end = start + ids[start..].partition_point(|id| arena.file_of(*id) == file_id);
        ids.drain(start..end);
        if ids.is_empty() {
            self.buckets.remove(&key);
        }
    }

    /// Add one file's ids. Each key's run is ordered by a stable sort with
    /// `compare` and placed among the other files in file order.
    pub fn link<T: FileRow>(
        &mut self,
        file_id: SourceFileId,
        entries: impl IntoIterator<Item = (K, RowId)>,
        arena: &FileArena<T>,
        mut compare: impl FnMut(&T, &T) -> Ordering,
    ) {
        let mut runs: HashMap<K, Vec<RowId>> = HashMap::new();
        for (key, id) in entries {
            runs.entry(key).or_default().push(id);
        }
        for (key, mut run) in runs {
            run.sort_by(|left, right| compare(arena.get(*left), arena.get(*right)));
            let ids = self.buckets.entry(key).or_default();
            let at = ids.partition_point(|id| arena.file_of(*id) < file_id);
            invariant!(
                ids.get(at).is_none_or(|id| arena.file_of(*id) != file_id),
                what = "an index still holds ids of the file being linked",
                why = "replacement must unlink a file's stale ids first",
                fix = "unlink every index key inside FileArena::remove_file",
            );
            ids.splice(at..at, run);
            ids.shrink_to_fit();
        }
    }

    pub fn estimated_heap_bytes(&self) -> usize {
        map_table_bytes(&self.buckets) + self.buckets.values().map(vec_payload_bytes).sum::<usize>()
    }

    pub fn shrink_to_fit(&mut self) {
        self.buckets.shrink_to_fit();
        for ids in self.buckets.values_mut() {
            ids.shrink_to_fit();
        }
    }
}
