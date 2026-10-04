//! Method visibility overrides (`private :name`) owned by file, with interned
//! owners and an index from each method name to the files that override it.

use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::mem::size_of;

use super::MethodVisibility;
use crate::core::names::fqn_id::FqnId;
use crate::core::storage::memory_estimate::{map_table_bytes, vec_payload_bytes};
use crate::core::{RubyMethod, SourceFileId, TextRange};

/// One override as the engine stores it; the range is in its owning file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StoredVisibilityOverride {
    pub(crate) owner: FqnId,
    pub(crate) method: RubyMethod,
    pub(crate) visibility: MethodVisibility,
    start_byte: u32,
    end_byte: u32,
}

impl StoredVisibilityOverride {
    pub(crate) fn new(
        owner: FqnId,
        method: RubyMethod,
        visibility: MethodVisibility,
        range: TextRange,
    ) -> Self {
        Self {
            owner,
            method,
            visibility,
            start_byte: range.start_byte,
            end_byte: range.end_byte,
        }
    }

    pub(crate) fn range(&self, file_id: SourceFileId) -> TextRange {
        TextRange::new(file_id, self.start_byte, self.end_byte)
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct VisibilityOverrideStore {
    by_file: HashMap<SourceFileId, Box<[StoredVisibilityOverride]>>,
    /// Sorted, distinct files with an override of each method name.
    files_by_method: HashMap<RubyMethod, Vec<SourceFileId>>,
}

impl VisibilityOverrideStore {
    pub(crate) fn replace_file(
        &mut self,
        file_id: SourceFileId,
        overrides: Box<[StoredVisibilityOverride]>,
    ) {
        self.remove_file(file_id);
        if overrides.is_empty() {
            return;
        }
        let mut methods = overrides.iter().map(|fact| fact.method).collect::<Vec<_>>();
        methods.sort_unstable();
        methods.dedup();
        for method in methods {
            let files = self.files_by_method.entry(method).or_default();
            if let Err(index) = files.binary_search(&file_id) {
                files.insert(index, file_id);
            }
        }
        self.by_file.insert(file_id, overrides);
    }

    pub(crate) fn remove_file(&mut self, file_id: SourceFileId) {
        let Some(old) = self.by_file.remove(&file_id) else {
            return;
        };
        for fact in old.iter() {
            let Entry::Occupied(mut files) = self.files_by_method.entry(fact.method) else {
                continue;
            };
            if let Ok(index) = files.get().binary_search(&file_id) {
                files.get_mut().remove(index);
            }
            if files.get().is_empty() {
                files.remove();
            }
        }
    }

    pub(crate) fn in_file(&self, file_id: SourceFileId) -> &[StoredVisibilityOverride] {
        self.by_file.get(&file_id).map_or(&[], |facts| facts)
    }

    /// Every override of `method`, by ascending file id, then in file order.
    pub(crate) fn named(
        &self,
        method: RubyMethod,
    ) -> impl Iterator<Item = (SourceFileId, &StoredVisibilityOverride)> {
        self.files_by_method
            .get(&method)
            .into_iter()
            .flatten()
            .flat_map(move |file_id| {
                self.in_file(*file_id)
                    .iter()
                    .filter(move |fact| fact.method == method)
                    .map(move |fact| (*file_id, fact))
            })
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (SourceFileId, &StoredVisibilityOverride)> {
        self.by_file
            .iter()
            .flat_map(|(file_id, facts)| facts.iter().map(move |fact| (*file_id, fact)))
    }

    pub(crate) fn estimated_heap_bytes(&self) -> usize {
        map_table_bytes(&self.by_file)
            + self
                .by_file
                .values()
                .map(|facts| facts.len() * size_of::<StoredVisibilityOverride>())
                .sum::<usize>()
            + map_table_bytes(&self.files_by_method)
            + self
                .files_by_method
                .values()
                .map(vec_payload_bytes)
                .sum::<usize>()
    }

    pub(crate) fn shrink_to_fit(&mut self) {
        self.by_file.shrink_to_fit();
        self.files_by_method.shrink_to_fit();
        for files in self.files_by_method.values_mut() {
            files.shrink_to_fit();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{StoredVisibilityOverride, VisibilityOverrideStore};
    use crate::core::names::fqn_id::FqnId;
    use crate::core::{MethodVisibility, RubyMethod, SourceFileId, TextRange};

    fn fact(file: u32, owner: u32, method: &str, start: u32) -> StoredVisibilityOverride {
        StoredVisibilityOverride::new(
            FqnId(owner),
            RubyMethod::new(method).unwrap(),
            MethodVisibility::Private,
            TextRange::new(SourceFileId(file), start, start + 1),
        )
    }

    fn named(store: &VisibilityOverrideStore, method: &str) -> Vec<(u32, u32)> {
        store
            .named(RubyMethod::new(method).unwrap())
            .map(|(file, fact)| (file.0, fact.range(file).start_byte))
            .collect()
    }

    #[test]
    fn replacing_a_file_updates_its_overrides_and_the_method_index() {
        let mut store = VisibilityOverrideStore::default();
        store.replace_file(
            SourceFileId(2),
            Box::new([
                fact(2, 1, "secret", 0),
                fact(2, 1, "hidden", 5),
                fact(2, 3, "secret", 9),
            ]),
        );
        store.replace_file(SourceFileId(1), Box::new([fact(1, 1, "secret", 4)]));

        assert_eq!(named(&store, "secret"), [(1, 4), (2, 0), (2, 9)]);
        assert_eq!(named(&store, "hidden"), [(2, 5)]);
        assert_eq!(store.in_file(SourceFileId(2)).len(), 3);

        store.replace_file(SourceFileId(2), Box::new([fact(2, 1, "hidden", 7)]));
        assert_eq!(named(&store, "secret"), [(1, 4)]);
        assert_eq!(named(&store, "hidden"), [(2, 7)]);

        store.remove_file(SourceFileId(1));
        store.replace_file(SourceFileId(2), Box::new([]));
        assert_eq!(named(&store, "secret"), []);
        assert_eq!(store.iter().count(), 0);
        assert!(store.files_by_method.is_empty());
    }
}
