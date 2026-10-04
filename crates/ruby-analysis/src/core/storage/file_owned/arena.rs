//! File-owned rows with stable ids, for stores that also find rows by a key
//! that spans files.
//!
//! `FileArena` keeps each file's rows in one immutable shared block. Cloning
//! an arena shares every block, so engines seeded from one template share its
//! rows until a file is replaced. A `FileIndex` maps a key to row ids grouped
//! by file in ascending `SourceFileId` order, so one file's ids form a single
//! run that replacement can cut out or splice in without touching other
//! files' ids.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

use super::FileRow;
use crate::core::storage::memory_estimate::{map_table_bytes, vec_payload_bytes};
use crate::core::SourceFileId;
use crate::invariant::ExpectInvariant;

/// A row of one `FileArena`: its file's block and its place in that block.
/// Block ids are reused after their file is removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(in crate::core::storage) struct RowId {
    block: u32,
    row: u32,
}

fn compact_id(index: usize) -> u32 {
    u32::try_from(index).expect_invariant(
        "file arena exceeded u32 block or row ids",
        "file-owned indexes use compact u32 block and row ids",
        "widen RowId and every index that stores it together",
    )
}

/// One file's rows, ordered at insertion and never mutated afterwards.
#[derive(Debug)]
struct FileBlock<T> {
    file_id: SourceFileId,
    rows: Arc<[T]>,
}

impl<T> Clone for FileBlock<T> {
    fn clone(&self) -> Self {
        Self {
            file_id: self.file_id,
            rows: Arc::clone(&self.rows),
        }
    }
}

#[derive(Debug)]
pub(in crate::core::storage) struct FileArena<T> {
    blocks: Vec<Option<FileBlock<T>>>,
    free: Vec<u32>,
    by_file: HashMap<SourceFileId, u32>,
    len: usize,
}

impl<T> Clone for FileArena<T> {
    fn clone(&self) -> Self {
        Self {
            blocks: self.blocks.clone(),
            free: self.free.clone(),
            by_file: self.by_file.clone(),
            len: self.len,
        }
    }
}

impl<T> Default for FileArena<T> {
    fn default() -> Self {
        Self {
            blocks: Vec::new(),
            free: Vec::new(),
            by_file: HashMap::new(),
            len: 0,
        }
    }
}

impl<T> FileArena<T> {
    pub fn get(&self, id: RowId) -> &T {
        self.block(id.block)
            .rows
            .get(id.row as usize)
            .expect_invariant(
                "a file-owned row id points past its file's rows",
                "row ids are created only for rows of an installed block",
                "keep RowId construction inside FileArena::insert_file",
            )
    }

    /// Rows of one file, in the order chosen at insertion.
    pub fn rows_in_file(&self, file_id: SourceFileId) -> impl Iterator<Item = &T> {
        self.by_file
            .get(&file_id)
            .map(|block| &*self.block(*block).rows)
            .unwrap_or(&[])
            .iter()
    }

    /// Every row in block order, each file's rows in insertion order. Block
    /// order is deterministic for a given sequence of replacements.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.blocks
            .iter()
            .flatten()
            .flat_map(|block| block.rows.iter())
    }

    pub fn len(&self) -> usize {
        self.len
    }

    /// Block, free-list, and file-map storage plus each block's row slots.
    /// A block shared with another arena is counted by each owner. Callers
    /// add each row's own heap.
    pub fn estimated_heap_bytes(&self) -> usize {
        vec_payload_bytes(&self.blocks)
            + vec_payload_bytes(&self.free)
            + map_table_bytes(&self.by_file)
            + self
                .blocks
                .iter()
                .flatten()
                .map(|block| std::mem::size_of_val::<[T]>(&block.rows))
                .sum::<usize>()
    }

    pub fn shrink_to_fit(&mut self) {
        self.blocks.shrink_to_fit();
        self.free.shrink_to_fit();
        self.by_file.shrink_to_fit();
    }

    /// Free every row of one file. `unlink` sees each row while all of the
    /// file's rows are still present, so indexes can drop their ids first.
    pub fn remove_file(&mut self, file_id: SourceFileId, mut unlink: impl FnMut(&Self, &T)) {
        let Some(block) = self.by_file.remove(&file_id) else {
            return;
        };
        for row in self.block(block).rows.iter() {
            unlink(self, row);
        }
        let removed = self.blocks[block as usize].take().expect_invariant(
            "a removed file's block slot is already empty",
            "each file maps to the one block it installed",
            "free a block slot only through FileArena::remove_file",
        );
        self.len -= removed.rows.len();
        self.free.push(block);
    }

    fn block(&self, block: u32) -> &FileBlock<T> {
        self.blocks
            .get(block as usize)
            .and_then(Option::as_ref)
            .expect_invariant(
                "a file-owned row id points to an empty block",
                "indexes must drop a file's ids before the arena frees its block",
                "unlink every index inside FileArena::remove_file",
            )
    }

    fn file_of(&self, id: RowId) -> SourceFileId {
        self.block(id.block).file_id
    }
}

impl<T: FileRow> FileArena<T> {
    /// Install the rows of a file that has none. The file's rows are ordered
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
        let mut rows = rows.into_iter().enumerate().collect::<Vec<_>>();
        for (_, row) in &rows {
            invariant!(
                row.file_id() == file_id,
                what = "a replacement row belongs to another file",
                why = "replacing a file must install only that file's rows",
                fix = "partition rows by SourceFileId before replacing a file",
            );
        }
        if rows.is_empty() {
            return Vec::new();
        }
        rows.sort_by(|(_, left), (_, right)| compare(left, right));
        let block = match self.free.pop() {
            Some(block) => block,
            None => {
                self.blocks.push(None);
                compact_id(self.blocks.len() - 1)
            }
        };
        let mut inserted = vec![RowId { block, row: 0 }; rows.len()];
        for (position, (input, _)) in rows.iter().enumerate() {
            inserted[*input].row = compact_id(position);
        }
        let rows = rows.into_iter().map(|(_, row)| row).collect::<Arc<[T]>>();
        self.len += rows.len();
        let slot = &mut self.blocks[block as usize];
        invariant!(
            slot.is_none(),
            what = "the file arena free list points to an occupied block",
            why = "only FileArena::remove_file frees blocks, once each",
            fix = "push a block id to the free list only when its slot is cleared",
        );
        *slot = Some(FileBlock { file_id, rows });
        self.by_file.insert(file_id, block);
        inserted
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
