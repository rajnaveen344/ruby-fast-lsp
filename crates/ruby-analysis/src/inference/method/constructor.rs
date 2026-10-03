//! What `Receiver.new` returns for a class receiver.
//!
//! The default `Class#new` returns an instance of its receiver. A class may
//! replace it with an explicit singleton `new`: core `Struct.new`, for example,
//! builds and returns a new class. Only such a declaration overrides the
//! instance result, and a declared result without a concrete type stays
//! Unknown instead of falling back to an instance of the receiver.
//!
//! The engine walks the receiver's singleton lookup chain and asks
//! [`declared_singleton_new`] about each ancestor. Seed facts produced before
//! the project graph is installed consult only the receiver itself through
//! [`seed_constructor_type`]; the engine solve replaces them afterwards.

use rbs_parser::RbsType;

use crate::core::{
    FullyQualifiedName, RubyConstant, RubyType, TypeInferenceOutcome, UnknownReason,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConstructorResult {
    /// `new` reaches the default constructor and returns a receiver instance.
    Instance,
    /// An explicit singleton `new` declaration replaces the default constructor.
    Declared(TypeInferenceOutcome),
}

impl ConstructorResult {
    /// The call outcome, given the receiver instance type the caller represents.
    pub fn into_type_outcome(self, instance: RubyType) -> TypeInferenceOutcome {
        match self {
            Self::Instance => TypeInferenceOutcome::proven(instance),
            Self::Declared(outcome) => outcome,
        }
    }
}

/// The embedded RBS singleton `new` declared directly on one namespace, if any.
///
/// RBS `initialize` declarations are instance methods in the catalog, so they
/// never appear here; only an explicit `def self.new` does.
pub fn declared_singleton_new(namespace_parts: &[RubyConstant]) -> Option<ConstructorResult> {
    if namespace_parts.is_empty() {
        return None;
    }
    let name = namespace_parts
        .iter()
        .map(RubyConstant::as_str)
        .collect::<Vec<_>>()
        .join("::");
    let declared = crate::inference::rbs::get_rbs_method_return_type(&name, "new", true)?;
    Some(match declared {
        RbsType::Instance | RbsType::SelfType => ConstructorResult::Instance,
        RbsType::Class(_)
        | RbsType::ClassInstance { .. }
        | RbsType::Interface(_)
        | RbsType::TypeVar(_)
        | RbsType::Union(_)
        | RbsType::Intersection(_)
        | RbsType::Optional(_)
        | RbsType::Tuple(_)
        | RbsType::Record(_)
        | RbsType::Proc(_)
        | RbsType::Literal(_)
        | RbsType::ClassType
        | RbsType::Void
        | RbsType::Nil
        | RbsType::Bool
        | RbsType::Untyped
        | RbsType::Top
        | RbsType::Bot => {
            let ruby_type = crate::inference::rbs::rbs_type_to_ruby_type(&declared);
            ConstructorResult::Declared(if RubyType::contains_unknown(&ruby_type) {
                TypeInferenceOutcome::unknown(UnknownReason::UnresolvedMethodReturn)
            } else {
                TypeInferenceOutcome::proven(ruby_type)
            })
        }
    })
}

/// A seed type for `class.new` before the engine resolves the project graph.
///
/// Returns the receiver instance unless the receiver itself declares a
/// replacement singleton `new`, in which case only a concrete declared result
/// is returned. The engine's constructor projection remains authoritative.
pub fn seed_constructor_type(class: &FullyQualifiedName) -> Option<RubyType> {
    declared_singleton_new(class.namespace_parts_slice())
        .unwrap_or(ConstructorResult::Instance)
        .into_type_outcome(RubyType::Class(class.clone()))
        .into_proven_type()
}
