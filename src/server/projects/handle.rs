//! `ProjectHandle`: one Ruby project's semantic state, shared by readers and
//! written through one owner operation at a time, beside the project's runtime
//! state and published requires, which keep their own locks.
//!
//! Readers take [`ProjectHandle::view`], which holds the engine read guard for
//! exactly one synchronous closure, so a request's answer reflects one
//! semantic revision and no guard crosses an `.await`. Writers take
//! [`ProjectHandle::update`]. Clones share the same project; two handles are
//! the same project exactly when [`ProjectHandle::is_same`] holds.
//!
//! Lifecycle code writes through the named operations below (register,
//! remove, clear, resolve, reset, and the conditional require-diagnostic
//! refresh); `update` remains for fixtures and tools. The loader writes through
//! the named operations of the [`LoadTarget`] that [`ProjectHandle::load_target`]
//! returns, and never receives the engine lock. The named operations are the
//! vocabulary a single project writer would accept as commands.
use super::ProjectRuntimeState;
use crate::invariant::ExpectInvariant;
use crate::loader::context::{LoadTarget, NamedWrite, PublishedRequires};
use crate::loader::require_paths::RequireFeatureIndex;
use parking_lot::RwLock;
use ruby_analysis::core::{DiagnosticFact, FileAnalysis, SourceFileId, SourceKind};
use ruby_analysis::engine::{
    Project, ResolveMode, SourceFile, SourceFileInput, SourceFileSnapshot, View,
};
use ruby_analysis::inference::semantics::Semantics;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone)]
pub struct ProjectHandle {
    engine: Arc<RwLock<Project>>,
    /// Runtime selection, detected Ruby version, and JRuby add-on, each under
    /// its own lock and never under the engine guard.
    runtime: ProjectRuntimeState,
    /// Require roots and feature index published after dependency indexing,
    /// replaced independently of engine writes.
    requires: PublishedRequires,
}

impl ProjectHandle {
    pub(crate) fn new(engine: Project) -> Self {
        Self {
            engine: Arc::new(RwLock::new(engine)),
            runtime: ProjectRuntimeState::default(),
            requires: PublishedRequires::default(),
        }
    }

    /// The project's runtime selection, Ruby version, and JRuby add-on.
    pub fn runtime(&self) -> &ProjectRuntimeState {
        &self.runtime
    }

    /// Absolute gem/stdlib require roots retained after dependency indexing.
    pub fn dependency_require_paths(&self) -> Vec<PathBuf> {
        self.requires.paths()
    }

    /// Project-relative folders the project's gemspecs declare as
    /// `require_paths`, searched after configured `loadPaths`.
    pub fn declared_require_paths(&self) -> Vec<String> {
        self.requires.declared_paths()
    }

    /// Publish the gemspec-declared require folders an indexing run read.
    pub(crate) fn set_declared_require_paths(&self, declared: Vec<String>) {
        self.requires.replace_declared_paths(declared);
    }

    /// The require feature index published for the project.
    pub fn require_feature_index(&self) -> Arc<RequireFeatureIndex> {
        self.requires.feature_index()
    }

    /// Hold this identity guard through delayed require-fact commit and
    /// publication.
    pub(in crate::server) fn require_feature_guard(
        &self,
    ) -> parking_lot::RwLockReadGuard<'_, Arc<RequireFeatureIndex>> {
        self.requires.feature_index_guard()
    }

    /// The published requires a load of this project reads live.
    pub(crate) fn published_requires(&self) -> PublishedRequires {
        self.requires.clone()
    }

    /// Publish the require roots and feature index dependency indexing built.
    pub(crate) fn set_dependency_require_resolution(
        &self,
        paths: Vec<PathBuf>,
        index: Arc<RequireFeatureIndex>,
    ) {
        self.requires.replace(paths, index);
    }

    #[cfg(test)]
    pub(crate) fn set_dependency_require_paths(&self, paths: Vec<PathBuf>) {
        let index = Arc::new(RequireFeatureIndex::build(&paths, None));
        self.set_dependency_require_resolution(paths, index);
    }

    /// Run `read` over one view of the project's current semantic state. The
    /// read guard is held only for the closure, which is synchronous.
    pub fn view<R>(&self, read: impl FnOnce(&View<'_>) -> R) -> R {
        let engine = self.engine.read();
        read(&engine.view())
    }

    /// Apply one owner write to the project's engine. Writes are serialized;
    /// readers observe either the state before or after the whole closure.
    pub fn update<R>(&self, write: impl FnOnce(&mut Project) -> R) -> R {
        write(&mut self.engine.write())
    }

    /// Register `content` at `path`, replacing the source of a file already
    /// registered there, and return the file's identity.
    pub fn register_source(&self, path: PathBuf, content: &str, kind: SourceKind) -> SourceFileId {
        self.update(|engine| engine.register_file_borrowed(path, content, kind))
    }

    /// Resolve the whole project after deferred registrations.
    pub fn resolve(&self) {
        self.update(Project::resolve);
    }

    /// Replace the project's semantic state with an empty engine before a
    /// full rebuild.
    pub fn reset(&self) {
        self.update(|engine| *engine = Project::new());
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
                .map(|file| file.path.to_path_buf())
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

    /// Replace the unresolved-require diagnostics of the file at `path` while
    /// its current source is `snapshot`, then run `publish` over the updated
    /// view under the same write guard, so no edit lands between the commit
    /// and the projection it publishes. `requires` runs under the guard too:
    /// it decides whether the file still accepts the refresh and returns the
    /// replacement diagnostics, or `None` to skip the commit. Returns whether
    /// the diagnostics were replaced.
    pub fn refresh_require_diagnostics_if_snapshot(
        &self,
        path: &Path,
        snapshot: SourceFileSnapshot,
        requires: impl FnOnce(&View<'_>, &SourceFile) -> Option<Vec<DiagnosticFact>>,
        publish: impl FnOnce(&View<'_>),
    ) -> bool {
        self.update(|engine| {
            let view = engine.view();
            if view.source_snapshot_for_path(path) != Some(snapshot) {
                return false;
            }
            let Some(file) = view.file_id(path).and_then(|id| view.file(id)) else {
                return false;
            };
            let Some(requires) = requires(&view, file) else {
                return false;
            };
            if !engine.replace_unresolved_require_diagnostics_if_source_snapshot(snapshot, requires)
            {
                return false;
            }
            publish(&engine.view());
            true
        })
    }

    /// Whether `self` and `other` are handles to the same project.
    pub fn is_same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.engine, &other.engine)
    }

    /// The target the loader reads and writes while it loads this project.
    pub fn load_target(&self) -> Arc<dyn LoadTarget> {
        Arc::new(self.clone())
    }

    /// Whether `target` addresses this project's engine.
    pub(crate) fn is_target(&self, target: &Arc<dyn LoadTarget>) -> bool {
        std::ptr::addr_eq(
            Arc::as_ptr(&target.clone().engine_identity()),
            Arc::as_ptr(&self.engine),
        )
    }

    /// Read-only semantics over the project's current state, for a file walk
    /// that runs outside one view.
    pub(crate) fn semantics(&self) -> Arc<dyn Semantics> {
        self.engine.clone()
    }

    /// Test-only owned read guard, for assertions that inspect engine state
    /// across many statements. Production readers use [`Self::view`].
    #[cfg(test)]
    pub(crate) fn test_read(&self) -> ArcRwLockReadGuard<RawRwLock, Project> {
        self.engine.read_arc()
    }

    /// Test-only owned write guard, for fixtures that seed engine state
    /// directly. Production writers use [`Self::update`].
    #[cfg(test)]
    pub(crate) fn test_write(&self) -> ArcRwLockWriteGuard<RawRwLock, Project> {
        self.engine.write_arc()
    }
}

impl LoadTarget for ProjectHandle {
    fn read_engine(&self, read: &mut dyn FnMut(&Project)) {
        read(&self.engine.read());
    }

    fn write_engine(&self, _proof: NamedWrite, write: &mut dyn FnMut(&mut Project)) {
        self.update(|engine| write(engine));
    }

    fn semantics(self: Arc<Self>) -> Arc<dyn Semantics> {
        self.engine.clone()
    }

    fn engine_identity(self: Arc<Self>) -> Arc<dyn Send + Sync> {
        self.engine.clone()
    }
}

fn registered_file_of_kind(
    engine: &Project,
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
