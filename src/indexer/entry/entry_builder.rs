use std::collections::HashMap;

use lsp_types::Location;

use crate::types::fully_qualified_name::FullyQualifiedName;

use super::{Entry, EntryKind};

pub struct EntryBuilder {
    fqn: Option<FullyQualifiedName>,
    location: Option<Location>,
    kind: Option<EntryKind>,
    metadata: HashMap<String, String>,
}

impl EntryBuilder {
    /// Create a new builder with empty fields
    pub fn new() -> Self {
        EntryBuilder {
            fqn: None,
            location: None,
            kind: None,
            metadata: HashMap::new(),
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

    /// Set the entry type (required)
    pub fn kind(mut self, kind: EntryKind) -> Self {
        self.kind = Some(kind);
        self
    }
    /// Add metadata key-value pair (optional)
    pub fn metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
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
            metadata: self.metadata,
        })
    }
}
