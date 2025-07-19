pub mod entry_builder;
pub mod entry_kind;

use std::cmp::PartialEq;

use entry_kind::EntryKind;
use lsp_types::Location;

use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_namespace::RubyConstant;

#[derive(Debug, Clone)]
pub struct Entry {
    /// The fully qualified name of this entity
    pub fqn: FullyQualifiedName,

    /// Location of the definition in source code
    pub location: Location,

    /// Type-specific metadata
    pub kind: EntryKind,
}

impl Entry {
    pub fn add_includes(&mut self, mixin_refs: Vec<MixinRef>) {
        self.kind.add_includes(mixin_refs);
    }

    pub fn add_extends(&mut self, mixin_refs: Vec<MixinRef>) {
        self.kind.add_extends(mixin_refs);
    }

    pub fn add_prepends(&mut self, mixin_refs: Vec<MixinRef>) {
        self.kind.add_prepends(mixin_refs);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MethodKind {
    /// Instance method defined in a class/module.
    /// Called on instances: `obj.method`
    /// Example: `def foo; end` in class body
    Instance,

    /// Class method defined on a class/module.
    /// Called on the class itself: `MyClass.method`
    /// Example: `def self.bar; end` or `class << self; def bar; end`
    Class,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MethodOrigin {
    /// Defined directly on the owner
    Direct,
    /// Inherited via class inheritance
    Inherited(FullyQualifiedName),
    /// Included via module
    Included(FullyQualifiedName),
    /// Extended via module
    Extended(FullyQualifiedName),
    /// Prepended via module
    Prepended(FullyQualifiedName),
}

/// Method visibility in Ruby
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MethodVisibility {
    Public,
    Protected,
    Private,
}

/// Constant visibility in Ruby
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConstVisibility {
    Public,
    Private,
}

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
