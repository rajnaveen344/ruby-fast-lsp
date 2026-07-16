use crate::{FullyQualifiedName, TextRange};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExecutionScopeMode {
    Preserve,
}

/// Framework-neutral runtime ownership active inside a Ruby block.
///
/// Lexical constant lookup and local closure behavior remain explicit and
/// independent from implicit receiver and method-definition ownership.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionContextFact {
    pub range: TextRange,
    pub lexical_namespace: FullyQualifiedName,
    pub implicit_receiver: FullyQualifiedName,
    pub method_definition_owner: FullyQualifiedName,
    pub lexical_scope: ExecutionScopeMode,
    pub local_scope: ExecutionScopeMode,
    pub extension_id: String,
}
