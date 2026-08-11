//! File-owned equations for types that depend on Ruby constants.
//!
//! The indexer emits these compact terms during its ordinary Prism traversal.
//! The engine resolves their lexical constant lookups only after the complete
//! project graph is installed, so consumers never require a second parse.

use std::collections::BTreeSet;

use super::{FullyQualifiedName, RubyConstant, TextRange, TypeSubject};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConstantTypeProjection {
    Value,
    ConstructorInstance,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConstantTypeDependency {
    pub parts: Vec<RubyConstant>,
    pub absolute: bool,
    pub lexical_context: Vec<RubyConstant>,
    projection: ConstantTypeProjection,
}

impl ConstantTypeDependency {
    pub fn new(
        parts: Vec<RubyConstant>,
        absolute: bool,
        lexical_context: Vec<RubyConstant>,
    ) -> Self {
        assert!(
            !parts.is_empty(),
            "INVARIANT VIOLATED: a constant-value dependency has an empty path. This is a bug because every Ruby constant reference has at least one name. Fix: construct dependencies only from validated Prism constant nodes."
        );
        Self {
            parts,
            absolute,
            lexical_context,
            projection: ConstantTypeProjection::Value,
        }
    }

    pub fn constructor(
        parts: Vec<RubyConstant>,
        absolute: bool,
        lexical_context: Vec<RubyConstant>,
    ) -> Self {
        let mut dependency = Self::new(parts, absolute, lexical_context);
        dependency.projection = ConstantTypeProjection::ConstructorInstance;
        dependency
    }

    pub fn projection(&self) -> ConstantTypeProjection {
        self.projection
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ConstantTypeTarget {
    Fact {
        subject: TypeSubject,
        range: TextRange,
    },
    LocalAssignment {
        name: String,
        range: TextRange,
    },
    LocalRead(TextRange),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConstantTypeEquation {
    target: ConstantTypeTarget,
    dependencies: BTreeSet<ConstantTypeDependency>,
}

impl ConstantTypeEquation {
    pub fn dependency(target: ConstantTypeTarget, dependency: ConstantTypeDependency) -> Self {
        Self {
            target,
            dependencies: BTreeSet::from([dependency]),
        }
    }

    pub fn from_dependencies(
        target: ConstantTypeTarget,
        dependencies: BTreeSet<ConstantTypeDependency>,
    ) -> Self {
        assert!(
            !dependencies.is_empty(),
            "INVARIANT VIOLATED: a constant type equation has no constant dependency. This is a bug because dependency-free types are ordinary TypeFacts. Fix: emit ConstantTypeEquation only for a retained constant lookup."
        );
        Self {
            target,
            dependencies,
        }
    }

    pub fn target(&self) -> &ConstantTypeTarget {
        &self.target
    }

    pub fn dependencies(&self) -> &BTreeSet<ConstantTypeDependency> {
        &self.dependencies
    }

    pub fn constant_target(&self) -> Option<&FullyQualifiedName> {
        match &self.target {
            ConstantTypeTarget::Fact {
                subject: TypeSubject::Constant(constant),
                ..
            } => Some(constant),
            ConstantTypeTarget::Fact { .. }
            | ConstantTypeTarget::LocalAssignment { .. }
            | ConstantTypeTarget::LocalRead(_) => None,
        }
    }
}
