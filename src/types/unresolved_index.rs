//! Unresolved diagnostics
//!
//! Defines unresolved reference entries used to emit analysis diagnostics.

use tower_lsp::lsp_types::Location;

use crate::inferrer::r#type::ruby::RubyType;

// ============================================================================
// UnresolvedEntry
// ============================================================================

/// Represents an unresolved reference for diagnostics.
/// Used to report missing constants/classes/modules/methods.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnresolvedEntry {
    /// An unresolved constant reference (e.g., `Foo::Bar`)
    Constant {
        /// The constant name as written in the source (e.g., "Foo::Bar" or just "Bar")
        name: String,
        /// The namespace context where this reference was written
        /// e.g., ["Outer", "Inner"] for code inside `module Outer; module Inner; ... end; end`
        /// Used to determine if a newly defined constant would resolve this reference
        /// via Ruby's reverse namespace lookup
        namespace_context: Vec<String>,
        /// Location where the constant was referenced
        location: Location,
    },
    /// An unresolved method call (e.g., `foo.bar` or `bar`)
    Method {
        /// The method name as written in the source
        name: String,
        /// The receiver type if known
        /// None for method calls without explicit receiver (implicit self)
        /// Some(RubyType::Unknown) means explicit receiver with unknown type
        receiver_type: Option<RubyType>,
        /// Location where the method was called
        location: Location,
        /// Closest matching method name on the receiver's ancestors (Levenshtein-derived).
        /// `None` when no candidate is within threshold.
        suggestion: Option<String>,
    },
    /// A keyword argument at a callsite that the method doesn't declare
    /// (and the method doesn't accept `**kwargs`).
    UnknownKwarg {
        /// The method name
        method: String,
        /// The unknown keyword arg name (without trailing colon)
        kwarg: String,
        /// Closest matching declared keyword name (Levenshtein-derived)
        suggestion: Option<String>,
        /// Location of the keyword arg name at the callsite
        location: Location,
    },
    /// A method call with wrong number of positional arguments
    WrongArity {
        /// The method name
        name: String,
        /// Minimum required positional args
        expected_min: usize,
        /// Maximum positional args (None = unbounded due to splat)
        expected_max: Option<usize>,
        /// Actual positional args at callsite
        actual: usize,
        /// Location where the method was called (message location)
        location: Location,
    },
    /// One or more required keyword arguments are missing at a callsite.
    MissingKwarg {
        /// The method name
        method: String,
        /// All missing required keyword arg names (sorted, without trailing colon)
        missing: Vec<String>,
        /// Location of the method name at the callsite
        location: Location,
    },
    /// A `raise` call whose first argument is provably not an Exception subclass.
    RaiseNonException {
        /// The argument expression as written (e.g., "42", "[]", "MyClass")
        arg_repr: String,
        /// Location of the offending argument
        location: Location,
    },
    /// A splat whose target is provably the wrong type.
    /// `*expr` requires Array-like; `**expr` requires Hash-like.
    BadSplat {
        /// `"*"` or `"**"`
        operator: String,
        /// Argument expression as written (for the message)
        arg_repr: String,
        /// Expected type description (`"Array"` or `"Hash"`)
        expected: String,
        /// Location of the splat node (operator + target) for the underline
        location: Location,
    },
}

// Manual Hash implementation since Location from tower_lsp doesn't implement Hash
impl std::hash::Hash for UnresolvedEntry {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            UnresolvedEntry::Constant {
                name,
                namespace_context,
                location,
            } => {
                0u8.hash(state); // discriminant
                name.hash(state);
                namespace_context.hash(state);
                location.uri.hash(state);
                location.range.start.line.hash(state);
                location.range.start.character.hash(state);
                location.range.end.line.hash(state);
                location.range.end.character.hash(state);
            }
            UnresolvedEntry::Method {
                name,
                receiver_type,
                location,
                suggestion,
            } => {
                1u8.hash(state); // discriminant
                name.hash(state);
                receiver_type.hash(state);
                suggestion.hash(state);
                location.uri.hash(state);
                location.range.start.line.hash(state);
                location.range.start.character.hash(state);
                location.range.end.line.hash(state);
                location.range.end.character.hash(state);
            }
            UnresolvedEntry::UnknownKwarg {
                method,
                kwarg,
                suggestion,
                location,
            } => {
                3u8.hash(state); // discriminant
                method.hash(state);
                kwarg.hash(state);
                suggestion.hash(state);
                location.uri.hash(state);
                location.range.start.line.hash(state);
                location.range.start.character.hash(state);
                location.range.end.line.hash(state);
                location.range.end.character.hash(state);
            }
            UnresolvedEntry::WrongArity {
                name,
                expected_min,
                expected_max,
                actual,
                location,
            } => {
                2u8.hash(state); // discriminant
                name.hash(state);
                expected_min.hash(state);
                expected_max.hash(state);
                actual.hash(state);
                location.uri.hash(state);
                location.range.start.line.hash(state);
                location.range.start.character.hash(state);
                location.range.end.line.hash(state);
                location.range.end.character.hash(state);
            }
            UnresolvedEntry::MissingKwarg {
                method,
                missing,
                location,
            } => {
                4u8.hash(state); // discriminant
                method.hash(state);
                missing.hash(state);
                location.uri.hash(state);
                location.range.start.line.hash(state);
                location.range.start.character.hash(state);
                location.range.end.line.hash(state);
                location.range.end.character.hash(state);
            }
            UnresolvedEntry::RaiseNonException { arg_repr, location } => {
                5u8.hash(state); // discriminant
                arg_repr.hash(state);
                location.uri.hash(state);
                location.range.start.line.hash(state);
                location.range.start.character.hash(state);
                location.range.end.line.hash(state);
                location.range.end.character.hash(state);
            }
            UnresolvedEntry::BadSplat {
                operator,
                arg_repr,
                expected,
                location,
            } => {
                6u8.hash(state); // discriminant
                operator.hash(state);
                arg_repr.hash(state);
                expected.hash(state);
                location.uri.hash(state);
                location.range.start.line.hash(state);
                location.range.start.character.hash(state);
                location.range.end.line.hash(state);
                location.range.end.character.hash(state);
            }
        }
    }
}

impl UnresolvedEntry {
    /// Create an unresolved constant entry with namespace context
    pub fn constant_with_context(
        name: String,
        namespace_context: Vec<String>,
        location: Location,
    ) -> Self {
        Self::Constant {
            name,
            namespace_context,
            location,
        }
    }

    /// Create an unresolved constant entry (legacy, assumes root context)
    pub fn constant(name: String, location: Location) -> Self {
        Self::Constant {
            name,
            namespace_context: Vec::new(),
            location,
        }
    }

    /// Create an unresolved method entry
    pub fn method(name: String, receiver_type: Option<RubyType>, location: Location) -> Self {
        Self::Method {
            name,
            receiver_type,
            location,
            suggestion: None,
        }
    }

    /// Create an unresolved method entry with a Levenshtein-derived suggestion
    pub fn method_with_suggestion(
        name: String,
        receiver_type: Option<RubyType>,
        location: Location,
        suggestion: Option<String>,
    ) -> Self {
        Self::Method {
            name,
            receiver_type,
            location,
            suggestion,
        }
    }

    /// Create a wrong-arity entry
    pub fn wrong_arity(
        name: String,
        expected_min: usize,
        expected_max: Option<usize>,
        actual: usize,
        location: Location,
    ) -> Self {
        Self::WrongArity {
            name,
            expected_min,
            expected_max,
            actual,
            location,
        }
    }

    /// Create an unknown-kwarg entry
    pub fn unknown_kwarg(
        method: String,
        kwarg: String,
        suggestion: Option<String>,
        location: Location,
    ) -> Self {
        Self::UnknownKwarg {
            method,
            kwarg,
            suggestion,
            location,
        }
    }

    /// Create a missing-kwarg entry
    pub fn missing_kwarg(method: String, missing: Vec<String>, location: Location) -> Self {
        Self::MissingKwarg {
            method,
            missing,
            location,
        }
    }

    /// Create a raise-non-exception entry
    pub fn raise_non_exception(arg_repr: String, location: Location) -> Self {
        Self::RaiseNonException { arg_repr, location }
    }

    /// Create a bad-splat entry
    pub fn bad_splat(
        operator: String,
        arg_repr: String,
        expected: String,
        location: Location,
    ) -> Self {
        Self::BadSplat {
            operator,
            arg_repr,
            expected,
            location,
        }
    }

    /// Get the location of this unresolved entry
    pub fn location(&self) -> &Location {
        match self {
            Self::Constant { location, .. } => location,
            Self::Method { location, .. } => location,
            Self::WrongArity { location, .. } => location,
            Self::UnknownKwarg { location, .. } => location,
            Self::MissingKwarg { location, .. } => location,
            Self::RaiseNonException { location, .. } => location,
            Self::BadSplat { location, .. } => location,
        }
    }

    /// Get the name of this entry (constant name or method name)
    pub fn name(&self) -> &str {
        match self {
            Self::Constant { name, .. } => name,
            Self::Method { name, .. } => name,
            Self::WrongArity { name, .. } => name,
            Self::UnknownKwarg { method, .. } => method,
            Self::MissingKwarg { method, .. } => method,
            Self::RaiseNonException { arg_repr, .. } => arg_repr,
            Self::BadSplat { arg_repr, .. } => arg_repr,
        }
    }
}
