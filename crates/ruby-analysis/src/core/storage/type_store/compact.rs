//! Compact arena ids and stored fact records behind the type store.

use super::{TextRange, TypeProvenance};
use crate::invariant::ExpectInvariant;

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
        invariant!(
            id.0 < Self::EXPRESSION_TAG,
            what = "type subject interner exceeded the 31-bit id space",
            why = "the high bit marks range-owned expression facts",
            fix = "widen StoredTypeSubject and its references together",
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
            invariant!(
                self.0 < Self::EXPRESSION_TAG,
                what = "stored type subject has an unknown compact tag",
                why = "only interned ids and the expression tag are valid",
                fix = "construct subjects via StoredTypeSubject::interned or ::expression",
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
        Self(u32::try_from(index).expect_invariant(
            "type fact arena exceeded u32 ids",
            "type indexes use compact u32 ids",
            "widen TypeFactId and every stored type-fact index together",
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
        Self(u32::try_from(index).expect_invariant(
            "type subject interner exceeded u32 ids",
            "type facts refer to compact subject ids",
            "widen TypeSubjectId and every subject reference together",
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
        Self(u32::try_from(index).expect_invariant(
            "Ruby type interner exceeded u32 ids",
            "type facts refer to compact Ruby type ids",
            "widen RubyTypeId and every Ruby type reference together",
        ))
    }

    pub(super) fn index(self) -> usize {
        self.0 as usize
    }
}
