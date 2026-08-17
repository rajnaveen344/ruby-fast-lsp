//! Canonical, file-owned templates for block-bearing method signatures.
//!
//! These values are declaration evidence, not inferred runtime types. They may
//! be retained on an ordinary method fact, while only a fully substituted
//! [`RubyType`] may escape the higher-order inference proof boundary.

use super::{MethodParamKind, RubyMethod, RubyType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ForwardedBlockCall {
    pub(crate) receiver_parameter: String,
    pub(crate) method: RubyMethod,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DirectYieldCall {
    pub(crate) parameter_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CallableTypeTemplate {
    Concrete(RubyType),
    /// The concrete runtime receiver of this call (`self` in RBS).
    Receiver,
    Variable(String),
    Array(Box<CallableTypeTemplate>),
    Hash(Box<CallableTypeTemplate>, Box<CallableTypeTemplate>),
    Union(Vec<CallableTypeTemplate>),
    /// A declared `untyped`/truthiness result that neither binds a variable
    /// nor contributes to the solved call result.
    Unconstrained,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CallableParameterTemplate {
    pub(crate) kind: MethodParamKind,
    pub(crate) ruby_type: CallableTypeTemplate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CallableBlockTemplate {
    pub(crate) parameters: Vec<CallableTypeTemplate>,
    pub(crate) return_type: CallableTypeTemplate,
    pub(crate) required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CallableSignature {
    /// The prefix of `type_parameters` bound by the receiver's declared
    /// generic arguments. Method-local variables are solved from arguments or
    /// the block result instead.
    pub(crate) receiver_type_parameters: Vec<String>,
    pub(crate) type_parameters: Vec<String>,
    pub(crate) parameters: Vec<CallableParameterTemplate>,
    pub(crate) block: CallableBlockTemplate,
    pub(crate) return_type: CallableTypeTemplate,
}
