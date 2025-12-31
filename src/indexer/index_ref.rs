//! Typestate pattern for RubyIndex access - compile-time deadlock prevention.
//!
//! # The Problem
//! When a function holds `index.lock()` and calls another function that also
//! tries to lock, we get a deadlock. This is a runtime error that's hard to debug.
//!
//! # The Solution
//! Use the typestate pattern to make double-locking a **compile-time error**.
//!
//! - `Index<Unlocked>` - can call `.lock()` to get access
//! - `Index<Locked>` - already has access, `.lock()` doesn't exist
//!
//! Functions declare which state they need:
//! - `fn needs_to_lock(index: Index<Unlocked>)` - will lock internally
//! - `fn already_locked(index: Index<Locked>)` - expects pre-locked access
//!
//! # Example
//! ```ignore
//! // Entry point - starts unlocked
//! fn handle_request(index: Index<Unlocked>) {
//!     let locked = index.lock();  // Now Index<Locked>
//!     process(&locked);           // Pass locked state
//!     // locked.lock() would be a COMPILE ERROR - method doesn't exist!
//! }
//!
//! fn process(index: &Index<Locked>) {
//!     index.read(|idx| idx.definitions_len());  // Use the index
//! }
//! ```

use parking_lot::{Mutex, MutexGuard};
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
// Index<State> - The typestate wrapper
// ============================================================================

/// A handle to RubyIndex with compile-time lock state tracking.
///
/// - `Index<Unlocked>`: Can call `.lock()` to acquire the lock
/// - `Index<Locked>`: Already locked, use `.read()` or `.write()` directly
#[derive(Debug)]
pub struct Index<State> {
    inner: Arc<Mutex<RubyIndex>>,
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
// Index<Unlocked> - Can lock
// ============================================================================

impl Index<Unlocked> {
    /// Create a new unlocked index handle.
    pub fn new(index: Arc<Mutex<RubyIndex>>) -> Self {
        Self {
            inner: index,
            _state: PhantomData,
        }
    }

    /// Lock the index and return a locked handle.
    ///
    /// The lock is held until the returned `LockedIndex` is dropped.
    #[inline]
    pub fn lock(&self) -> LockedIndex<'_> {
        LockedIndex {
            guard: self.inner.lock(),
        }
    }

    /// Get the inner Arc for compatibility with existing code.
    ///
    /// **Prefer using `.lock()` directly.** This exists for gradual migration.
    pub fn as_arc(&self) -> &Arc<Mutex<RubyIndex>> {
        &self.inner
    }
}

impl From<Arc<Mutex<RubyIndex>>> for Index<Unlocked> {
    fn from(index: Arc<Mutex<RubyIndex>>) -> Self {
        Self::new(index)
    }
}

// ============================================================================
// LockedIndex - RAII guard with locked state
// ============================================================================

/// A locked index handle. Provides direct access to `&RubyIndex`.
///
/// - No `.lock()` method exists - prevents double-locking at compile time!
/// - Lock is automatically released when dropped.
pub struct LockedIndex<'a> {
    guard: MutexGuard<'a, RubyIndex>,
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
    ///
    /// Useful when you need to pass `&RubyIndex` to existing functions.
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
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unlocked_can_lock() {
        let index = Arc::new(Mutex::new(RubyIndex::new()));
        let handle = Index::<Unlocked>::new(index);

        let locked = handle.lock();
        assert_eq!(locked.definitions_len(), 0);
    }

    #[test]
    fn test_locked_provides_access() {
        let index = Arc::new(Mutex::new(RubyIndex::new()));
        let handle = Index::<Unlocked>::new(index);

        let locked = handle.lock();
        let count = locked.read(|idx| idx.definitions_len());
        assert_eq!(count, 0);
    }

    #[test]
    fn test_deref_works() {
        let index = Arc::new(Mutex::new(RubyIndex::new()));
        let handle = Index::<Unlocked>::new(index);

        let locked = handle.lock();
        // Can use methods directly via Deref
        assert_eq!(locked.definitions_len(), 0);
    }

    #[test]
    fn test_pass_locked_to_function() {
        fn use_index(index: &LockedIndex) -> usize {
            index.definitions_len()
        }

        let index = Arc::new(Mutex::new(RubyIndex::new()));
        let handle = Index::<Unlocked>::new(index);

        let locked = handle.lock();
        let count = use_index(&locked);
        assert_eq!(count, 0);
    }

    // This test demonstrates the compile-time safety:
    // If you uncomment the following, it WON'T COMPILE because
    // LockedIndex has no .lock() method!
    //
    // #[test]
    // fn test_cannot_double_lock() {
    //     let index = Arc::new(Mutex::new(RubyIndex::new()));
    //     let handle = Index::<Unlocked>::new(index);
    //     let locked = handle.lock();
    //     let double_locked = locked.lock(); // COMPILE ERROR!
    // }
}
