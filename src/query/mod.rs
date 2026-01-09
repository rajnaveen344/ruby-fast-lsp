//! Query Engine - Unified query layer for RubyIndex
//!
//! This module provides a single entry point for all index queries, consolidating
//! business logic that was previously scattered across capabilities.
//!
//! # Architecture
//!
//! ```text
//! server.rs (API) → query/ (Service) → indexer/ (Data)
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! let query = IndexQuery::new(&index);
//! let definitions = query.find_definitions(&uri, position);
//! ```

mod definition;
mod hover;
mod method;
mod references;
mod types;

pub use hover::HoverInfo;
pub use method::MethodInfo;
pub use types::{TypeHint, TypeHintKind, TypeQuery};

use crate::indexer::index::RubyIndex;
use crate::types::ruby_document::RubyDocument;
use tower_lsp::lsp_types::Url;

/// Unified query interface for RubyIndex.
///
/// Provides high-level, domain-focused query methods that abstract away
/// the low-level index details. All index-heavy capability logic should
/// use this interface.
pub struct IndexQuery<'a> {
    index: &'a RubyIndex,
    doc: Option<&'a RubyDocument>,
    uri: Option<&'a Url>,
    content: Option<&'a [u8]>,
}

impl<'a> IndexQuery<'a> {
    /// Create a new IndexQuery for index-wide queries.
    pub fn new(index: &'a RubyIndex) -> Self {
        Self {
            index,
            doc: None,
            uri: None,
            content: None,
        }
    }

    /// Create an IndexQuery with full document context.
    /// This enables access to local variables and AST-based queries.
    pub fn with_doc(index: &'a RubyIndex, doc: &'a RubyDocument) -> Self {
        Self {
            index,
            doc: Some(doc),
            uri: Some(&doc.uri),
            content: Some(doc.content.as_bytes()),
        }
    }

    /// Create an IndexQuery with raw file context (fallback/testing).
    pub fn for_file(index: &'a RubyIndex, uri: &'a Url, content: &'a [u8]) -> Self {
        Self {
            index,
            doc: None,
            uri: Some(uri),
            content: Some(content),
        }
    }

    /// Get a reference to the underlying index.
    #[inline]
    pub fn index(&self) -> &RubyIndex {
        self.index
    }

    /// Get the current file URI if set.
    #[inline]
    pub fn uri(&self) -> Option<&Url> {
        self.uri
    }

    /// Get the current file content if set.
    #[inline]
    pub fn content(&self) -> Option<&[u8]> {
        self.content
    }

    /// Get the document if attached.
    #[inline]
    pub fn doc(&self) -> Option<&RubyDocument> {
        self.doc
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_index_query_creation() {
        let index = RubyIndex::new();
        let query = IndexQuery::new(&index);
        assert!(query.uri().is_none());
        assert!(query.content().is_none());
    }

    #[test]
    fn test_index_query_with_file_context() {
        let index = RubyIndex::new();
        let uri = Url::parse("file:///test.rb").unwrap();
        let content = b"class Foo; end";
        let query = IndexQuery::for_file(&index, &uri, content);
        assert_eq!(query.uri(), Some(&uri));
        assert_eq!(query.content(), Some(content.as_slice()));
    }
}
