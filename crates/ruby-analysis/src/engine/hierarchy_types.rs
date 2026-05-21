use crate::core::{FullyQualifiedName, GraphNodeKind, SourceFileId, TextRange};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallHierarchyMethod {
    pub fqn: FullyQualifiedName,
    pub range: TextRange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IncomingCall {
    pub from: CallHierarchyMethod,
    pub from_ranges: Vec<TextRange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutgoingCall {
    pub to: CallHierarchyMethod,
    pub from_ranges: Vec<TextRange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeHierarchyRelation {
    Superclass,
    Include,
    Prepend,
    Extend,
    Subclass,
    IncludedBy,
    PrependedBy,
    ExtendedBy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeHierarchyEntry {
    pub fqn: FullyQualifiedName,
    pub node_kind: Option<GraphNodeKind>,
    pub relation: TypeHierarchyRelation,
    pub range: TextRange,
    pub edge_file_id: Option<SourceFileId>,
    pub unresolved: bool,
}
