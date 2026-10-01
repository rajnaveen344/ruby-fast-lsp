//! Rows that belong to exactly one source file.
//!
//! Every store keeps facts owned by one `SourceFileId`, and replacing a file
//! must drop all of that file's previous rows and nothing else. `FileOwned`
//! owns that bookkeeping so stores only describe their rows and lookups.

pub(in crate::core::storage) mod arena;

use std::cmp::Ordering;
use std::collections::HashMap;

use crate::core::storage::memory_estimate::{map_table_bytes, vec_payload_bytes};
use crate::core::SourceFileId;

/// A row that records the file that owns it.
pub(in crate::core::storage) trait FileRow {
    fn file_id(&self) -> SourceFileId;
}

/// Rows grouped by owning file. A file with no rows has no entry.
#[derive(Debug, Clone)]
pub(in crate::core::storage) struct FileOwned<T> {
    files: HashMap<SourceFileId, Vec<T>>,
}

impl<T> Default for FileOwned<T> {
    fn default() -> Self {
        Self {
            files: HashMap::new(),
        }
    }
}

impl<T> FileOwned<T> {
    /// The rows of one file, in the order chosen at replacement.
    pub fn rows(&self, file_id: SourceFileId) -> &[T] {
        self.files.get(&file_id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Every row, grouped by file in unspecified file order.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.files.values().flatten()
    }

    /// Every file that owns at least one row, in unspecified order.
    pub fn files(&self) -> impl Iterator<Item = SourceFileId> + '_ {
        self.files.keys().copied()
    }

    pub fn len(&self) -> usize {
        self.files.values().map(Vec::len).sum()
    }

    /// Drop every row of one file. Returns the rows it removed.
    pub fn remove(&mut self, file_id: SourceFileId) -> Option<Vec<T>> {
        self.files.remove(&file_id)
    }

    /// Table and row storage, plus each row's own heap from `row_heap_bytes`.
    pub fn estimated_heap_bytes(&self, row_heap_bytes: impl Fn(&T) -> usize) -> usize {
        map_table_bytes(&self.files)
            + self
                .files
                .values()
                .map(|rows| {
                    vec_payload_bytes(rows) + rows.iter().map(&row_heap_bytes).sum::<usize>()
                })
                .sum::<usize>()
    }

    pub fn shrink_to_fit(&mut self) {
        self.files.shrink_to_fit();
        for rows in self.files.values_mut() {
            rows.shrink_to_fit();
        }
    }
}

impl<T: FileRow> FileOwned<T> {
    /// Make `rows` the whole content of one file, ordered by a stable sort
    /// with `compare`. Returns the rows it replaced.
    pub fn replace(
        &mut self,
        file_id: SourceFileId,
        rows: impl IntoIterator<Item = T>,
        compare: impl FnMut(&T, &T) -> Ordering,
    ) -> Option<Vec<T>> {
        let mut rows = rows.into_iter().collect::<Vec<_>>();
        for row in &rows {
            invariant!(
                row.file_id() == file_id,
                what = "a replacement row belongs to another file",
                why = "replacing a file must install only that file's rows",
                fix = "partition rows by SourceFileId before replacing a file",
            );
        }
        if rows.is_empty() {
            return self.files.remove(&file_id);
        }
        rows.sort_by(compare);
        rows.shrink_to_fit();
        self.files.insert(file_id, rows)
    }
}

#[cfg(test)]
mod tests;
