use crate::core::{LiteralKey, RubyType, UnknownReason};
use std::collections::{BTreeMap, BTreeSet};

/// One flow-local abstract identity for a mutable Hash value.
///
/// The identity never leaves one TypeTracker pass. Engine facts retain only
/// the resulting canonical RubyType or an explicit UnknownReason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(in crate::inference::type_tracker) struct ShapeIdentity(
    pub(in crate::inference::type_tracker) u32,
);

/// One proven containment edge between two flow-local Hash identities.
///
/// The edge is deliberately inference-private. A parent Shape stores the
/// canonical projected child type, while this edge retains the Ruby object
/// identity needed to update that projection after a known child mutation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::inference::type_tracker) struct ShapeContainment {
    pub(in crate::inference::type_tracker) parent: ShapeIdentity,
    pub(in crate::inference::type_tracker) key: LiteralKey,
    pub(in crate::inference::type_tracker) child: ShapeIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::inference::type_tracker) struct ShapeAlternativeTransition {
    pub(in crate::inference::type_tracker) before: RubyType,
    pub(in crate::inference::type_tracker) after: RubyType,
}

/// Bounded positional identity evidence for one exact local Array value.
///
/// Only positions containing tracked Hash identities are retained. `length`
/// makes negative literal indices deterministic without allocating storage for
/// scalar-only positions, while `contained` supports whole-Array escape
/// invalidation.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(in crate::inference::type_tracker) struct ArrayShapeAliases {
    pub(in crate::inference::type_tracker) length: usize,
    pub(in crate::inference::type_tracker) positions: BTreeMap<usize, BTreeSet<ShapeIdentity>>,
    pub(in crate::inference::type_tracker) contained: BTreeSet<ShapeIdentity>,
    pub(in crate::inference::type_tracker) unknown_reason: Option<UnknownReason>,
}

impl ArrayShapeAliases {
    pub(in crate::inference::type_tracker) fn identities_at(
        &self,
        index: i32,
    ) -> BTreeSet<ShapeIdentity> {
        let position = if index >= 0 {
            usize::try_from(index).expect(
                "INVARIANT VIOLATED: a nonnegative i32 Array index did not fit usize. This is a bug because every supported Rust target can represent u32-sized collection positions. Fix: keep positional indices bounded to Prism's i32 conversion.",
            )
        } else {
            let from_end = usize::try_from(index.unsigned_abs()).expect(
                "INVARIANT VIOLATED: an i32 Array index magnitude did not fit usize. This is a bug because every supported Rust target can represent u32-sized collection positions. Fix: keep positional indices bounded to Prism's i32 conversion.",
            );
            let Some(position) = self.length.checked_sub(from_end) else {
                return BTreeSet::new();
            };
            position
        };
        if position >= self.length {
            return BTreeSet::new();
        }
        self.positions.get(&position).cloned().unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::inference::type_tracker) enum ShapeIdentityState {
    Proven(RubyType),
    Invalidated(UnknownReason),
}
