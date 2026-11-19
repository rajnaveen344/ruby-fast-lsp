//! File-owned proof equations for inferred method returns.
//!
//! The parser-facing traversal emits this compact, editor-independent IR. A
//! solver may combine equations from one file or a complete project without
//! retaining Prism nodes or reimplementing Ruby method lookup.

use std::collections::BTreeSet;
use std::mem::size_of;

use crate::core::storage::memory_estimate::{fqn_heap_bytes, ruby_type_heap_bytes};
use crate::core::{
    ConstantTypeDependency, FullyQualifiedName, MethodReturnObservation, RubyConstant, RubyType,
    TypeInferenceOutcome, UnknownReason,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum MethodReturnBase {
    Bottom,
    Proven(RubyType),
    Unknown(UnknownReason),
}

/// Compact equation `method = base | dependencies...`.
///
/// `Bottom` is private solver state and is never represented by
/// `RubyType::Unknown` in a published result.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MethodReturnEquation {
    method: FullyQualifiedName,
    base: MethodReturnBase,
    /// Sorted and unique.
    dependencies: Box<[FullyQualifiedName]>,
    /// Sorted and unique.
    constant_dependencies: Box<[ConstantTypeDependency]>,
    /// Sorted and unique. Returns of other methods the base was derived
    /// from; the base holds only while each lookup still gives that answer.
    observations: Box<[MethodReturnObservation]>,
    /// Sorted and unique. Same-file methods whose collected return the base
    /// was derived from; the base is withheld whenever one of theirs is.
    local_callees: Box<[FullyQualifiedName]>,
}

impl MethodReturnEquation {
    pub(crate) fn new(
        method: FullyQualifiedName,
        base: MethodReturnBase,
        dependencies: BTreeSet<FullyQualifiedName>,
    ) -> Self {
        Self {
            method,
            base,
            dependencies: dependencies.into_iter().collect(),
            constant_dependencies: Box::default(),
            observations: Box::default(),
            local_callees: Box::default(),
        }
    }

    pub(crate) fn with_observations(
        mut self,
        observations: BTreeSet<MethodReturnObservation>,
        local_callees: BTreeSet<FullyQualifiedName>,
    ) -> Self {
        self.observations = observations.into_iter().collect();
        self.local_callees = local_callees
            .into_iter()
            .filter(|callee| *callee != self.method)
            .collect();
        self
    }

    pub(crate) fn local_callees(&self) -> &[FullyQualifiedName] {
        &self.local_callees
    }

    pub(crate) fn observations(&self) -> &[MethodReturnObservation] {
        &self.observations
    }

    /// The equation once an observation no longer holds: its base was
    /// derived from an answer that changed, so nothing is proven.
    pub(crate) fn with_withheld_base(&self) -> Self {
        Self {
            method: self.method.clone(),
            base: MethodReturnBase::Unknown(UnknownReason::UnresolvedMethodReturn),
            dependencies: Box::default(),
            constant_dependencies: Box::default(),
            observations: self.observations.clone(),
            local_callees: self.local_callees.clone(),
        }
    }

    pub(crate) fn with_constant_dependencies(
        mut self,
        dependencies: BTreeSet<ConstantTypeDependency>,
    ) -> Self {
        self.constant_dependencies = dependencies.into_iter().collect();
        self
    }

    pub(crate) fn proven(method: FullyQualifiedName, ruby_type: RubyType) -> Self {
        invariant!(
            !RubyType::union_members_contain_unknown(&ruby_type),
            what = "a proven method-return equation contains Unknown (exactly or inside a union member)",
            why = "unknown cannot be a concrete equation base and union members absorb Unknown",
            fix = "construct an unknown equation with its precise reason",
        );
        Self::new(method, MethodReturnBase::Proven(ruby_type), BTreeSet::new())
    }

    pub(crate) fn from_ruby_type(
        method: FullyQualifiedName,
        ruby_type: RubyType,
        unknown_reason: UnknownReason,
    ) -> Self {
        if RubyType::union_members_contain_unknown(&ruby_type) {
            Self::new(
                method,
                MethodReturnBase::Unknown(unknown_reason),
                BTreeSet::new(),
            )
        } else {
            Self::proven(method, ruby_type)
        }
    }

    pub(crate) fn method(&self) -> &FullyQualifiedName {
        &self.method
    }

    pub(crate) fn base(&self) -> &MethodReturnBase {
        &self.base
    }

    pub(crate) fn dependencies(&self) -> &[FullyQualifiedName] {
        &self.dependencies
    }

    pub fn constant_dependencies(&self) -> &[ConstantTypeDependency] {
        &self.constant_dependencies
    }

    pub(crate) fn with_resolved_constant_types(
        &self,
        resolved: impl IntoIterator<Item = Option<RubyType>>,
    ) -> Self {
        if self.constant_dependencies.is_empty() {
            return self.clone();
        }
        let mut members = Vec::new();
        match &self.base {
            MethodReturnBase::Bottom => {}
            MethodReturnBase::Proven(ruby_type) => members.push(ruby_type.clone()),
            MethodReturnBase::Unknown(reason) => {
                let mut equation = self.clone();
                equation.base = MethodReturnBase::Unknown(*reason);
                equation.constant_dependencies = Box::default();
                return equation;
            }
        }
        for ruby_type in resolved {
            let Some(ruby_type) = ruby_type else {
                let mut equation = self.clone();
                equation.base = MethodReturnBase::Unknown(UnknownReason::UnresolvedAssignmentValue);
                equation.constant_dependencies = Box::default();
                return equation;
            };
            if RubyType::union_members_contain_unknown(&ruby_type) {
                let mut equation = self.clone();
                equation.base = MethodReturnBase::Unknown(UnknownReason::UnresolvedAssignmentValue);
                equation.constant_dependencies = Box::default();
                return equation;
            }
            members.push(ruby_type);
        }
        let mut equation = self.clone();
        equation.base = if members.is_empty() {
            MethodReturnBase::Bottom
        } else {
            MethodReturnBase::Proven(RubyType::union(members))
        };
        equation.constant_dependencies = Box::default();
        equation
    }

    /// Project the equation before its complete dependency graph is solved.
    ///
    /// A dependency-free proven base is immediately safe to expose to later
    /// methods in the same traversal. Any dependency stays Unknown until an
    /// SCC solve has every participating equation.
    pub(crate) fn immediate_outcome(&self) -> TypeInferenceOutcome {
        match (
            &self.base,
            self.dependencies.is_empty() && self.constant_dependencies.is_empty(),
        ) {
            (MethodReturnBase::Proven(ruby_type), true) => {
                TypeInferenceOutcome::proven(ruby_type.clone())
            }
            (MethodReturnBase::Unknown(reason), _) => TypeInferenceOutcome::unknown(*reason),
            (MethodReturnBase::Bottom | MethodReturnBase::Proven(_), false)
            | (MethodReturnBase::Bottom, true) => {
                TypeInferenceOutcome::unknown(UnknownReason::UnresolvedMethodReturn)
            }
        }
    }

    pub(crate) fn estimated_heap_bytes(&self) -> usize {
        fqn_heap_bytes(&self.method)
            + match &self.base {
                MethodReturnBase::Bottom | MethodReturnBase::Unknown(_) => 0,
                MethodReturnBase::Proven(ruby_type) => ruby_type_heap_bytes(ruby_type),
            }
            + self.dependencies.len() * size_of::<FullyQualifiedName>()
            + self.dependencies.iter().map(fqn_heap_bytes).sum::<usize>()
            + self.constant_dependencies.len() * size_of::<ConstantTypeDependency>()
            + self
                .constant_dependencies
                .iter()
                .map(|dependency| {
                    (dependency.parts.len() + dependency.lexical_context.len())
                        * size_of::<RubyConstant>()
                })
                .sum::<usize>()
            + self
                .observations
                .iter()
                .map(MethodReturnObservation::estimated_heap_bytes)
                .sum::<usize>()
            + self.local_callees.len() * size_of::<FullyQualifiedName>()
            + self.local_callees.iter().map(fqn_heap_bytes).sum::<usize>()
    }
}
