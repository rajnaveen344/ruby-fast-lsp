//! Entry Builder
//!
//! Provides a builder pattern for constructing `Entry` objects with validation.
//! The builder now stores an intermediate LocationData that can be converted
//! to CompactLocation when added to the index.

use tower_lsp::lsp_types::Location;

use super::{Entry, EntryKind};
use crate::types::{compact_location::CompactLocation, fully_qualified_name::FullyQualifiedName};

// ============================================================================
// LocationData - Intermediate storage before conversion to CompactLocation
// ============================================================================

/// Intermediate location data before conversion to CompactLocation
/// This allows EntryBuilder to accept Location without needing file_id upfront.
#[derive(Debug, Clone)]
pub enum LocationData {
    /// Standard LSP Location (will be converted to CompactLocation in add_entry)
    Lsp(Location),
    /// Already compact (used when file_id is known)
    Compact(CompactLocation),
}

// ============================================================================
// EntryBuilder
// ============================================================================

/// Builder for creating `Entry` objects with required field validation
///
/// Accepts LSP Location for API compatibility. Conversion to CompactLocation
/// happens when the entry is added to the index.
pub struct EntryBuilder {
    fqn: Option<FullyQualifiedName>,
    location_data: Option<LocationData>,
    kind: Option<EntryKind>,
}

impl EntryBuilder {
    /// Create a new builder with empty fields
    pub fn new() -> Self {
        EntryBuilder {
            fqn: None,
            location_data: None,
            kind: None,
        }
    }

    /// Set the fully qualified name (required)
    pub fn fqn(mut self, fqn: FullyQualifiedName) -> Self {
        self.fqn = Some(fqn);
        self
    }

    /// Set the source location from LSP Location (required)
    ///
    /// The Location will be converted to CompactLocation when the entry
    /// is added to the index via RubyIndex::add_entry().
    pub fn location(mut self, location: Location) -> Self {
        self.location_data = Some(LocationData::Lsp(location));
        self
    }

    /// Set the source location directly from CompactLocation
    ///
    /// Use this when you already have a CompactLocation with file_id.
    pub fn compact_location(mut self, location: CompactLocation) -> Self {
        self.location_data = Some(LocationData::Compact(location));
        self
    }

    /// Set the entry kind (required)
    pub fn kind(mut self, kind: EntryKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Build the Entry with validation
    ///
    /// Requires mutable access to RubyIndex to:
    /// 1. Intern the FullyQualifiedName
    /// 2. Register the file URI if using LSP Location
    pub fn build(
        self,
        index: &mut crate::indexer::index::RubyIndex,
    ) -> Result<Entry, &'static str> {
        let fqn = self.fqn.ok_or("Fully qualified name (fqn) is required")?;
        let location_data = self.location_data.ok_or("Location is required")?;
        let kind = self.kind.ok_or("Entry kind is required")?;

        // 1. Intern FQN
        let fqn_id = index.intern_fqn(fqn);

        // 2. Resolve Location
        let location = match location_data {
            LocationData::Compact(compact) => compact,
            LocationData::Lsp(lsp) => {
                let file_id = index.get_or_insert_file(&lsp.uri);
                CompactLocation::new(file_id, lsp.range)
            }
        };

        Ok(Entry {
            fqn_id,
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
