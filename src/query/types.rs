//! Types Query - Type inference and lookup
//!
//! Re-exports existing TypeQuery from inferrer/query.rs.
//! This module acts as a bridge during migration.

// Re-export the existing TypeQuery until migration is complete
pub use crate::inferrer::query::{TypeHint, TypeHintKind, TypeQuery};
