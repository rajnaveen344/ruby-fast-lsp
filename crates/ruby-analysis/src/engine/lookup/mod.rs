//! One entry point per semantic lookup, answered with an explicit outcome.
//!
//! A lookup states its receiver, member, access, and wanted product in one
//! request and returns a [`MethodAnswer`]. The answer separates three kinds of
//! "no result" that the access-flavoured wrappers used to collapse into `None`
//! or an empty `Vec`:
//!
//! - [`MethodAnswer::Ambiguous`]: several definitions may win.
//! - [`MethodAnswer::Missing`]: every edge that could supply the member is
//!   known, and none does.
//! - [`MethodAnswer::Unknown`]: an edge is unknown, so absence is unproven.
//!
//! Unknown lookup edges suppress missing-method claims. A caller that would
//! report or act on absence must do so only for `Missing`, and must fail
//! closed on `Unknown`.
//!
//! Requests delegate to the engine's existing resolution functions; equality
//! with the legacy `View` wrappers is tested per want, access, and receiver.

mod method;
#[cfg(test)]
mod tests;

pub use method::{method, method_cached, LookupReceiver, MethodFound, MethodRequest, MethodWant};

use crate::core::{FullyQualifiedName, RubyMethod};

/// The outcome of one method lookup.
///
/// `T` is the found payload; `U` is why absence is unproven. Lookups that can
/// never meet an unknown edge use [`std::convert::Infallible`] for `U`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MethodAnswer<T = MethodFound, U = LookupUnknown> {
    /// A proven winner.
    Found(T),
    /// More than one definition may win at `owner`.
    Ambiguous {
        owner: FullyQualifiedName,
        method: RubyMethod,
    },
    /// Every lookup edge is known and none supplies the method.
    Missing,
    /// An edge is unknown; absence is not proven.
    Unknown(U),
}

/// Why a lookup cannot prove absence. Unknown is an expected analysis
/// outcome, never corrupt state, and never evidence that a member is missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LookupUnknown {
    /// The receiver has no indexed namespace, or a receiver-type member has no
    /// proven namespace or winner.
    Receiver,
    /// An unresolved ancestor edge precedes any winner.
    IncompleteChain,
    /// The lookup found no proven product, such as a return type or a
    /// signature; that is not evidence the method is absent.
    NoEvidence,
    /// The wanted product is not defined for this receiver or access.
    Unsupported,
}

impl<T, U> MethodAnswer<T, U> {
    /// The found payload transformed by `f`; other outcomes are unchanged.
    pub fn map_found<V>(self, f: impl FnOnce(T) -> V) -> MethodAnswer<V, U> {
        match self {
            MethodAnswer::Found(found) => MethodAnswer::Found(f(found)),
            MethodAnswer::Ambiguous { owner, method } => MethodAnswer::Ambiguous { owner, method },
            MethodAnswer::Missing => MethodAnswer::Missing,
            MethodAnswer::Unknown(reason) => MethodAnswer::Unknown(reason),
        }
    }

    /// Whether the lookup selected no definition, either because the method
    /// is proven missing or because absence is unknown. Callers that only
    /// need a target treat both alike; absence claims must use
    /// [`Self::is_missing`].
    pub fn has_no_target(&self) -> bool {
        match self {
            MethodAnswer::Missing | MethodAnswer::Unknown(_) => true,
            MethodAnswer::Found(_) | MethodAnswer::Ambiguous { .. } => false,
        }
    }

    /// Whether absence is proven.
    pub fn is_missing(&self) -> bool {
        match self {
            MethodAnswer::Missing => true,
            MethodAnswer::Found(_) | MethodAnswer::Ambiguous { .. } | MethodAnswer::Unknown(_) => {
                false
            }
        }
    }
}
