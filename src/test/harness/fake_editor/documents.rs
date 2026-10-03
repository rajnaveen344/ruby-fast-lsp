//! Buffer lifecycle and editing operations routed through the real handlers.

use tower_lsp::lsp_types::{
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, FileChangeType, FileEvent,
    TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
    VersionedTextDocumentIdentifier, WorkspaceEdit,
};

use super::FakeEditor;
use crate::lsp::lifecycle::indexing;

impl FakeEditor {
    /// Open a file in the editor with the given content.
    ///
    /// Routes through the real `handle_did_open` handler.
    /// Panics if the file is already open (use `set()` to update).
    pub async fn open(&mut self, filename: &str, content: &str) {
        invariant!(
            !self.buffers.contains_key(filename),
            what = "file '{}' is already open",
            why = "open() starts a new buffer",
            fix = "use set() to update content",
            filename,
        );

        let uri = Self::filename_to_uri(filename);
        let version = 1;

        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri,
                language_id: "ruby".to_string(),
                version,
                text: content.to_string(),
            },
        };
        indexing::handle_did_open(&self.server, params).await;

        self.buffers
            .insert(filename.to_string(), (content.to_string(), version));
    }

    /// Update an open file's content.
    ///
    /// Routes through the real `handle_did_change` handler.
    /// Panics if the file is not open (use `open()` first).
    pub async fn set(&mut self, filename: &str, new_content: &str) {
        let (_, version) = self.buffers.get(filename).unwrap_or_else(|| {
            unreachable_invariant!(
                what = "file '{}' is not open",
                why = "set() edits an open buffer",
                fix = "call open() before set()",
                filename,
            )
        });
        let new_version = version + 1;

        let uri = Self::filename_to_uri(filename);

        let params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri,
                version: new_version,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: new_content.to_string(),
            }],
        };
        indexing::handle_did_change(&self.server, params).await;

        self.buffers
            .insert(filename.to_string(), (new_content.to_string(), new_version));
    }

    /// Save a file in the editor.
    ///
    /// Routes through the real `handle_did_save` handler.
    /// Triggers YARD diagnostics and inlay hint refresh.
    /// Panics if the file is not open.
    pub async fn save(&mut self, filename: &str) {
        invariant!(
            self.buffers.contains_key(filename),
            what = "file '{}' is not open",
            why = "save() writes an open buffer",
            fix = "call open() before save()",
            filename,
        );

        let uri = Self::filename_to_uri(filename);

        let params = DidSaveTextDocumentParams {
            text_document: TextDocumentIdentifier { uri },
            text: None,
        };
        indexing::handle_did_save(&self.server, params).await;
    }

    /// Close a file in the editor.
    ///
    /// Routes through the real `handle_did_close` handler.
    /// Index entries are preserved (matching real LSP behavior).
    pub async fn close(&mut self, filename: &str) {
        invariant!(
            self.buffers.remove(filename).is_some(),
            what = "file '{}' is not open",
            why = "close() needs a buffer that was opened",
            fix = "open the file before closing it",
            filename,
        );

        let uri = Self::filename_to_uri(filename);

        let params = DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier { uri },
        };
        indexing::handle_did_close(&self.server, params).await;
    }

    /// Report a file-system change to a closed file.
    ///
    /// Routes through the real `handle_watched_files_changed` handler, which
    /// reads created and changed files from disk. Panics if the file is open.
    pub async fn watched_file_changed(&mut self, filename: &str, typ: FileChangeType) {
        invariant!(
            !self.buffers.contains_key(filename),
            what = "watched change reported for open file '{}'",
            why = "open buffers are authoritative, so the handler ignores their disk events",
            fix = "close the file before reporting a disk change",
            filename,
        );
        let params = DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: Self::filename_to_uri(filename),
                typ,
            }],
        };
        indexing::handle_watched_files_changed(&self.server, params).await;
    }

    // ─── Editing Helpers ───────────────────────────────────────────────

    /// Insert text at a 0-indexed position, triggering a `did_change`.
    ///
    /// Simulates the user typing at a specific cursor position.
    /// The position is in the file's current content (before insertion).
    pub async fn type_at(&mut self, filename: &str, line: u32, character: u32, text: &str) {
        let (content, _) = self.buffers.get(filename).unwrap_or_else(|| {
            unreachable_invariant!(
                what = "file '{}' is not open",
                why = "type_at() edits an open buffer",
                fix = "call open() before type_at()",
                filename,
            )
        });

        let offset = Self::position_to_byte_offset(content, line, character);
        let mut new_content = String::with_capacity(content.len() + text.len());
        new_content.push_str(&content[..offset]);
        new_content.push_str(text);
        new_content.push_str(&content[offset..]);

        self.set(filename, &new_content).await;
    }

    /// Delete `count` characters before a 0-indexed position, triggering a `did_change`.
    ///
    /// Simulates the user pressing backspace at a specific cursor position.
    pub async fn backspace_at(&mut self, filename: &str, line: u32, character: u32, count: usize) {
        let (content, _) = self.buffers.get(filename).unwrap_or_else(|| {
            unreachable_invariant!(
                what = "file '{}' is not open",
                why = "backspace_at() edits an open buffer",
                fix = "call open() before backspace_at()",
                filename,
            )
        });

        let offset = Self::position_to_byte_offset(content, line, character);
        let delete_start = offset.saturating_sub(count);
        let mut new_content = String::with_capacity(content.len() - (offset - delete_start));
        new_content.push_str(&content[..delete_start]);
        new_content.push_str(&content[offset..]);

        self.set(filename, &new_content).await;
    }

    /// Applies a `WorkspaceEdit` to the editor's buffers.
    ///
    /// Updates affected files via `set()`, so changes go through the real
    /// `handle_did_change` handler. Only supports the `changes` field
    /// (not `document_changes`).
    pub async fn apply_edit(&mut self, edit: &WorkspaceEdit) {
        if let Some(changes) = &edit.changes {
            for (uri, text_edits) in changes {
                // Find the filename for this URI
                let filename = self
                    .buffers
                    .keys()
                    .find(|f| Self::filename_to_uri(f) == *uri)
                    .cloned();

                let filename = filename.unwrap_or_else(|| {
                    unreachable_invariant!(
                        what = "WorkspaceEdit references URI '{}' that is not open",
                        why = "edits apply to open buffers",
                        fix = "open the file before applying the edit",
                        uri,
                    )
                });

                let (content, _) = &self.buffers[&filename];
                let mut new_content = content.clone();

                // Apply edits in reverse order to preserve positions
                let mut sorted_edits = text_edits.clone();
                sorted_edits.sort_by(|a, b| {
                    b.range
                        .start
                        .line
                        .cmp(&a.range.start.line)
                        .then(b.range.start.character.cmp(&a.range.start.character))
                });

                for edit in &sorted_edits {
                    let start = Self::position_to_byte_offset(
                        &new_content,
                        edit.range.start.line,
                        edit.range.start.character,
                    );
                    let end = Self::position_to_byte_offset(
                        &new_content,
                        edit.range.end.line,
                        edit.range.end.character,
                    );
                    new_content = format!(
                        "{}{}{}",
                        &new_content[..start],
                        edit.new_text,
                        &new_content[end..]
                    );
                }

                self.set(&filename, &new_content).await;
            }
        }
    }

    /// Get the current content of an open file.
    pub fn content(&self, filename: &str) -> &str {
        let (content, _) = self.buffers.get(filename).unwrap_or_else(|| {
            unreachable_invariant!(
                what = "file '{}' is not open",
                why = "content() reads an open buffer",
                fix = "call open() before content()",
                filename,
            )
        });
        content
    }
}
