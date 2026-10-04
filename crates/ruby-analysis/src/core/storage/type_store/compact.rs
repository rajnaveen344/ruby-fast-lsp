//! Compact arena ids and stored fact records behind the type store.

use super::{TextRange, TypeProvenance};
use crate::core::UnknownReason;
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
    /// Subject index marker: the subject has no fact.
    pub(super) const NONE: Self = Self(u32::MAX);
    /// Subject index marker: the subject's facts live in a bucket.
    pub(super) const MANY: Self = Self(u32::MAX - 1);

    pub(super) fn from_index(index: usize) -> Self {
        let id = u32::try_from(index).expect_invariant(
            "type fact arena exceeded u32 ids",
            "type indexes use compact u32 ids",
            "widen TypeFactId and every stored type-fact index together",
        );
        invariant!(
            id < Self::MANY.0,
            what = "type fact arena reached the subject index markers",
            why = "the two highest ids mark empty and multi-fact subjects",
            fix = "widen TypeFactId and every stored type-fact index together",
        );
        Self(id)
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

/// A proven Ruby type id or an Unknown reason in one word. The high bit marks
/// Unknown; the rest holds the type id or the reason's position in
/// `UnknownReason::ALL`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StoredOutcome(u32);

impl StoredOutcome {
    const UNKNOWN: u32 = 1 << 31;

    pub(crate) fn proven(ruby_type: RubyTypeId) -> Self {
        invariant!(
            ruby_type.0 < Self::UNKNOWN,
            what = "Ruby type id collides with the Unknown outcome tag",
            why = "stored outcomes keep the high bit for Unknown reasons",
            fix = "widen StoredOutcome before the Ruby type interner reaches 2^31 ids",
        );
        Self(ruby_type.0)
    }

    pub(crate) fn unknown(reason: UnknownReason) -> Self {
        Self(Self::UNKNOWN | reason as u32)
    }

    pub(crate) fn get(self) -> Result<RubyTypeId, UnknownReason> {
        if self.0 & Self::UNKNOWN == 0 {
            Ok(RubyTypeId(self.0))
        } else {
            Err(UnknownReason::ALL[(self.0 & !Self::UNKNOWN) as usize])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_outcomes_round_trip_types_and_every_unknown_reason() {
        let ruby_type = RubyTypeId::from_index(41);
        assert_eq!(StoredOutcome::proven(ruby_type).get(), Ok(ruby_type));
        for reason in UnknownReason::ALL {
            assert_eq!(StoredOutcome::unknown(reason).get(), Err(reason));
        }
    }
}
