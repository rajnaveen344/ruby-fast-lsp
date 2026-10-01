//! Production invariant checks with one message shape:
//! `invariant violated: <what> — bug: <why> — fix: <fix>`.
//!
//! Every crate that checks internal invariants compiles this file as a private
//! `#[macro_use] mod invariant` declared before its other modules, so the macros
//! are in scope crate-wide and `crate::invariant::ExpectInvariant` resolves the
//! same way everywhere. The checks always run (never `debug_assert!`) and format
//! their message only when they fail.
//!
//! - `invariant!(cond, what = "...", why = "...", fix = "...", args...)`
//! - `invariant_eq!(left, right, what = ..., why = ..., fix = ..., args...)`
//!   and `invariant_ne!`, which also print both operands.
//! - `unreachable_invariant!(what = ..., why = ..., fix = ..., args...)` for
//!   impossible states; it evaluates to `!`.
//! - `option.expect_invariant(what, why, fix)` and the same on `Result`, which
//!   appends the error.
//!
//! The three message parts are string literals joined into one format string,
//! so explicit format arguments may refer to placeholders in any part. Pass
//! captured variables explicitly (`name = name`): a format string assembled by
//! `concat!` cannot capture identifiers implicitly.

use std::fmt;

/// Panics with the shared invariant message unless `cond` holds.
macro_rules! invariant {
    ($cond:expr, what = $what:literal, why = $why:literal, fix = $fix:literal $(,)?) => {
        assert!($cond, concat!("invariant violated: ", $what, " — bug: ", $why, " — fix: ", $fix))
    };
    ($cond:expr, what = $what:literal, why = $why:literal, fix = $fix:literal, $($arg:tt)+) => {
        assert!(
            $cond,
            concat!("invariant violated: ", $what, " — bug: ", $why, " — fix: ", $fix),
            $($arg)+
        )
    };
}

/// Panics with the shared invariant message and both operands unless they are equal.
macro_rules! invariant_eq {
    ($left:expr, $right:expr, what = $what:literal, why = $why:literal, fix = $fix:literal $(,)?) => {
        assert_eq!(
            $left,
            $right,
            concat!("invariant violated: ", $what, " — bug: ", $why, " — fix: ", $fix)
        )
    };
    ($left:expr, $right:expr, what = $what:literal, why = $why:literal, fix = $fix:literal, $($arg:tt)+) => {
        assert_eq!(
            $left,
            $right,
            concat!("invariant violated: ", $what, " — bug: ", $why, " — fix: ", $fix),
            $($arg)+
        )
    };
}

/// Panics with the shared invariant message and both operands if they are equal.
macro_rules! invariant_ne {
    ($left:expr, $right:expr, what = $what:literal, why = $why:literal, fix = $fix:literal $(,)?) => {
        assert_ne!(
            $left,
            $right,
            concat!("invariant violated: ", $what, " — bug: ", $why, " — fix: ", $fix)
        )
    };
    ($left:expr, $right:expr, what = $what:literal, why = $why:literal, fix = $fix:literal, $($arg:tt)+) => {
        assert_ne!(
            $left,
            $right,
            concat!("invariant violated: ", $what, " — bug: ", $why, " — fix: ", $fix),
            $($arg)+
        )
    };
}

/// Panics with the shared invariant message; use where a state is impossible.
macro_rules! unreachable_invariant {
    (what = $what:literal, why = $why:literal, fix = $fix:literal $(,)?) => {
        panic!(concat!("invariant violated: ", $what, " — bug: ", $why, " — fix: ", $fix))
    };
    (what = $what:literal, why = $why:literal, fix = $fix:literal, $($arg:tt)+) => {
        panic!(
            concat!("invariant violated: ", $what, " — bug: ", $why, " — fix: ", $fix),
            $($arg)+
        )
    };
}

/// `expect` for values whose absence or error is an internal bug.
pub trait ExpectInvariant<T> {
    /// Returns the value or panics with the shared invariant message. A `Result`
    /// error is appended to `what` with its `Debug` form.
    #[track_caller]
    fn expect_invariant(self, what: &str, why: &str, fix: &str) -> T;
}

impl<T> ExpectInvariant<T> for Option<T> {
    #[inline]
    #[track_caller]
    fn expect_invariant(self, what: &str, why: &str, fix: &str) -> T {
        match self {
            Some(value) => value,
            None => violated(what, None, why, fix),
        }
    }
}

impl<T, E: fmt::Debug> ExpectInvariant<T> for Result<T, E> {
    #[inline]
    #[track_caller]
    fn expect_invariant(self, what: &str, why: &str, fix: &str) -> T {
        match self {
            Ok(value) => value,
            Err(error) => violated(what, Some(&error), why, fix),
        }
    }
}

#[cold]
#[inline(never)]
#[track_caller]
fn violated(what: &str, error: Option<&dyn fmt::Debug>, why: &str, fix: &str) -> ! {
    match error {
        Some(error) => {
            panic!("invariant violated: {what}: {error:?} — bug: {why} — fix: {fix}")
        }
        None => panic!("invariant violated: {what} — bug: {why} — fix: {fix}"),
    }
}
