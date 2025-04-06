pub mod entry_builder;
pub mod entry_kind;

use std::cmp::PartialEq;
use std::collections::HashMap;

use entry_kind::EntryKind;
use lsp_types::Location;

use super::types::fully_qualified_name::FullyQualifiedName;

#[derive(Debug, Clone)]
pub struct Entry {
    /// The fully qualified name of this entity
    pub fqn: FullyQualifiedName,

    /// Location of the definition in source code
    pub location: Location,

    /// Type-specific metadata
    pub kind: EntryKind,

    /// Additional metadata (docstrings, annotations, etc)
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MethodKind {
    /// Instance method defined in a class/module.
    /// Called on instances: `obj.method`
    /// Example: `def foo; end` in class body
    Instance,

    /// Class method defined on a class/module.
    /// Called on the class itself: `MyClass.method`
    /// Example: `def self.bar; end` or `class << self; def bar; end`
    Class,

    /// Singleton method defined on a specific object's eigenclass.
    /// Exists outside normal class hierarchy.
    /// Example: `obj = MyClass.new; def obj.unique_method; end`
    Singleton,

    /// Module function created with `module_function`.
    /// Has dual nature:
    /// - Public class method on the module (`MyModule.method`)
    /// - Private instance method when included in other classes
    ModuleFunc,
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
