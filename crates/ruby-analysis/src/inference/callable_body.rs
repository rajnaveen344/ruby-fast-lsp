//! Evaluation of canonical callable-body summaries from proven call inputs.
//!
//! The evaluator is parser-free and workspace-free. Method dispatch is
//! supplied by the caller and must delegate to `AnalysisQuery`; shape reads
//! reuse the canonical shape algebra directly.

use crate::core::{
    CallableBodyExpression, CallableBodyParameterKind, CallableBodySummary, LiteralKey, RubyMethod,
    RubyType, ShapeExactness, ShapeField, ShapeStability, ShapeType, TypeInferenceOutcome,
    UnknownReason,
};
use crate::inference::r#type::shape as shape_reads;

#[derive(Default)]
struct CallableEvaluationBudget {
    solve_iterations: usize,
}

impl CallableEvaluationBudget {
    fn consume_call_constraint(&mut self) -> Result<(), UnknownReason> {
        self.solve_iterations = self
            .solve_iterations
            .checked_add(1)
            .ok_or(UnknownReason::CallableBodyBoundExceeded)?;
        if self.solve_iterations > crate::core::callable_body::MAX_CALLABLE_BODY_SOLVE_ITERATIONS {
            return Err(UnknownReason::CallableBodyBoundExceeded);
        }
        Ok(())
    }
}

pub(crate) fn instantiate_callable_body(
    summary: &CallableBodySummary,
    arguments: &[RubyType],
    mut capture_type: impl FnMut(&str) -> Option<RubyType>,
    mut resolve_callable_capture: impl FnMut(&str, &[RubyType]) -> Option<TypeInferenceOutcome>,
    mut resolve_method: impl FnMut(&RubyType, &RubyMethod, &[RubyType]) -> TypeInferenceOutcome,
) -> TypeInferenceOutcome {
    if let Err(reason) = summary.validate() {
        return TypeInferenceOutcome::unknown(reason);
    }
    if arguments.iter().any(RubyType::contains_unknown) {
        return TypeInferenceOutcome::unknown(UnknownReason::IncompleteCallableInput);
    }

    let mut captures = Vec::with_capacity(summary.captures.len());
    for name in &summary.captures {
        let Some(ruby_type) = capture_type(name) else {
            return TypeInferenceOutcome::unknown(UnknownReason::IncompleteCallableCapture);
        };
        if RubyType::contains_unknown(&ruby_type) {
            return TypeInferenceOutcome::unknown(UnknownReason::IncompleteCallableCapture);
        }
        captures.push(ruby_type);
    }

    let mut parameters = Vec::with_capacity(summary.parameters.len());
    let mut budget = CallableEvaluationBudget::default();
    let mut argument_index = 0usize;
    let required = summary
        .parameters
        .iter()
        .filter(|parameter| parameter.kind == CallableBodyParameterKind::Required)
        .count();
    let optional = summary
        .parameters
        .iter()
        .filter(|parameter| parameter.kind == CallableBodyParameterKind::Optional)
        .count();
    let has_rest = summary
        .parameters
        .iter()
        .any(|parameter| parameter.kind == CallableBodyParameterKind::Rest);
    if summary.strict_arity
        && (arguments.len() < required
            || (!has_rest && arguments.len() > required.saturating_add(optional)))
    {
        return TypeInferenceOutcome::unknown(UnknownReason::IncompleteCallableInput);
    }

    for parameter in &summary.parameters {
        match parameter.kind {
            CallableBodyParameterKind::Required => {
                parameters.push(
                    arguments
                        .get(argument_index)
                        .cloned()
                        .unwrap_or_else(RubyType::nil_class),
                );
                argument_index = argument_index.saturating_add(1);
            }
            CallableBodyParameterKind::Optional => {
                if let Some(argument) = arguments.get(argument_index) {
                    parameters.push(argument.clone());
                    argument_index = argument_index.saturating_add(1);
                } else if let Some(default) = &parameter.default {
                    match evaluate_expression(
                        default,
                        &parameters,
                        &captures,
                        summary,
                        0,
                        &mut budget,
                        &mut resolve_callable_capture,
                        &mut resolve_method,
                    ) {
                        Ok(ruby_type) => parameters.push(ruby_type),
                        Err(reason) => return TypeInferenceOutcome::unknown(reason),
                    }
                } else {
                    parameters.push(RubyType::nil_class());
                }
            }
            CallableBodyParameterKind::Rest => {
                let remaining = arguments[argument_index.min(arguments.len())..].to_vec();
                parameters.push(RubyType::Array(RubyType::canonical_union_members(
                    remaining,
                )));
                argument_index = arguments.len();
            }
        }
    }

    match evaluate_expression(
        &summary.result,
        &parameters,
        &captures,
        summary,
        0,
        &mut budget,
        &mut resolve_callable_capture,
        &mut resolve_method,
    ) {
        Ok(ruby_type)
            if !RubyType::contains_unknown(&ruby_type)
                && !exceeds_callable_union_bound(&ruby_type)
                && !exceeds_callable_type_depth(&ruby_type, 0) =>
        {
            TypeInferenceOutcome::proven(ruby_type)
        }
        Ok(ruby_type)
            if exceeds_callable_union_bound(&ruby_type)
                || exceeds_callable_type_depth(&ruby_type, 0) =>
        {
            TypeInferenceOutcome::unknown(UnknownReason::CallableBodyBoundExceeded)
        }
        Ok(_) => TypeInferenceOutcome::unknown(UnknownReason::IncompleteCallableInput),
        Err(reason) => TypeInferenceOutcome::unknown(reason),
    }
}

fn exceeds_callable_type_depth(ruby_type: &RubyType, depth: usize) -> bool {
    if depth > crate::core::callable_body::MAX_CALLABLE_BODY_TYPE_DEPTH {
        return true;
    }
    match ruby_type {
        RubyType::Array(members) | RubyType::Union(members) => members
            .iter()
            .any(|member| exceeds_callable_type_depth(member, depth + 1)),
        RubyType::Hash(keys, values) => keys
            .iter()
            .chain(values.iter())
            .any(|member| exceeds_callable_type_depth(member, depth + 1)),
        RubyType::Shape(shape) => {
            shape
                .fields()
                .iter()
                .any(|field| exceeds_callable_type_depth(field.value(), depth + 1))
                || shape.rest().is_some_and(|rest| {
                    exceeds_callable_type_depth(rest.key(), depth + 1)
                        || exceeds_callable_type_depth(rest.value(), depth + 1)
                })
        }
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Literal(_)
        | RubyType::Unknown => false,
    }
}

fn exceeds_callable_union_bound(ruby_type: &RubyType) -> bool {
    match ruby_type {
        RubyType::Union(members) => {
            members.len() > crate::core::callable_body::MAX_CALLABLE_BODY_UNION_VARIANTS
                || members.iter().any(exceeds_callable_union_bound)
        }
        RubyType::Array(members) => members.iter().any(exceeds_callable_union_bound),
        RubyType::Hash(keys, values) => keys
            .iter()
            .chain(values.iter())
            .any(exceeds_callable_union_bound),
        RubyType::Shape(shape) => {
            shape
                .fields()
                .iter()
                .any(|field| exceeds_callable_union_bound(field.value()))
                || shape.rest().is_some_and(|rest| {
                    exceeds_callable_union_bound(rest.key())
                        || exceeds_callable_union_bound(rest.value())
                })
        }
        RubyType::Class(_)
        | RubyType::Module(_)
        | RubyType::ClassReference(_)
        | RubyType::ModuleReference(_)
        | RubyType::Literal(_)
        | RubyType::Unknown => false,
    }
}

fn evaluate_expression(
    expression: &CallableBodyExpression,
    parameters: &[RubyType],
    captures: &[RubyType],
    summary: &CallableBodySummary,
    depth: usize,
    budget: &mut CallableEvaluationBudget,
    resolve_callable_capture: &mut impl FnMut(&str, &[RubyType]) -> Option<TypeInferenceOutcome>,
    resolve_method: &mut impl FnMut(&RubyType, &RubyMethod, &[RubyType]) -> TypeInferenceOutcome,
) -> Result<RubyType, UnknownReason> {
    if depth > crate::core::callable_body::MAX_CALLABLE_BODY_TYPE_DEPTH {
        return Err(UnknownReason::CallableBodyBoundExceeded);
    }
    match expression {
        CallableBodyExpression::Literal(ruby_type) => {
            if RubyType::contains_unknown(ruby_type) {
                Err(UnknownReason::UnsupportedCallableBody)
            } else {
                Ok(ruby_type.clone())
            }
        }
        CallableBodyExpression::Parameter(index) => parameters
            .get(*index)
            .cloned()
            .ok_or(UnknownReason::IncompleteCallableInput),
        CallableBodyExpression::Capture(name) => {
            let index = summary
                .captures
                .binary_search(name)
                .map_err(|_| UnknownReason::IncompleteCallableCapture)?;
            captures
                .get(index)
                .cloned()
                .ok_or(UnknownReason::IncompleteCallableCapture)
        }
        CallableBodyExpression::Array(values) => {
            let mut elements = Vec::with_capacity(values.len());
            for value in values {
                elements.push(evaluate_expression(
                    value,
                    parameters,
                    captures,
                    summary,
                    depth + 1,
                    budget,
                    resolve_callable_capture,
                    resolve_method,
                )?);
            }
            Ok(RubyType::Array(RubyType::canonical_union_members(elements)))
        }
        CallableBodyExpression::Shape(fields) => {
            let mut resolved = Vec::with_capacity(fields.len());
            for (key, value) in fields {
                resolved.push(ShapeField::required(
                    key.clone(),
                    evaluate_expression(
                        value,
                        parameters,
                        captures,
                        summary,
                        depth + 1,
                        budget,
                        resolve_callable_capture,
                        resolve_method,
                    )?,
                ));
            }
            ShapeType::try_new(
                resolved,
                None,
                ShapeExactness::Exact,
                ShapeStability::TrackedMutable,
            )
            .map(|shape| RubyType::Shape(Box::new(shape)))
            .map_err(|_| UnknownReason::CallableBodyBoundExceeded)
        }
        CallableBodyExpression::ExhaustiveUnion(values) => {
            if values.len() > crate::core::callable_body::MAX_CALLABLE_BODY_UNION_VARIANTS {
                return Err(UnknownReason::CallableBodyBoundExceeded);
            }
            let mut members = Vec::with_capacity(values.len());
            for value in values {
                members.push(evaluate_expression(
                    value,
                    parameters,
                    captures,
                    summary,
                    depth + 1,
                    budget,
                    resolve_callable_capture,
                    resolve_method,
                )?);
            }
            let result = RubyType::union(members);
            if RubyType::contains_unknown(&result) {
                Err(UnknownReason::IncompleteCallableInput)
            } else {
                Ok(result)
            }
        }
        CallableBodyExpression::Call {
            receiver,
            method,
            arguments,
            literal_argument_keys,
        } => {
            budget.consume_call_constraint()?;
            let mut argument_types = Vec::with_capacity(arguments.len());
            for argument in arguments {
                argument_types.push(evaluate_expression(
                    argument,
                    parameters,
                    captures,
                    summary,
                    depth + 1,
                    budget,
                    resolve_callable_capture,
                    resolve_method,
                )?);
            }
            if method.as_str() == "call" {
                if let CallableBodyExpression::Capture(name) = receiver.as_ref() {
                    if let Some(outcome) = resolve_callable_capture(name, &argument_types) {
                        let reason = outcome.unknown_reason();
                        return outcome.into_proven_type().ok_or_else(|| {
                            reason.unwrap_or(UnknownReason::IncompleteCallableCapture)
                        });
                    }
                }
            }
            let receiver = evaluate_expression(
                receiver,
                parameters,
                captures,
                summary,
                depth + 1,
                budget,
                resolve_callable_capture,
                resolve_method,
            )?;
            if RubyType::contains_unknown(&receiver)
                || argument_types.iter().any(RubyType::contains_unknown)
            {
                return Err(UnknownReason::IncompleteCallableInput);
            }
            if shape_reads::is_shape_only(&receiver) {
                if let Some(result) =
                    evaluate_shape_call(&receiver, method, &argument_types, literal_argument_keys)
                {
                    return result;
                }
            }
            if method.as_str() == "freeze" && argument_types.is_empty() {
                return Ok(receiver);
            }
            resolve_method(&receiver, method, &argument_types)
                .into_proven_type()
                .ok_or(UnknownReason::UnresolvedMethodReturn)
        }
    }
}

fn evaluate_shape_call(
    receiver: &RubyType,
    method: &RubyMethod,
    argument_types: &[RubyType],
    literal_argument_keys: &[Option<LiteralKey>],
) -> Option<Result<RubyType, UnknownReason>> {
    match method.as_str() {
        "[]" if argument_types.len() == 1 => Some(shape_reads::indexed_read(
            receiver,
            literal_argument_keys.first().and_then(Option::as_ref),
        )),
        "fetch" if matches!(argument_types.len(), 1 | 2) => Some(shape_reads::fetch(
            receiver,
            literal_argument_keys.first().and_then(Option::as_ref),
            argument_types.get(1),
        )),
        "dig" if !argument_types.is_empty() => {
            Some(shape_reads::dig(receiver, literal_argument_keys))
        }
        "key?" | "has_key?" | "include?" | "member?" if argument_types.len() == 1 => {
            Some(shape_reads::key_presence(
                receiver,
                literal_argument_keys.first().and_then(Option::as_ref),
            ))
        }
        "keys" if argument_types.is_empty() => Some(shape_reads::keys(receiver)),
        "values" if argument_types.is_empty() => Some(shape_reads::values(receiver)),
        _ => None,
    }
}
