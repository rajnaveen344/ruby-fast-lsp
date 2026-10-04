//! A higher-order call site, shared by the collector walk and `TypeTracker`.
//!
//! Both walks prepare `receiver.method(args) { ... }` the same way and finish
//! a `&expression` block argument the same way; each passes its own argument
//! typing, capture lookup, constant callables, and method resolution.

use crate::core::callables::callable_body::CallableBodySummary;
use crate::core::{FullyQualifiedName, RubyMethod, RubyType, TypeInferenceOutcome, UnknownReason};
use crate::inference::higher_order::{KnownProcType, LocalCallables, PreparedCallableSet};
use crate::inference::semantics::Semantics;
use ruby_prism::*;

/// Select the signatures a block call may use. An unproven receiver cannot
/// select one, so arguments are typed only after the receiver is proven.
pub(crate) fn prepare_call(
    project: Option<&dyn Semantics>,
    receiver_type: Option<&RubyType>,
    implicit_namespace: &FullyQualifiedName,
    method_name: &str,
    argument_types: impl FnOnce() -> Vec<RubyType>,
) -> Result<PreparedCallableSet, UnknownReason> {
    if receiver_type.is_some_and(|receiver| {
        receiver == &RubyType::Unknown || RubyType::contains_unknown(receiver)
    }) {
        return Err(UnknownReason::IncompleteBlockInput);
    }
    let argument_types = argument_types();
    match project {
        Some(project) => project.prepare_higher_order_call(
            receiver_type,
            implicit_namespace,
            method_name,
            &argument_types,
        ),
        None => crate::inference::rbs::prepare_higher_order_call_with_fallbacks(
            None::<&dyn Semantics>,
            receiver_type,
            None,
            method_name,
            &argument_types,
        ),
    }
}

impl PreparedCallableSet {
    /// The block's positional parameters bound to the yielded types; a
    /// parameter the signature does not yield is nil.
    pub(crate) fn block_parameter_bindings(&self, names: &[String]) -> Vec<(String, RubyType)> {
        names
            .iter()
            .enumerate()
            .map(|(index, name)| {
                let parameter_type = self
                    .block_parameter_types()
                    .get(index)
                    .cloned()
                    .unwrap_or_else(RubyType::nil_class);
                (name.clone(), parameter_type)
            })
            .collect()
    }
}

impl LocalCallables {
    /// The callable `expression` names: a local bound to a proc literal, or a
    /// constant holding one. `None` when it names neither.
    pub(crate) fn callable_for(
        &self,
        expression: &Node<'_>,
        constant_callable: impl FnOnce(&Node<'_>) -> Option<Result<CallableBodySummary, UnknownReason>>,
    ) -> Option<Result<KnownProcType, UnknownReason>> {
        if let Some(local) = expression.as_local_variable_read_node() {
            let name = String::from_utf8_lossy(local.name().as_slice());
            return self.get(name.as_ref()).cloned().map(Ok);
        }
        constant_callable(expression).map(|result| result.map(KnownProcType::constant))
    }

    /// Finish a call whose block is `&expression`: a static `&:method`, or a
    /// known callable. `None` only for a symbol that is not valid UTF-8.
    pub(crate) fn finish_block_argument_node(
        &self,
        prepared: PreparedCallableSet,
        block_argument: &BlockArgumentNode<'_>,
        constant_callable: impl FnOnce(&Node<'_>) -> Option<Result<CallableBodySummary, UnknownReason>>,
        capture_type: &dyn Fn(&str) -> Option<RubyType>,
        resolve_method: &dyn Fn(&RubyType, &str) -> TypeInferenceOutcome,
    ) -> Option<TypeInferenceOutcome> {
        let Some(expression) = block_argument.expression() else {
            return Some(TypeInferenceOutcome::unknown(
                UnknownReason::UnsupportedCallable,
            ));
        };
        if let Some(symbol) = expression.as_symbol_node() {
            let target = std::str::from_utf8(symbol.unescaped()).ok()?;
            return Some(prepared.finish_static_method(target, resolve_method));
        }
        Some(match self.callable_for(&expression, constant_callable) {
            None => TypeInferenceOutcome::unknown(UnknownReason::UnsupportedCallable),
            Some(Err(reason)) => TypeInferenceOutcome::unknown(reason),
            Some(Ok(callable)) => self.finish_block_argument(
                prepared,
                &callable,
                capture_type,
                &|receiver: &RubyType, method: &RubyMethod| {
                    resolve_method(receiver, method.as_str())
                },
            ),
        })
    }
}
