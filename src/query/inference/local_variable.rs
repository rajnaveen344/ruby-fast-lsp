//! Local Variable Type Resolution
//!
//! Resolves types for local variables at specific positions in the source code.

use crate::indexer::index_ref::{Index, Unlocked};
use crate::inferrer::r#type::ruby::RubyType;
use crate::types::ruby_document::RubyDocument;
use crate::utils::position_to_offset;
use parking_lot::RwLock;
use std::sync::Arc;
use tower_lsp::lsp_types::Position;

/// Resolves types for local variables at specific positions
pub struct LocalVariableResolver<'a> {
    index: &'a Index<Unlocked>,
    document: &'a Arc<RwLock<RubyDocument>>,
}

impl<'a> LocalVariableResolver<'a> {
    /// Create a new LocalVariableResolver
    pub fn new(index: &'a Index<Unlocked>, document: &'a Arc<RwLock<RubyDocument>>) -> Self {
        Self { index, document }
    }

    /// Resolve the type of a local variable at the given position
    pub fn resolve(&self, name: &str, position: Position, content: &str) -> Option<RubyType> {
        let offset = position_to_offset(content, position);
        let doc = self.document.read();
        doc.get_var_type(offset, name).cloned()
    }

    /// Get a reference to the index
    #[allow(dead_code)]
    pub fn index(&self) -> &Index<Unlocked> {
        self.index
    }
}
