#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub(crate) struct FqnId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub(crate) struct ConstLookupId(pub u32);

/// An optional [`FqnId`] in four bytes. `u32::MAX` marks absence; the name
/// interner never issues it because ids are dense insertion indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct OptionalFqnId(u32);

impl OptionalFqnId {
    pub(crate) const NONE: Self = Self(u32::MAX);

    pub(crate) fn new(id: Option<FqnId>) -> Self {
        match id {
            Some(id) => {
                invariant!(
                    id.0 != u32::MAX,
                    what = "an interned FQN id collides with the absent sentinel",
                    why = "OptionalFqnId reserves u32::MAX for None",
                    fix = "cap the name interner below u32::MAX entries",
                );
                Self(id.0)
            }
            None => Self::NONE,
        }
    }

    pub(crate) fn get(self) -> Option<FqnId> {
        (self.0 != u32::MAX).then_some(FqnId(self.0))
    }
}

impl From<Option<FqnId>> for OptionalFqnId {
    fn from(id: Option<FqnId>) -> Self {
        Self::new(id)
    }
}
