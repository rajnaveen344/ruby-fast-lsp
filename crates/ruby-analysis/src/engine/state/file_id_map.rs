use std::collections::HashMap;
use std::mem::size_of;
use std::path::{Path, PathBuf};

use crate::core::SourceFileId;

/// Stable source-file id allocator.
///
/// Adapters map editor URIs or agent paths to canonical paths before asking for
/// ids. The analysis engine keeps ids stable for the process lifetime.
#[derive(Debug, Clone, Default)]
pub struct FileIdMap {
    by_path: HashMap<PathBuf, SourceFileId>,
    next_id: u32,
}

impl FileIdMap {
    pub fn get_or_insert(&mut self, path: impl AsRef<Path>) -> SourceFileId {
        let path = normalize_path(path.as_ref());
        if let Some(id) = self.by_path.get(&path) {
            return *id;
        }

        let id = SourceFileId(self.next_id);
        self.next_id = self.next_id.checked_add(1).expect(
            "INVARIANT VIOLATED: source file id allocator overflowed u32. \
             This is a bug because SourceFileId currently stores u32 ids. \
             Fix: widen SourceFileId before indexing more than u32::MAX files.",
        );
        self.by_path.insert(path, id);
        id
    }

    pub fn get(&self, path: impl AsRef<Path>) -> Option<SourceFileId> {
        self.by_path.get(&normalize_path(path.as_ref())).copied()
    }

    pub fn shrink_to_fit(&mut self) {
        self.by_path.shrink_to_fit();
    }

    pub fn estimated_heap_bytes(&self) -> usize {
        self.by_path.capacity() * (size_of::<PathBuf>() + size_of::<SourceFileId>() + 1)
            + self
                .by_path
                .keys()
                .map(|path| path_heap_bytes(path.as_path()))
                .sum::<usize>()
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    path.components().collect()
}

fn path_heap_bytes(path: &Path) -> usize {
    path.as_os_str().len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_path_gets_same_id() {
        let mut ids = FileIdMap::default();

        let first = ids.get_or_insert("app/user.rb");
        let second = ids.get_or_insert("app/user.rb");

        assert_eq!(first, second);
        assert_eq!(ids.get("app/user.rb"), Some(first));
    }

    #[test]
    fn different_paths_get_different_ids() {
        let mut ids = FileIdMap::default();

        let first = ids.get_or_insert("app/user.rb");
        let second = ids.get_or_insert("app/team.rb");

        assert_ne!(first, second);
        assert_eq!(ids.get("app/user.rb"), Some(first));
        assert_eq!(ids.get("app/team.rb"), Some(second));
    }
}
