//! Entry Builder
//!
//! Provides a builder pattern for constructing `Entry` objects with validation.

use tower_lsp::lsp_types::Location;

use super::{Entry, EntryKind};
use crate::types::fully_qualified_name::FullyQualifiedName;

// ============================================================================
// EntryBuilder
// ============================================================================

/// Builder for creating `Entry` objects with required field validation
pub struct EntryBuilder {
    fqn: Option<FullyQualifiedName>,
    location: Option<Location>,
    kind: Option<EntryKind>,
}

impl EntryBuilder {
    /// Create a new builder with empty fields
    pub fn new() -> Self {
        EntryBuilder {
            fqn: None,
            location: None,
            kind: None,
        }
    }

    /// Set the fully qualified name (required)
    pub fn fqn(mut self, fqn: FullyQualifiedName) -> Self {
        self.fqn = Some(fqn);
        self
    }

    /// Set the source location (required)
    pub fn location(mut self, location: Location) -> Self {
        self.location = Some(location);
        self
    }

    /// Set the entry kind (required)
    pub fn kind(mut self, kind: EntryKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Build the Entry with validation
    pub fn build(self) -> Result<Entry, &'static str> {
        let fqn = self.fqn.ok_or("Fully qualified name (fqn) is required")?;
        let location = self.location.ok_or("Location is required")?;
        let kind = self.kind.ok_or("Entry kind is required")?;

        Ok(Entry {
            fqn,
            location,
            kind,
        })
    }
}

impl Default for EntryBuilder {
    fn default() -> Self {
        Self::new()
    }
}
