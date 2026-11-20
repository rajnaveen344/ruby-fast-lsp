//! File-owned evidence for instance variables returned by methods.
//!
//! A reader method returns whatever any assignment in the object's whole
//! hierarchy left in the variable, so its type is a project-wide question.
//! The traversal records each assignment and each returned read; the engine
//! combines them once every participating file is installed.

use std::mem::size_of;

use ustr::Ustr;

use crate::core::storage::memory_estimate::{fqn_heap_bytes, ruby_type_heap_bytes};
use crate::core::{FullyQualifiedName, RubyType};

/// An instance variable a method returns directly.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InstanceVariableDependency {
    /// The namespace, with its instance or singleton kind, whose object holds
    /// the variable.
    pub owner: FullyQualifiedName,
    pub name: Ustr,
}

/// The type a method body was evaluated with for an instance variable it
/// calls a method on. The body's result holds only while the project still
/// proves exactly that type.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct InstanceVariableObservation {
    pub variable: InstanceVariableDependency,
    pub observed: RubyType,
    /// The memoising assignment of this file, by its [`InstanceVariableWrite::memo`]
    /// number, that `observed` leaves out: `@name ||= value` evaluated with
    /// what every other assignment proves. `None` observes all of them.
    pub except_memo: Option<u32>,
}

impl InstanceVariableObservation {
    pub(crate) fn estimated_heap_bytes(&self) -> usize {
        self.variable.estimated_heap_bytes() + ruby_type_heap_bytes(&self.observed)
    }
}

/// One assignment to an instance variable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceVariableWrite {
    pub owner: FullyQualifiedName,
    pub name: Ustr,
    /// The assigned type when the source alone proves it: a scalar literal
    /// or a declared parameter the method never reassigns. `None` for every
    /// other value and for writers the source does not spell out.
    pub proven: Option<RubyType>,
    /// Whether every completed `initialize` reaches this assignment: a plain
    /// top-level statement of the constructor with no earlier `return`.
    pub in_constructor: bool,
    /// Numbers a `@name ||= value` assignment within its file, so a method
    /// returning it can name it without a source position.
    pub memo: Option<u32>,
}

impl InstanceVariableWrite {
    /// The name recorded when a writer is generated for names the source
    /// does not spell out. It leaves every variable of that owner unproven.
    pub(crate) const ANY_NAME: &'static str = "@*";

    pub(crate) fn estimated_heap_bytes(&self) -> usize {
        fqn_heap_bytes(&self.owner) + self.proven.as_ref().map_or(0, ruby_type_heap_bytes)
    }

    pub(crate) const fn inline_bytes() -> usize {
        size_of::<Self>()
    }
}

impl InstanceVariableDependency {
    pub(crate) fn estimated_heap_bytes(&self) -> usize {
        fqn_heap_bytes(&self.owner)
    }
}
