//! Local Variable Type Resolution
//!
//! Resolves types for local variables at specific positions in the source code.

use crate::indexer::index_ref::{Index, Unlocked};
use crate::inferrer::r#type::ruby::RubyType;
use crate::types::ruby_document::RubyDocument;
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
    pub fn resolve(&self, name: &str, position: Position, _content: &str) -> Option<RubyType> {
        let doc = self.document.read();
        let scope_id = doc
            .variable_scopes()
            .find_scope_for_variable_at(name, position)
            .or_else(|| doc.variable_scopes().scope_at_position(position))?;
        let ty = doc
            .variable_scopes()
            .get_type_at_position(name, scope_id, position)?;
        if *ty != RubyType::Unknown {
            Some(ty.clone())
        } else {
            None
        }
    }

    /// Get a reference to the index
    #[allow(dead_code)]
    pub fn index(&self) -> &Index<Unlocked> {
        self.index
    }
}
