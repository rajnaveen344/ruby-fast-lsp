//! Compact arena ids and stored fact records behind the type store.

use super::{TextRange, TypeProvenance};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StoredTypeFact {
    pub(super) subject: StoredTypeSubject,
    pub(super) ruby_type: RubyTypeId,
    pub(super) range: TextRange,
    pub(super) provenance: TypeProvenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) struct StoredTypeSubject(u32);

impl StoredTypeSubject {
    const EXPRESSION_TAG: u32 = 1 << 31;

    pub(super) fn interned(id: TypeSubjectId) -> Self {
        assert!(
            id.0 < Self::EXPRESSION_TAG,
            "INVARIANT VIOLATED: the non-expression type subject interner exceeded the compact 31-bit id space. This is a bug because the high bit distinguishes range-owned expression facts. Fix: widen StoredTypeSubject and every stored subject reference together before interning 2^31 subjects."
        );
        Self(id.0)
    }

    pub(super) fn expression() -> Self {
        Self(Self::EXPRESSION_TAG)
    }

    pub(super) fn interned_id(self) -> Option<TypeSubjectId> {
        if self.0 == Self::EXPRESSION_TAG {
            None
        } else {
            assert!(
                self.0 < Self::EXPRESSION_TAG,
                "INVARIANT VIOLATED: stored type subject has an unknown compact tag. This is a bug because only interned ids and the expression tag are valid. Fix: construct stored subjects through StoredTypeSubject::interned or StoredTypeSubject::expression."
            );
            Some(TypeSubjectId(self.0))
        }
    }

    pub(super) fn is_expression(self) -> bool {
        self.interned_id().is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) struct TypeFactId(u32);

impl TypeFactId {
    pub(super) fn from_index(index: usize) -> Self {
        Self(u32::try_from(index).expect(
            "INVARIANT VIOLATED: type fact arena exceeded u32 ids. This is a bug because the \
             retained type indexes use bounded compact ids. Fix: widen TypeFactId and every \
             stored type-fact index together before retaining more than u32::MAX facts.",
        ))
    }

    pub(super) fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(super) struct TypeSubjectId(u32);

impl TypeSubjectId {
    pub(super) fn from_index(index: usize) -> Self {
        Self(u32::try_from(index).expect(
            "INVARIANT VIOLATED: type subject interner exceeded u32 ids. This is a bug because \
             every stored type fact refers to a compact subject id. Fix: widen TypeSubjectId \
             and every stored subject reference together before interning more than u32::MAX \
             subjects.",
        ))
    }

    pub(super) fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct RubyTypeId(u32);

impl RubyTypeId {
    pub(super) fn from_index(index: usize) -> Self {
        Self(u32::try_from(index).expect(
            "INVARIANT VIOLATED: Ruby type interner exceeded u32 ids. This is a bug because \
             every stored type fact refers to a compact Ruby type id. Fix: widen RubyTypeId \
             and every stored Ruby type reference together before interning more than \
             u32::MAX distinct types.",
        ))
    }

    pub(super) fn index(self) -> usize {
        self.0 as usize
    }
}
