//! `Files`: the engine's registered sources. It allocates stable file ids,
//! keeps each file's line index (and non-ASCII text), issues revision
//! snapshots for background commits, and records each file's semantic export
//! fingerprint.

use crate::invariant::ExpectInvariant;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::mem::size_of;
use std::path::{Path, PathBuf};

use crate::core::storage::memory_estimate::vec_payload_bytes;
use crate::core::{LibraryPackageId, SourceFileId, SourceKind, TextRange};
use crate::engine::persist::fingerprint::SemanticExportFingerprint;

use super::Project;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub id: SourceFileId,
    pub path: PathBuf,
    pub source: Option<String>,
    pub line_index: SourceLineIndex,
    pub content_hash: u64,
    pub kind: SourceKind,
    revision: u64,
    /// Present for `SourceKind::Gem` files bound from a locked package.
    pub library_package: Option<LibraryPackageId>,
}

/// Opaque identity of one registered file snapshot in one engine.
///
/// Keeping the engine, file, and revision identities together prevents a fact
/// batch from accidentally validating against another file or cloned engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceFileSnapshot {
    pub(super) engine_instance_id: u64,
    pub(super) file_id: SourceFileId,
    revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SourceLineIndex {
    line_offsets: Vec<u32>,
    len: u32,
    ascii: bool,
}

impl SourceLineIndex {
    fn new(source: &str) -> Self {
        let len = u32::try_from(source.len()).expect_invariant(
            "source file byte length exceeded u32",
            "every analysis TextRange and SourceFileId-relative byte offset is represented as u32",
            "reject or segment files larger than u32::MAX before registration",
        );
        let mut line_offsets = vec![0];
        for (idx, byte) in source.bytes().enumerate() {
            if byte == b'\n' {
                line_offsets.push(u32::try_from(idx + 1).expect_invariant(
                    "source line offset exceeded u32 after the complete source length fit u32",
                    "a position within a bounded source cannot exceed its length",
                    "keep source length validation before line-index construction",
                ));
            }
        }
        if line_offsets.last() != Some(&len) {
            line_offsets.push(len);
        }
        Self {
            line_offsets,
            len,
            ascii: source.is_ascii(),
        }
    }

    pub fn line_offsets(&self) -> &[u32] {
        &self.line_offsets
    }

    pub fn len(&self) -> usize {
        self.len as usize
    }

    pub fn is_ascii(&self) -> bool {
        self.ascii
    }

    fn shrink_to_fit(&mut self) {
        self.line_offsets.shrink_to_fit();
    }
}

impl SourceFile {
    pub fn source_text(&self) -> Option<&str> {
        self.source.as_deref()
    }

    pub fn byte_offset_to_line_character(&self, byte_offset: u32) -> Option<(u32, u32)> {
        let target = byte_offset;
        if target > self.line_index.len {
            return None;
        }
        let line_index = match self.line_index.line_offsets.binary_search(&target) {
            Ok(exact) => exact,
            Err(after) => after.saturating_sub(1),
        };
        let line_start = *self.line_index.line_offsets.get(line_index)?;
        let character = if self.line_index.ascii {
            target.checked_sub(line_start)?
        } else {
            let source = self.source.as_deref()?;
            let target = target as usize;
            let line_start = line_start as usize;
            if !source.is_char_boundary(target) {
                return None;
            }
            source[line_start..target]
                .chars()
                .map(char::len_utf16)
                .sum::<usize>()
                .try_into()
                .ok()?
        };
        Some((
            u32::try_from(line_index).expect_invariant(
                "source line index exceeded u32",
                "LSP positions require u32 lines",
                "reject or segment files with more than u32::MAX lines",
            ),
            character,
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFileInput {
    pub path: PathBuf,
    pub content: String,
    pub kind: SourceKind,
}

/// Registered sources of one engine.
///
/// Adapters map editor URIs or agent paths to canonical paths before asking
/// for ids. File ids stay stable for the engine's lifetime.
#[derive(Debug, Clone, Default)]
pub(in crate::engine) struct Files {
    ids_by_path: HashMap<PathBuf, SourceFileId>,
    next_id: u32,
    files: HashMap<SourceFileId, SourceFile>,
    next_revision: u64,
    export_fingerprints: HashMap<SourceFileId, SemanticExportFingerprint>,
}

impl Files {
    pub(in crate::engine) fn id(&self, path: impl AsRef<Path>) -> Option<SourceFileId> {
        self.ids_by_path
            .get(&normalize_path(path.as_ref()))
            .copied()
    }

    pub(in crate::engine) fn get(&self, id: SourceFileId) -> Option<&SourceFile> {
        self.files.get(&id)
    }

    pub(in crate::engine) fn ids(&self) -> impl Iterator<Item = SourceFileId> + '_ {
        self.files.keys().copied()
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &SourceFile> {
        self.files.values()
    }

    pub(super) fn len(&self) -> usize {
        self.files.len()
    }

    pub(super) fn source_bytes(&self) -> usize {
        self.files.values().map(|file| file.line_index.len()).sum()
    }

    pub(super) fn content_matches(&self, id: SourceFileId, content: &str) -> bool {
        self.get(id)
            .is_some_and(|file| file.content_hash == source_hash(content))
    }

    pub(super) fn assert_known(&self, file_id: SourceFileId, message: &str) {
        invariant!(
            self.files.contains_key(&file_id),
            what = "{message}",
            why = "analysis facts and ranges must only reference registered files",
            fix = "call Project::register_file before adding file facts",
            message = message,
        );
    }

    fn id_or_insert(&mut self, path: impl AsRef<Path>) -> SourceFileId {
        let path = normalize_path(path.as_ref());
        if let Some(id) = self.ids_by_path.get(&path) {
            return *id;
        }

        let id = SourceFileId(self.next_id);
        self.next_id = self.next_id.checked_add(1).expect_invariant(
            "source file id allocator overflowed u32",
            "SourceFileId currently stores u32 ids",
            "widen SourceFileId before indexing more than u32::MAX files",
        );
        self.ids_by_path.insert(path, id);
        id
    }

    fn register_owned(
        &mut self,
        file: SourceFileInput,
        library_package: Option<LibraryPackageId>,
    ) -> SourceFileId {
        let line_index = SourceLineIndex::new(&file.content);
        let content_hash = source_hash(&file.content);
        let source = if line_index.is_ascii() {
            None
        } else {
            Some(file.content)
        };
        self.register_indexed(
            file.path,
            file.kind,
            line_index,
            content_hash,
            source,
            library_package,
        )
    }

    /// Register source whose caller retains the owned buffer. ASCII files need
    /// only their line index and content hash after collection; non-ASCII files
    /// retain one engine-owned copy for exact UTF-16 conversion.
    fn register_borrowed(
        &mut self,
        path: PathBuf,
        content: &str,
        kind: SourceKind,
    ) -> SourceFileId {
        let line_index = SourceLineIndex::new(content);
        let content_hash = source_hash(content);
        let source = if line_index.is_ascii() {
            None
        } else {
            Some(content.to_string())
        };
        self.register_indexed(path, kind, line_index, content_hash, source, None)
    }

    fn register_indexed(
        &mut self,
        path: PathBuf,
        kind: SourceKind,
        line_index: SourceLineIndex,
        content_hash: u64,
        source: Option<String>,
        library_package: Option<LibraryPackageId>,
    ) -> SourceFileId {
        let id = self.id_or_insert(&path);
        if self.files.get(&id).is_some_and(|existing| {
            existing.path == path
                && existing.kind == kind
                && existing.line_index == line_index
                && existing.content_hash == content_hash
                && existing.source == source
                && existing.library_package == library_package
        }) {
            return id;
        }
        self.next_revision = self.next_revision.checked_add(1).expect_invariant(
            "analysis engine source revision exhausted u64",
            "stale background commits need monotonic source identity",
            "widen the source snapshot revision",
        );
        self.files.insert(
            id,
            SourceFile {
                id,
                path: path.components().collect(),
                source,
                line_index,
                content_hash,
                kind,
                revision: self.next_revision,
                library_package,
            },
        );
        id
    }

    fn snapshot_for_path(
        &self,
        path: impl AsRef<Path>,
        engine_instance_id: u64,
    ) -> Option<SourceFileSnapshot> {
        let file_id = self.id(path)?;
        let file = self.get(file_id)?;
        Some(SourceFileSnapshot {
            engine_instance_id,
            file_id,
            revision: file.revision,
        })
    }

    fn register_borrowed_if_snapshot(
        &mut self,
        path: PathBuf,
        content: &str,
        kind: SourceKind,
        expected_snapshot: Option<SourceFileSnapshot>,
        engine_instance_id: u64,
    ) -> Option<SourceFileSnapshot> {
        if let Some(expected) = expected_snapshot {
            invariant_eq!(
                expected.engine_instance_id,
                engine_instance_id,
                what = "conditional source registration received a snapshot from another analysis engine",
                why = "source revisions are engine-local lifecycle identities",
                fix = "capture and commit the snapshot through the same isolated project engine",
            );
        }
        if self.snapshot_for_path(&path, engine_instance_id) != expected_snapshot {
            return None;
        }
        let file_id = self.register_borrowed(path, content, kind);
        let file = self.get(file_id).unwrap_or_else(|| {
            unreachable_invariant!(
                what = "conditional source registration lost file id {:?}",
                why = "registration and revision capture occur under one engine write borrow",
                fix = "keep Files insertion atomic",
                file_id,
            )
        });
        Some(SourceFileSnapshot {
            engine_instance_id,
            file_id,
            revision: file.revision,
        })
    }

    /// Whether `snapshot`'s file is still at the captured revision. The caller
    /// checks the engine identity first.
    pub(super) fn is_current(&self, snapshot: SourceFileSnapshot) -> bool {
        self.get(snapshot.file_id).map(|file| file.revision) == Some(snapshot.revision)
    }

    pub(super) fn record_export_fingerprint(
        &mut self,
        file_id: SourceFileId,
        fingerprint: SemanticExportFingerprint,
    ) -> Option<SemanticExportFingerprint> {
        self.export_fingerprints.insert(file_id, fingerprint)
    }

    pub(in crate::engine) fn export_fingerprint(
        &self,
        file_id: SourceFileId,
    ) -> Option<SemanticExportFingerprint> {
        self.export_fingerprints.get(&file_id).copied()
    }

    pub(in crate::engine) fn export_fingerprints(
        &self,
    ) -> impl Iterator<Item = (&SourceFileId, &SemanticExportFingerprint)> {
        self.export_fingerprints.iter()
    }

    pub(super) fn estimated_heap_bytes(&self) -> usize {
        self.ids_by_path.capacity() * (size_of::<PathBuf>() + size_of::<SourceFileId>() + 1)
            + self
                .ids_by_path
                .keys()
                .map(|path| path.as_os_str().len())
                .sum::<usize>()
            + self.files.capacity() * (size_of::<SourceFileId>() + size_of::<SourceFile>() + 1)
            + self
                .files
                .values()
                .map(|file| {
                    file.path.as_os_str().len()
                        + file.source.as_ref().map(String::capacity).unwrap_or(0)
                        + vec_payload_bytes(&file.line_index.line_offsets)
                })
                .sum::<usize>()
            + self.export_fingerprints.capacity()
                * (size_of::<SourceFileId>() + size_of::<SemanticExportFingerprint>() + 1)
    }

    pub(super) fn shrink_to_fit(&mut self) {
        self.ids_by_path.shrink_to_fit();
        self.files.shrink_to_fit();
        for file in self.files.values_mut() {
            file.path.shrink_to_fit();
            if let Some(source) = &mut file.source {
                source.shrink_to_fit();
            }
            file.line_index.shrink_to_fit();
        }
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    path.components().collect()
}

fn source_hash(source: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}

impl Project {
    pub fn register_file(&mut self, file: SourceFileInput) -> SourceFileId {
        self.files.register_owned(file, None)
    }

    /// Register a locked gem source with explicit package identity for library-tree grouping.
    pub fn register_gem_file(
        &mut self,
        file: SourceFileInput,
        package: LibraryPackageId,
    ) -> SourceFileId {
        invariant!(
            file.kind == SourceKind::Gem,
            what = "register_gem_file received SourceKind::{:?}",
            why = "only Gem sources carry locked package identity",
            fix = "use register_file for non-gem sources or pass SourceKind::Gem",
            file.kind,
        );
        self.files.register_owned(file, Some(package))
    }

    /// Register source whose caller retains the owned buffer. ASCII files need
    /// only their line index and content hash after collection; non-ASCII files
    /// retain one engine-owned copy for exact UTF-16 conversion.
    pub fn register_file_borrowed(
        &mut self,
        path: PathBuf,
        content: &str,
        kind: SourceKind,
    ) -> SourceFileId {
        self.files.register_borrowed(path, content, kind)
    }

    pub fn source_snapshot_for_path(&self, path: impl AsRef<Path>) -> Option<SourceFileSnapshot> {
        self.files.snapshot_for_path(path, self.instance_id)
    }

    /// Register a source only if no newer registration occurred after the
    /// caller captured `expected_snapshot`.
    pub fn register_file_borrowed_if_snapshot(
        &mut self,
        path: PathBuf,
        content: &str,
        kind: SourceKind,
        expected_snapshot: Option<SourceFileSnapshot>,
    ) -> Option<SourceFileSnapshot> {
        self.files.register_borrowed_if_snapshot(
            path,
            content,
            kind,
            expected_snapshot,
            self.instance_id,
        )
    }

    pub fn file_id(&self, path: impl AsRef<Path>) -> Option<SourceFileId> {
        self.files.id(path)
    }

    pub fn file(&self, id: SourceFileId) -> Option<&SourceFile> {
        self.files.get(id)
    }

    pub fn file_content_matches(&self, id: SourceFileId, content: &str) -> bool {
        self.files.content_matches(id, content)
    }

    pub fn files(&self) -> impl Iterator<Item = &SourceFile> {
        self.files.iter()
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn text_range(&self, file_id: SourceFileId, start_byte: u32, end_byte: u32) -> TextRange {
        self.files
            .assert_known(file_id, "TextRange requested for unknown source file id");
        TextRange::new(file_id, start_byte, end_byte)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_path_gets_same_id() {
        let mut files = Files::default();

        let first = files.id_or_insert("app/user.rb");
        let second = files.id_or_insert("app/user.rb");

        assert_eq!(first, second);
        assert_eq!(files.id("app/user.rb"), Some(first));
    }

    #[test]
    fn different_paths_get_different_ids() {
        let mut files = Files::default();

        let first = files.id_or_insert("app/user.rb");
        let second = files.id_or_insert("app/team.rb");

        assert_ne!(first, second);
        assert_eq!(files.id("app/user.rb"), Some(first));
        assert_eq!(files.id("app/team.rb"), Some(second));
    }
}
