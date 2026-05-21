use crate::core::{FullyQualifiedName, RubyType, SymbolKind, TextRange};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstantLookupRequest {
    pub partial_name: String,
    pub namespace_prefix: Option<FullyQualifiedName>,
    pub is_qualified: bool,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstantMatch {
    pub fqn: FullyQualifiedName,
    pub kind: SymbolKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodMatch {
    pub name: String,
    pub params: Vec<String>,
    pub return_type: Option<RubyType>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MixinUsageKind {
    Include,
    Prepend,
    Extend,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MixinUsage {
    pub kind: MixinUsageKind,
    pub range: TextRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VariableTypeKind {
    Local,
    Instance,
    Class,
    Global,
    Constant,
}
