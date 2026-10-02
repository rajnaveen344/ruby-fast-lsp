//! `ProjectHandle`: one Ruby project's semantic state, shared by readers and
//! written through one owner operation at a time.
//!
//! Readers take [`ProjectHandle::view`], which holds the engine read guard for
//! exactly one synchronous closure, so a request's answer reflects one
//! semantic revision and no guard crosses an `.await`. Writers take
//! [`ProjectHandle::update`]. Clones share the same project; two handles are
//! the same project exactly when [`ProjectHandle::is_same`] holds.
//!
//! Lifecycle code writes through the named operations below (register,
//! remove, clear, resolve, reset); `update` remains for the conditional
//! require-diagnostic commit, fixtures, and tools. The named operations are
//! the vocabulary a single project writer would accept as commands.
use crate::invariant::ExpectInvariant;
use parking_lot::RwLock;
use ruby_analysis::core::{FileAnalysis, SourceFileId, SourceKind};
use ruby_analysis::engine::{AnalysisEngine, ResolveMode, SourceFileInput, View};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone)]
pub struct ProjectHandle {
    engine: Arc<RwLock<AnalysisEngine>>,
}

impl ProjectHandle {
    pub(crate) fn new(engine: AnalysisEngine) -> Self {
        Self {
            engine: Arc::new(RwLock::new(engine)),
        }
    }

    /// Run `read` over one view of the project's current semantic state. The
    /// read guard is held only for the closure, which is synchronous.
    pub fn view<R>(&self, read: impl FnOnce(&View<'_>) -> R) -> R {
        let engine = self.engine.read();
        read(&engine.view())
    }

    /// Apply one owner write to the project's engine. Writes are serialized;
    /// readers observe either the state before or after the whole closure.
    pub fn update<R>(&self, write: impl FnOnce(&mut AnalysisEngine) -> R) -> R {
        write(&mut self.engine.write())
    }

    /// Register `content` at `path`, replacing the source of a file already
    /// registered there, and return the file's identity.
    pub fn register_source(
        &self,
        path: PathBuf,
        content: String,
        kind: SourceKind,
    ) -> SourceFileId {
        self.update(|engine| {
            engine.register_file(SourceFileInput {
                path,
                content,
                kind,
            })
        })
    }

    /// Resolve the whole project after deferred registrations.
    pub fn resolve(&self) {
        self.update(AnalysisEngine::resolve);
    }

    /// Replace the project's semantic state with an empty engine before a
    /// full rebuild.
    pub fn reset(&self) {
        self.update(|engine| *engine = AnalysisEngine::new());
    }

    /// Remove the file registered at `path`, of any kind. Returns whether a
    /// file was removed.
    pub fn remove_path(&self, path: &Path) -> bool {
        self.update(|engine| {
            engine
                .view()
                .file_id(path)
                .is_some_and(|file_id| engine.remove(file_id, ResolveMode::Immediate))
        })
    }

    /// Remove the file at `path` if it is registered as `kind`: other files
    /// stop resolving into it, and a later registration at the same path starts
    /// from a fresh identity. Returns whether a file was removed.
    pub fn remove_path_of_kind(&self, path: &Path, kind: SourceKind) -> bool {
        self.update(|engine| {
            let Some(file_id) = registered_file_of_kind(engine, path, kind) else {
                return false;
            };
            engine.remove(file_id, ResolveMode::Immediate)
        })
    }

    /// Keep the file at `path`, if it is registered as `kind`, as an empty
    /// source with no facts, so require resolution still finds a file that
    /// exists but could not be read or analyzed. Returns whether a file was
    /// cleared.
    pub fn clear_path_facts_of_kind(&self, path: &Path, kind: SourceKind) -> bool {
        self.update(|engine| {
            let Some(file_id) = registered_file_of_kind(engine, path, kind) else {
                return false;
            };
            let path = engine
                .view()
                .file(file_id)
                .map(|file| file.path.clone())
                .expect_invariant(
                    "a registered file vanished under the engine write lock",
                    "the lookup and the clear hold one engine write borrow",
                    "keep lookup and clear inside one project update",
                );
            let file_id = engine.register_file(SourceFileInput {
                path,
                content: String::new(),
                kind,
            });
            engine.update(file_id, FileAnalysis::default(), ResolveMode::Immediate);
            true
        })
    }

    /// Whether `self` and `other` are handles to the same project.
    pub fn is_same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.engine, &other.engine)
    }

    /// Whether `engine` is this project's engine, as handed to the loader.
    pub(crate) fn owns_engine(&self, engine: &Arc<RwLock<AnalysisEngine>>) -> bool {
        Arc::ptr_eq(&self.engine, engine)
    }

    /// The shared engine the loader writes while it loads this project. The
    /// loader receives it only through `LoadSink::engine_for_uri` and the
    /// coordinator's engine override.
    pub fn shared_engine(&self) -> &Arc<RwLock<AnalysisEngine>> {
        &self.engine
    }

    /// Test-only owned read guard, for assertions that inspect engine state
    /// across many statements. Production readers use [`Self::view`].
    #[cfg(test)]
    pub(crate) fn test_read(&self) -> ArcRwLockReadGuard<RawRwLock, AnalysisEngine> {
        self.engine.read_arc()
    }

    /// Test-only owned write guard, for fixtures that seed engine state
    /// directly. Production writers use [`Self::update`].
    #[cfg(test)]
    pub(crate) fn test_write(&self) -> ArcRwLockWriteGuard<RawRwLock, AnalysisEngine> {
        self.engine.write_arc()
    }
}

fn registered_file_of_kind(
    engine: &AnalysisEngine,
    path: &Path,
    kind: SourceKind,
) -> Option<SourceFileId> {
    let view = engine.view();
    let file_id = view.file_id(path)?;
    view.file(file_id)
        .is_some_and(|file| file.kind == kind)
        .then_some(file_id)
}

#[cfg(test)]
use parking_lot::{ArcRwLockReadGuard, ArcRwLockWriteGuard, RawRwLock};
