//! Entry Types
//!
//! Defines the data structures for representing indexed Ruby entities.
//!
//! ## Components
//!
//! - **`Entry`**: The main indexed item containing FQN, location, and kind
//! - **`EntryKind`**: Type-specific metadata (Class, Module, Method, etc.)
//! - **`EntryBuilder`**: Builder pattern for constructing entries

pub mod entry_builder;
pub mod entry_kind;

use std::cmp::PartialEq;

pub use entry_kind::EntryKind;
use tower_lsp::lsp_types::Location;

use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_namespace::RubyConstant;

// ============================================================================
// Entry
// ============================================================================

/// An indexed Ruby entity (class, module, method, constant, variable)
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

    pub fn set_superclass(&mut self, superclass_ref: MixinRef) {
        self.kind.set_superclass(superclass_ref);
    }
}

// ============================================================================
// Method Types
// ============================================================================

/// Distinguishes between instance and class methods
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MethodKind {
    /// Instance method called on objects: `obj.method`
    Instance,
    /// Class method called on the class: `MyClass.method`
    Class,
    /// Unknown kind - search for both
    Unknown,
}

/// How a method was obtained (directly defined or inherited/mixed in)
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

// ============================================================================
// Constant Types
// ============================================================================

/// Constant visibility in Ruby
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConstVisibility {
    Public,
    Private,
}

// ============================================================================
// Mixin Types
// ============================================================================

/// A textual reference to a mixin constant, captured before resolution.
/// Allows single-pass indexing with lazy resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MixinRef {
    /// The constant parts of the name (e.g., `["Foo", "Bar"]` for `Foo::Bar`)
    pub parts: Vec<RubyConstant>,
    /// True if the path began with `::` (absolute path)
    pub absolute: bool,
}

/// Type of mixin operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MixinType {
    Include,
    Prepend,
    Extend,
}
