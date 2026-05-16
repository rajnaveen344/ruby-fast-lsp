use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ruby_analysis_core::SourceFileId;

/// Stable source-file id allocator.
///
/// Adapters map editor URIs or agent paths to canonical paths before asking for
/// ids. The analysis engine keeps ids stable for the process lifetime.
#[derive(Debug, Clone, Default)]
pub struct FileIdMap {
    by_path: HashMap<PathBuf, SourceFileId>,
    by_id: HashMap<SourceFileId, PathBuf>,
    next_id: u32,
}

impl FileIdMap {
    pub fn new() -> Self {
        Self::default()
    }

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
        self.by_path.insert(path.clone(), id);
        self.by_id.insert(id, path);
        id
    }

    pub fn get(&self, path: impl AsRef<Path>) -> Option<SourceFileId> {
        self.by_path.get(&normalize_path(path.as_ref())).copied()
    }

    pub fn path(&self, id: SourceFileId) -> Option<&Path> {
        self.by_id.get(&id).map(PathBuf::as_path)
    }

    pub fn len(&self) -> usize {
        self.by_path.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_path.is_empty()
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    path.components().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_path_gets_same_id() {
        let mut ids = FileIdMap::new();

        let first = ids.get_or_insert("app/user.rb");
        let second = ids.get_or_insert("app/user.rb");

        assert_eq!(first, second);
        assert_eq!(ids.len(), 1);
    }

    #[test]
    fn different_paths_get_different_ids() {
        let mut ids = FileIdMap::new();

        let first = ids.get_or_insert("app/user.rb");
        let second = ids.get_or_insert("app/team.rb");

        assert_ne!(first, second);
        assert_eq!(ids.path(first), Some(Path::new("app/user.rb")));
        assert_eq!(ids.path(second), Some(Path::new("app/team.rb")));
    }
}
