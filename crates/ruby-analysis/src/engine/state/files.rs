//! `Files`: the engine's registered sources. It allocates stable file ids,
//! keeps each file's line index (and non-ASCII text), issues revision
//! snapshots for background commits, and records each file's semantic export
//! fingerprint. Each registered file is immutable and shared, so cloned
//! engines share every file they have not re-registered.

use crate::invariant::ExpectInvariant;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::mem::size_of;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::core::{LibraryPackageId, SourceFileId, SourceKind, TextRange};
use crate::engine::persist::fingerprint::SemanticExportFingerprint;

use super::Project;
use crate::engine::View;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub id: SourceFileId,
    /// Shared with the engine's path-to-id map.
    pub path: Arc<Path>,
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
    /// Each line's start, then the source length when the last line has no
    /// newline.
    line_offsets: Box<[u32]>,
    len: u32,
    ends_with_newline: bool,
    non_ascii: NonAsciiLines,
}

/// The text of the lines that hold non-ASCII bytes. Those lines need their
/// text for UTF-16 positions and rename checks; an ASCII line maps each byte
/// to one UTF-16 unit, so it keeps no text.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct NonAsciiLines {
    /// Sorted line numbers, each with the start of its text in `text`.
    lines: Box<[(u32, u32)]>,
    text: Box<str>,
}

impl NonAsciiLines {
    fn new(source: &str, line_offsets: &[u32]) -> Self {
        if source.is_ascii() {
            return Self::default();
        }
        let mut lines = Vec::new();
        let mut text = String::new();
        for (line, start) in line_offsets.iter().enumerate() {
            let end = line_offsets
                .get(line + 1)
                .map_or(source.len(), |end| *end as usize);
            let line_text = &source[*start as usize..end];
            if line_text.is_ascii() {
                continue;
            }
            lines.push((as_u32(line), as_u32(text.len())));
            text.push_str(line_text);
        }
        Self {
            lines: lines.into_boxed_slice(),
            text: text.into_boxed_str(),
        }
    }

    /// The text of `line` including its newline, when the line is non-ASCII.
    fn line(&self, line: u32) -> Option<&str> {
        let index = self
            .lines
            .binary_search_by_key(&line, |(line, _)| *line)
            .ok()?;
        let start = self.lines[index].1 as usize;
        let end = self
            .lines
            .get(index + 1)
            .map_or(self.text.len(), |(_, end)| *end as usize);
        Some(&self.text[start..end])
    }
}

/// A line number or text offset of a source whose length fits u32.
fn as_u32(value: usize) -> u32 {
    u32::try_from(value).expect_invariant(
        "source line number or offset exceeded u32",
        "registration rejects sources longer than u32::MAX bytes",
        "keep source length validation before line-index construction",
    )
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
                line_offsets.push(as_u32(idx + 1));
            }
        }
        if line_offsets.last() != Some(&len) {
            line_offsets.push(len);
        }
        let non_ascii = NonAsciiLines::new(source, &line_offsets);
        Self {
            line_offsets: line_offsets.into_boxed_slice(),
            len,
            ends_with_newline: source.ends_with('\n'),
            non_ascii,
        }
    }

    pub fn line_offsets(&self) -> &[u32] {
        &self.line_offsets
    }

    pub fn len(&self) -> usize {
        self.len as usize
    }

    pub fn is_ascii(&self) -> bool {
        self.non_ascii.lines.is_empty()
    }

    /// Bytes of retained non-ASCII line text.
    pub fn retained_text_bytes(&self) -> usize {
        self.non_ascii.text.len()
    }

    fn estimated_heap_bytes(&self) -> usize {
        self.line_offsets.len() * size_of::<u32>()
            + self.non_ascii.lines.len() * size_of::<(u32, u32)>()
            + self.non_ascii.text.len()
    }
}

impl SourceFile {
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
        let line = as_u32(line_index);
        let character = match self.line_index.non_ascii.line(line) {
            None => target.checked_sub(line_start)?,
            Some(text) => text
                .get(..(target - line_start) as usize)?
                .chars()
                .map(char::len_utf16)
                .sum::<usize>()
                .try_into()
                .ok()?,
        };
        Some((line, character))
    }

    /// The text of a non-ASCII line without its line terminator, as
    /// `str::lines` yields it. ASCII lines keep no text.
    pub fn line_text(&self, line: u32) -> Option<&str> {
        let text = self.line_index.non_ascii.line(line)?;
        Some(match text.strip_suffix('\n') {
            Some(text) => text.strip_suffix('\r').unwrap_or(text),
            None => text,
        })
    }

    /// The text from `start` to `end` when both lie on one non-ASCII line,
    /// on character boundaries. ASCII lines keep no text.
    pub fn non_ascii_text(&self, start: u32, end: u32) -> Option<&str> {
        let (line, _) = self.byte_offset_to_line_character(start)?;
        let line_start = self.line_index.line_offsets[line as usize];
        let text = self.line_index.non_ascii.line(line)?;
        text.get((start - line_start) as usize..end.checked_sub(line_start)? as usize)
    }

    /// The LSP position at the end of the file: its newline count and the
    /// UTF-16 length of its last line.
    pub fn end_position(&self) -> (u32, u32) {
        let index = &self.line_index;
        let has_length_entry = index.len > 0 && !index.ends_with_newline;
        let last = as_u32(index.line_offsets.len() - 1 - usize::from(has_length_entry));
        let width = match index.non_ascii.line(last) {
            Some(text) => as_u32(text.encode_utf16().count()),
            None => index.len - index.line_offsets[last as usize],
        };
        (last, width)
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
    ids_by_path: HashMap<Arc<Path>, SourceFileId>,
    next_id: u32,
    files: HashMap<SourceFileId, Arc<SourceFile>>,
    next_revision: u64,
    export_fingerprints: HashMap<SourceFileId, SemanticExportFingerprint>,
}

impl Files {
    pub(in crate::engine) fn id(&self, path: impl AsRef<Path>) -> Option<SourceFileId> {
        self.ids_by_path
            .get(normalize_path(path.as_ref()).as_path())
            .copied()
    }

    pub(in crate::engine) fn get(&self, id: SourceFileId) -> Option<&SourceFile> {
        self.files.get(&id).map(|file| &**file)
    }

    pub(in crate::engine) fn ids(&self) -> impl Iterator<Item = SourceFileId> + '_ {
        self.files.keys().copied()
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &SourceFile> {
        self.files.values().map(|file| &**file)
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

    /// The id and shared normalized path of `path`, allocating an id when
    /// the path is new.
    fn path_entry(&mut self, path: &Path) -> (SourceFileId, Arc<Path>) {
        let path = normalize_path(path);
        if let Some((shared, id)) = self.ids_by_path.get_key_value(path.as_path()) {
            return (*id, Arc::clone(shared));
        }

        let id = SourceFileId(self.next_id);
        self.next_id = self.next_id.checked_add(1).expect_invariant(
            "source file id allocator overflowed u32",
            "SourceFileId currently stores u32 ids",
            "widen SourceFileId before indexing more than u32::MAX files",
        );
        let shared = Arc::<Path>::from(path);
        self.ids_by_path.insert(Arc::clone(&shared), id);
        (id, shared)
    }

    fn register_owned(
        &mut self,
        file: SourceFileInput,
        library_package: Option<LibraryPackageId>,
    ) -> SourceFileId {
        self.register_borrowed(file.path, &file.content, file.kind, library_package)
    }

    /// Register source whose caller retains the owned buffer. ASCII files need
    /// only their line index and content hash after collection; non-ASCII files
    /// retain the text of their non-ASCII lines for exact UTF-16 conversion.
    fn register_borrowed(
        &mut self,
        path: PathBuf,
        content: &str,
        kind: SourceKind,
        library_package: Option<LibraryPackageId>,
    ) -> SourceFileId {
        let line_index = SourceLineIndex::new(content);
        let content_hash = source_hash(content);
        let (id, shared_path) = self.path_entry(&path);
        if self.files.get(&id).is_some_and(|existing| {
            *existing.path == *path
                && existing.kind == kind
                && existing.line_index == line_index
                && existing.content_hash == content_hash
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
            Arc::new(SourceFile {
                id,
                path: shared_path,
                line_index,
                content_hash,
                kind,
                revision: self.next_revision,
                library_package,
            }),
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
        let file_id = self.register_borrowed(path, content, kind, None);
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

    /// Forget one registered file: its source, path mapping, and export
    /// fingerprint. The id is never reissued, so snapshots of the removed
    /// file stay stale. Returns false for an unknown id.
    pub(super) fn remove(&mut self, file_id: SourceFileId) -> bool {
        let Some(file) = self.files.remove(&file_id) else {
            return false;
        };
        let removed_path = self.ids_by_path.remove(&*file.path);
        invariant_eq!(
            removed_path,
            Some(file_id),
            what = "removed source file was not mapped from its own path",
            why = "registration maps each file's normalized path to its id",
            fix = "keep ids_by_path and files updated together",
        );
        self.export_fingerprints.remove(&file_id);
        true
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
        self.ids_by_path.capacity() * (size_of::<Arc<Path>>() + size_of::<SourceFileId>() + 1)
            + self.files.capacity() * (size_of::<SourceFileId>() + size_of::<Arc<SourceFile>>() + 1)
            + self.files.len() * size_of::<SourceFile>()
            + self
                .files
                .values()
                .map(|file| {
                    2 * size_of::<usize>()
                        + file.path.as_os_str().len()
                        + file.line_index.estimated_heap_bytes()
                })
                .sum::<usize>()
            + self.export_fingerprints.capacity()
                * (size_of::<SourceFileId>() + size_of::<SemanticExportFingerprint>() + 1)
    }

    pub(super) fn shrink_to_fit(&mut self) {
        self.ids_by_path.shrink_to_fit();
        self.export_fingerprints.shrink_to_fit();
        self.files.shrink_to_fit();
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

    /// Register a locked gem source with explicit package identity for
    /// library-tree grouping. The caller keeps the source buffer; see
    /// [`Self::register_file_borrowed`] for what the engine retains.
    pub fn register_gem_file(
        &mut self,
        path: PathBuf,
        content: &str,
        package: LibraryPackageId,
    ) -> SourceFileId {
        self.files
            .register_borrowed(path, content, SourceKind::Gem, Some(package))
    }

    /// Register source whose caller retains the buffer. The engine keeps the
    /// line index and content hash, plus the text of non-ASCII lines for exact
    /// UTF-16 conversion, never the whole source.
    pub fn register_file_borrowed(
        &mut self,
        path: PathBuf,
        content: &str,
        kind: SourceKind,
    ) -> SourceFileId {
        self.files.register_borrowed(path, content, kind, None)
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
}

impl<'a> View<'a> {
    pub fn source_snapshot_for_path(&self, path: impl AsRef<Path>) -> Option<SourceFileSnapshot> {
        self.engine
            .files
            .snapshot_for_path(path, self.engine.instance_id)
    }

    pub fn file_id(&self, path: impl AsRef<Path>) -> Option<SourceFileId> {
        self.engine.files.id(path)
    }

    pub fn file(&self, id: SourceFileId) -> Option<&'a SourceFile> {
        self.engine.files.get(id)
    }

    pub fn file_content_matches(&self, id: SourceFileId, content: &str) -> bool {
        self.engine.files.content_matches(id, content)
    }

    pub fn files(&self) -> impl Iterator<Item = &'a SourceFile> + 'a {
        self.engine.files.iter()
    }

    pub fn file_count(&self) -> usize {
        self.engine.files.len()
    }

    pub fn text_range(&self, file_id: SourceFileId, start_byte: u32, end_byte: u32) -> TextRange {
        self.engine
            .files
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

        let first = files.path_entry(Path::new("app/user.rb")).0;
        let second = files.path_entry(Path::new("app/user.rb")).0;

        assert_eq!(first, second);
        assert_eq!(files.id("app/user.rb"), Some(first));
    }

    #[test]
    fn different_paths_get_different_ids() {
        let mut files = Files::default();

        let first = files.path_entry(Path::new("app/user.rb")).0;
        let second = files.path_entry(Path::new("app/team.rb")).0;

        assert_ne!(first, second);
        assert_eq!(files.id("app/user.rb"), Some(first));
        assert_eq!(files.id("app/team.rb"), Some(second));
    }

    fn register(files: &mut Files, path: &str, content: &str) -> SourceFileId {
        files.register_borrowed(PathBuf::from(path), content, SourceKind::Project, None)
    }

    #[test]
    fn a_registered_path_is_stored_once() {
        let mut files = Files::default();
        let id = register(&mut files, "app/./user.rb", "A = 1\n");

        let file = files.get(id).expect("registered file");
        let (key, _) = files
            .ids_by_path
            .get_key_value(Path::new("app/user.rb"))
            .expect("normalized path is mapped");
        assert!(Arc::ptr_eq(key, &file.path));
        assert_eq!(files.id("app/user.rb"), Some(id));
    }

    #[test]
    fn non_ascii_files_keep_only_their_non_ascii_lines() {
        let mut files = Files::default();
        let content = "a = 1\nb = \"é😀\" # 1\r\nc = 3\nd = \"ü\"";
        let id = register(&mut files, "app/text.rb", content);
        let file = files.get(id).expect("registered file");

        assert!(!file.line_index.is_ascii());
        assert_eq!(
            file.line_index.retained_text_bytes(),
            "b = \"é😀\" # 1\r\nd = \"ü\"".len()
        );
        assert_eq!(file.line_text(0), None);
        assert_eq!(file.line_text(1), Some("b = \"é😀\" # 1"));
        assert_eq!(file.line_text(3), Some("d = \"ü\""));
        for (offset, _) in content.char_indices() {
            let before = &content[..offset];
            let line = before.matches('\n').count();
            let line_start = before.rfind('\n').map_or(0, |newline| newline + 1);
            let character = content[line_start..offset].encode_utf16().count();
            assert_eq!(
                file.byte_offset_to_line_character(offset as u32),
                Some((line as u32, character as u32)),
                "offset {offset}"
            );
        }
        let emoji = content.find('😀').expect("emoji") as u32;
        assert_eq!(file.byte_offset_to_line_character(emoji + 1), None);
        assert_eq!(file.non_ascii_text(emoji, emoji + 4), Some("😀"));
        assert_eq!(file.non_ascii_text(emoji, emoji + 1), None);
        assert_eq!(file.non_ascii_text(0, 1), None);
        assert_eq!(
            file.end_position(),
            (3, "d = \"ü\"".encode_utf16().count() as u32)
        );
    }

    #[test]
    fn end_position_counts_newlines_and_the_last_line() {
        let mut files = Files::default();
        for (content, expected) in [
            ("", (0, 0)),
            ("a", (0, 1)),
            ("\n", (1, 0)),
            ("a\nbc", (1, 2)),
            ("a\nbc\n", (2, 0)),
            ("é\nñé", (1, 2)),
        ] {
            let id = register(&mut files, "app/end.rb", content);
            let file = files.get(id).expect("registered file");
            assert_eq!(file.end_position(), expected, "{content:?}");
        }
    }
}
