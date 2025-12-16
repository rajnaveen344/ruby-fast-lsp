//! Compact Location Types
//!
//! Memory-efficient location storage for indexed entries.
//! Uses file IDs (SlotMap keys) instead of URLs while keeping the standard LSP Range type.

use tower_lsp::lsp_types::Range;

use crate::indexer::index::FileId;

// ============================================================================
// CompactLocation
// ============================================================================

/// A memory-efficient location (~24 bytes vs ~104 bytes for Location)
///
/// Instead of storing the full URL, we store a FileId (SlotMap key).
/// We keep the standard LSP Range for compatibility with existing code.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CompactLocation {
    /// SlotMap key to look up the URL in RubyIndex.files
    pub file_id: FileId,
    /// Standard LSP Range (start/end positions)
    pub range: Range,
}

impl CompactLocation {
    pub fn new(file_id: FileId, range: Range) -> Self {
        Self { file_id, range }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compact_location_size() {
        // CompactLocation should be smaller than Location
        // FileId (8 bytes) + Range (16 bytes) = 24 bytes
        // This is still much smaller than Location which includes a full URL
        println!(
            "CompactLocation size: {}",
            std::mem::size_of::<CompactLocation>()
        );
        println!("FileId size: {}", std::mem::size_of::<FileId>());
        assert!(std::mem::size_of::<CompactLocation>() <= 32);
    }
}
