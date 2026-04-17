//! Typestate pattern for RubyIndex access — compile-time deadlock prevention.
//!
//! # The Problem
//! When a function holds `index.lock()` and calls another function that also
//! tries to lock, we get a deadlock. This is a runtime error that's hard to debug.
//!
//! # The Solution
//! Use the typestate pattern to make double-locking a **compile-time error**.
//!
//! - `Index<Unlocked>` — can call `.lock()` (write) or `.read()` (shared).
//! - `Index<Locked>` — already has access, `.lock()` doesn't exist.
//!
//! Functions declare which state they need:
//! - `fn needs_to_lock(index: Index<Unlocked>)` — will lock internally.
//! - `fn already_locked(index: Index<Locked>)` — expects pre-locked access.
//!
//! # Lock kind
//! The underlying primitive is `parking_lot::RwLock` so that the many
//! Phase-2 reference-indexing sites can acquire shared read locks in
//! parallel. `.lock()` is kept as the write-lock entry point for
//! backward compatibility with the many existing call sites that mutate
//! the index; prefer `.read()` when only reading.

use parking_lot::{ArcRwLockWriteGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::marker::PhantomData;
use std::sync::Arc;

use super::index::RubyIndex;

// ============================================================================
// State Markers (zero-sized types)
// ============================================================================

/// Marker: Index is not locked, `.lock()` is available.
#[derive(Debug)]
pub struct Unlocked;

/// Marker: Index is locked, direct access is available.
#[derive(Debug)]
pub struct Locked;

// ============================================================================
// Index<State> — The typestate wrapper
// ============================================================================

/// A handle to RubyIndex with compile-time lock state tracking.
///
/// - `Index<Unlocked>`: Can call `.lock()` (write) / `.read()` (shared).
/// - `Index<Locked>`: Already locked, use `.read()` or `.write()` directly.
#[derive(Debug)]
pub struct Index<State> {
    inner: Arc<RwLock<RubyIndex>>,
    _state: PhantomData<State>,
}

// Allow cloning for Unlocked state (safe to share the Arc)
impl Clone for Index<Unlocked> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            _state: PhantomData,
        }
    }
}

// ============================================================================
// Index<Unlocked> — Can lock
// ============================================================================

impl Index<Unlocked> {
    /// Create a new unlocked index handle.
    pub fn new(index: Arc<RwLock<RubyIndex>>) -> Self {
        Self {
            inner: index,
            _state: PhantomData,
        }
    }

    /// Write-lock the index and return a borrowed locked handle.
    ///
    /// **Hot path** — used by indexing visitors, file processors, and any
    /// code that needs to mutate the index. Prefer [`read`] for read-only
    /// access so that multiple workers can read in parallel during Phase 2.
    #[inline]
    pub fn lock(&self) -> LockedIndex<'_> {
        LockedIndex {
            guard: self.inner.write(),
        }
    }

    /// Shared read-lock the index. Multiple readers can hold shared locks
    /// simultaneously; writers block until all readers release.
    ///
    /// Use this in code paths that only need to query the index
    /// (`contains_fqn`, `methods_on_owner`, `get_ancestor_chain`, …).
    /// A read guard cannot mutate.
    #[inline]
    pub fn read(&self) -> ReadLockedIndex<'_> {
        ReadLockedIndex {
            guard: self.inner.read(),
        }
    }

    /// Write-lock the index and return an owned locked handle.
    ///
    /// Slower than [`lock`] (clones the inner `Arc` per call), but the
    /// returned guard is `'static`, so it can outlive the `Index<Unlocked>`
    /// that produced it.
    #[inline]
    pub fn lock_arc(&self) -> ArcLockedIndex {
        ArcLockedIndex {
            guard: RwLock::write_arc(&self.inner),
        }
    }

    /// Get the inner Arc for compatibility with existing code.
    ///
    /// **Prefer using `.lock()` / `.read()` directly.** Exists for
    /// gradual migration.
    pub fn as_arc(&self) -> &Arc<RwLock<RubyIndex>> {
        &self.inner
    }
}

impl From<Arc<RwLock<RubyIndex>>> for Index<Unlocked> {
    fn from(index: Arc<RwLock<RubyIndex>>) -> Self {
        Self::new(index)
    }
}

// ============================================================================
// LockedIndex — RAII write guard
// ============================================================================

/// A write-locked index handle. Provides mutable access to `&mut RubyIndex`.
///
/// - No `.lock()` method exists — prevents double-locking at compile time!
/// - Lock is automatically released when dropped.
pub struct LockedIndex<'a> {
    guard: RwLockWriteGuard<'a, RubyIndex>,
}

impl<'a> LockedIndex<'a> {
    /// Read from the index.
    #[inline]
    pub fn read<R, F: FnOnce(&RubyIndex) -> R>(&self, f: F) -> R {
        f(&self.guard)
    }

    /// Write to the index.
    #[inline]
    pub fn write<R, F: FnOnce(&mut RubyIndex) -> R>(&mut self, f: F) -> R {
        f(&mut self.guard)
    }

    /// Get a reference to the underlying RubyIndex.
    #[inline]
    pub fn as_ref(&self) -> &RubyIndex {
        &self.guard
    }

    /// Get a mutable reference to the underlying RubyIndex.
    #[inline]
    pub fn as_mut(&mut self) -> &mut RubyIndex {
        &mut self.guard
    }
}

// Allow dereferencing to &RubyIndex for ergonomic access
impl<'a> std::ops::Deref for LockedIndex<'a> {
    type Target = RubyIndex;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl<'a> std::ops::DerefMut for LockedIndex<'a> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}

// ============================================================================
// ReadLockedIndex — RAII read guard
// ============================================================================

/// A read-locked (shared) index handle. Read-only access only. Multiple
/// readers can coexist; writers wait.
pub struct ReadLockedIndex<'a> {
    guard: RwLockReadGuard<'a, RubyIndex>,
}

impl<'a> ReadLockedIndex<'a> {
    #[inline]
    pub fn as_ref(&self) -> &RubyIndex {
        &self.guard
    }
}

impl<'a> std::ops::Deref for ReadLockedIndex<'a> {
    type Target = RubyIndex;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

/// Owned-guard variant of [`LockedIndex`]. Returned by
/// [`Index::<Unlocked>::lock_arc`] for callers that need a `'static` guard.
pub struct ArcLockedIndex {
    guard: ArcRwLockWriteGuard<parking_lot::RawRwLock, RubyIndex>,
}

impl std::ops::Deref for ArcLockedIndex {
    type Target = RubyIndex;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.guard
    }
}

impl std::ops::DerefMut for ArcLockedIndex {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.guard
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unlocked_can_lock() {
        let index = Arc::new(RwLock::new(RubyIndex::new()));
        let handle = Index::<Unlocked>::new(index);

        let locked = handle.lock();
        assert_eq!(locked.definitions_len(), 0);
    }

    #[test]
    fn test_read_shared_access() {
        let index = Arc::new(RwLock::new(RubyIndex::new()));
        let handle = Index::<Unlocked>::new(index);

        let r1 = handle.read();
        let r2 = handle.read();
        assert_eq!(r1.definitions_len(), 0);
        assert_eq!(r2.definitions_len(), 0);
    }

    #[test]
    fn test_locked_provides_access() {
        let index = Arc::new(RwLock::new(RubyIndex::new()));
        let handle = Index::<Unlocked>::new(index);

        let locked = handle.lock();
        let count = locked.read(|idx| idx.definitions_len());
        assert_eq!(count, 0);
    }

    #[test]
    fn test_deref_works() {
        let index = Arc::new(RwLock::new(RubyIndex::new()));
        let handle = Index::<Unlocked>::new(index);

        let locked = handle.lock();
        assert_eq!(locked.definitions_len(), 0);
    }

    #[test]
    fn test_pass_locked_to_function() {
        fn use_index(index: &LockedIndex<'_>) -> usize {
            index.definitions_len()
        }

        let index = Arc::new(RwLock::new(RubyIndex::new()));
        let handle = Index::<Unlocked>::new(index);

        let locked = handle.lock();
        let count = use_index(&locked);
        assert_eq!(count, 0);
    }
}
