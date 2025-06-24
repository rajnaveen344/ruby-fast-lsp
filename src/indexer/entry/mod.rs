pub mod entry_builder;
pub mod entry_kind;

use std::cmp::PartialEq;

use entry_kind::EntryKind;
use lsp_types::Location;

use crate::types::fully_qualified_name::FullyQualifiedName;

#[derive(Debug, Clone)]
pub struct Entry {
    /// The fully qualified name of this entity
    pub fqn: FullyQualifiedName,

    /// Location of the definition in source code
    pub location: Location,

    /// Type-specific metadata
    pub kind: EntryKind,
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

#[derive(Debug)]
pub enum Mixin {
    Include(FullyQualifiedName), // Module being included
    Extend(FullyQualifiedName),  // Module being extended
    Prepend(FullyQualifiedName),
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
