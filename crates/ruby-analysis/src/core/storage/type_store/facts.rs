//! Public type-fact model: file ids, byte ranges, subjects, provenance, and resolutions.

use crate::core::{FullyQualifiedName, RubyType};
use ustr::Ustr;

/// Stable file identifier owned by the analysis layer.
///
/// Editor adapters can map this to URIs; agent adapters can map it to paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct SourceFileId(pub u32);

/// Byte range in a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct TextRange {
    pub file_id: SourceFileId,
    pub start_byte: u32,
    pub end_byte: u32,
}

impl TextRange {
    pub fn new(file_id: SourceFileId, start_byte: u32, end_byte: u32) -> Self {
        invariant!(
            start_byte <= end_byte,
            what = "TextRange start_byte must be <= end_byte",
            why = "byte ranges must be normalized before storage",
            fix = "construct TextRange with sorted byte offsets",
        );
        Self {
            file_id,
            start_byte,
            end_byte,
        }
    }

    pub fn contains_offset(&self, file_id: SourceFileId, byte_offset: u32) -> bool {
        self.file_id == file_id && self.start_byte <= byte_offset && byte_offset <= self.end_byte
    }

    pub(super) fn starts_before_or_at(&self, file_id: SourceFileId, byte_offset: u32) -> bool {
        self.file_id == file_id && self.start_byte <= byte_offset
    }
}

/// Typed program entity that can have facts.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum TypeSubject {
    Constant(FullyQualifiedName),
    Local {
        scope_id: u32,
        name: Ustr,
    },
    InstanceVariable {
        owner: FullyQualifiedName,
        name: Ustr,
    },
    ClassVariable {
        owner: FullyQualifiedName,
        name: Ustr,
    },
    GlobalVariable(Ustr),
    MethodReturn(FullyQualifiedName),
    Parameter {
        method: FullyQualifiedName,
        name: Ustr,
    },
    Expression(TextRange),
}

/// Where a type fact came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeProvenance {
    Literal,
    Assignment,
    Flow,
    Rbs,
    Yard,
    Runtime,
    Extension,
    Inferred,
}

/// One type assignment/narrowing fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeFact {
    pub subject: TypeSubject,
    pub ruby_type: RubyType,
    pub range: TextRange,
    pub provenance: TypeProvenance,
}

impl TypeFact {
    pub fn new(
        subject: TypeSubject,
        ruby_type: RubyType,
        range: TextRange,
        provenance: TypeProvenance,
    ) -> Self {
        Self {
            subject,
            ruby_type,
            range,
            provenance,
        }
    }
}

/// Deterministic type query result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeResolution {
    Resolved(TypeFact),
    Ambiguous(Vec<TypeFact>),
    Unresolved,
}

/// Borrowed result for one internal named-fact selection.
///
/// This keeps hot indexing queries allocation-free without exposing compact
/// store ids or indexes as semantic API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NamedTypeResolution<'a> {
    Resolved(&'a RubyType),
    Ambiguous,
    Unresolved,
}
