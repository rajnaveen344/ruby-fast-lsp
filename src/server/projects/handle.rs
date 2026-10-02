//! `ProjectHandle`: one Ruby project's semantic state, shared by readers and
//! written through one owner operation at a time.
//!
//! Readers take [`ProjectHandle::view`], which holds the engine read guard for
//! exactly one synchronous closure, so a request's answer reflects one
//! semantic revision and no guard crosses an `.await`. Writers take
//! [`ProjectHandle::update`]. Clones share the same project; two handles are
//! the same project exactly when [`ProjectHandle::is_same`] holds.
use parking_lot::RwLock;
use ruby_analysis::engine::{AnalysisEngine, View};
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

#[cfg(test)]
use parking_lot::{ArcRwLockReadGuard, ArcRwLockWriteGuard, RawRwLock};
