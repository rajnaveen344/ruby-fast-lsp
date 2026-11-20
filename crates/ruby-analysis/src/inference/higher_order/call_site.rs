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

/// Whether a block call that could not be prepared must stay unknown. The
/// plain result lookup reads the overload without a block, so its answer is
/// wrong whenever the block selects another one on any possible receiver.
pub(crate) fn unprepared_block_result_is_unknown(
    receiver_type: Option<&RubyType>,
    method_name: &str,
) -> bool {
    let class_name = match receiver_type {
        Some(RubyType::Union(members)) => {
            return members
                .iter()
                .any(|member| unprepared_block_result_is_unknown(Some(member), method_name));
        }
        Some(RubyType::Array(_)) => "Array".to_string(),
        Some(RubyType::Hash(_, _) | RubyType::Shape(_)) => "Hash".to_string(),
        Some(RubyType::Class(fqn)) => fqn
            .namespace_parts()
            .iter()
            .map(|part| part.to_string())
            .collect::<Vec<_>>()
            .join("::"),
        Some(
            RubyType::ClassReference(_)
            | RubyType::Module(_)
            | RubyType::ModuleReference(_)
            | RubyType::Literal(_)
            | RubyType::Unknown,
        )
        | None => return false,
    };
    crate::inference::rbs::block_selects_another_result(&class_name, method_name)
}

/// The documented yield types a block's parameters may take, by position.
///
/// Documentation proves a type only for a plain required parameter that
/// receives exactly one documented value. A block that also declares
/// optional, rest, post, or destructuring parameters, or more parameters than
/// the method documents, distributes the yielded values in a way the
/// documentation does not describe, so nothing is bound.
pub(crate) fn documented_block_parameter_types(
    documented: &[RubyType],
    block: &BlockNode<'_>,
) -> Vec<RubyType> {
    let Some(parameters_node) = block.parameters() else {
        return Vec::new();
    };
    let plain_parameter_count =
        if let Some(numbered) = parameters_node.as_numbered_parameters_node() {
            usize::from(numbered.maximum())
        } else {
            let Some(parameters) = parameters_node
                .as_block_parameters_node()
                .and_then(|node| node.parameters())
            else {
                return Vec::new();
            };
            let only_plain_required_parameters = parameters
                .requireds()
                .iter()
                .all(|required| required.as_required_parameter_node().is_some())
                && parameters.optionals().iter().next().is_none()
                && parameters.rest().is_none()
                && parameters.posts().iter().next().is_none()
                && parameters.keywords().iter().next().is_none()
                && parameters.keyword_rest().is_none();
            if !only_plain_required_parameters {
                return Vec::new();
            }
            parameters.requireds().iter().count()
        };
    if plain_parameter_count == 0 || plain_parameter_count > documented.len() {
        return Vec::new();
    }
    documented[..plain_parameter_count].to_vec()
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
