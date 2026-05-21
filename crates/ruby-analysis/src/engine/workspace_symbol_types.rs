use crate::core::{SymbolKind, TextRange};

#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceSymbolMatch {
    pub name: String,
    pub kind: SymbolKind,
    pub range: TextRange,
    pub container_name: Option<String>,
    pub relevance: f64,
}
