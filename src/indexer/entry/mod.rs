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

pub use entry_builder::EntryBuilder;
pub use entry_kind::EntryKind;

use crate::indexer::index::FqnId;
use crate::types::compact_location::CompactLocation;
use crate::types::fully_qualified_name::FullyQualifiedName;
use crate::types::ruby_namespace::RubyConstant;

// ============================================================================
// Entry
// ============================================================================

/// An indexed Ruby entity (class, module, method, constant, variable)
#[derive(Debug, Clone)]
pub struct Entry {
    /// The fully qualified name ID of this entity
    pub fqn_id: FqnId,
    /// Location of the definition in source code (compact representation)
    pub location: CompactLocation,
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
// Namespace Types
// ============================================================================

/// Distinguishes between instance namespace and singleton namespace
/// In Ruby, all methods are instance methods - they differ in which namespace they belong to:
/// - Instance namespace: Regular class/module (Foo)
/// - Singleton namespace: The singleton class (#<Class:Foo>)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NamespaceKind {
    /// Instance namespace: methods defined on the regular class
    Instance,
    /// Singleton namespace: methods defined on the singleton class (class methods)
    Singleton,
}

// Alias for backward compatibility during migration
pub type MethodKind = NamespaceKind;

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
    /// Location where the include/extend/prepend call was written
    pub location: CompactLocation,
}

/// Type of mixin operation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MixinType {
    Include,
    Prepend,
    Extend,
}
