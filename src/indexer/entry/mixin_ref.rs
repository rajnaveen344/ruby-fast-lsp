use crate::types::ruby_namespace::RubyConstant;

/// A purely textual reference to a mixin constant, captured before it is resolved.
/// This allows the indexer to remain single-pass and resolve the constant later,
/// during an on-demand query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MixinRef {
    /// The constant parts of the name, e.g., `["Foo", "Bar"]` for `Foo::Bar`.
    pub parts: Vec<RubyConstant>,
    /// True if the constant path began with `::`, indicating it's an absolute path.
    pub absolute: bool,
}
