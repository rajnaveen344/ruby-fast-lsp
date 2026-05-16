use crate::{FullyQualifiedName, RubyMethod, TextRange};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodCalleeResolution {
    Exact,
    ReceiverOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMethodCallee {
    pub owner: FullyQualifiedName,
    pub method: RubyMethod,
    pub resolution: MethodCalleeResolution,
    pub definition_ranges: Vec<TextRange>,
}
