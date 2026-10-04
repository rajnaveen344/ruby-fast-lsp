//! Snapshot encoding and restoration of callable bodies, signatures, and templates.

use super::fact_codec::{restore_param_kind, snapshot_param_kind};
use super::type_codec::{
    restore_fqn, restore_literal_key, restore_range, restore_ruby_type, snapshot_fqn,
    snapshot_literal_key, snapshot_range, snapshot_ruby_type,
};
use super::{
    SnapshotCallableBlockTemplate, SnapshotCallableBodyExpression, SnapshotCallableBodyParameter,
    SnapshotCallableBodyParameterKind, SnapshotCallableBodySummary,
    SnapshotCallableParameterTemplate, SnapshotCallableSignature, SnapshotCallableTypeTemplate,
    SnapshotConstantCallableBodyFact,
};
use crate::core::callables::callable_body::CallableBodyExpression;
use crate::core::callables::callable_body::CallableBodyParameter;
use crate::core::callables::callable_body::CallableBodyParameterKind;
use crate::core::callables::callable_body::CallableBodySummary;
use crate::core::callables::callable_body::ConstantCallableBodyFact;
use crate::core::{RubyMethod, SourceFileId};
use ustr::Ustr;

pub(super) fn snapshot_constant_callable_body(
    fact: &ConstantCallableBodyFact,
) -> Result<SnapshotConstantCallableBodyFact, String> {
    if !fact.summary.is_capture_free() {
        return Err("project-neutral callable body unexpectedly retains captures".to_string());
    }
    Ok(SnapshotConstantCallableBodyFact {
        constant: snapshot_fqn(&fact.constant)?,
        summary: snapshot_callable_body_summary(&fact.summary)?,
        range: snapshot_range(fact.range),
    })
}

pub(super) fn restore_constant_callable_body(
    fact: SnapshotConstantCallableBodyFact,
    file_id: SourceFileId,
) -> Result<ConstantCallableBodyFact, String> {
    let summary = restore_callable_body_summary(fact.summary)?;
    if !summary.is_capture_free() {
        return Err("persistent project-neutral callable body contains captures".to_string());
    }
    Ok(ConstantCallableBodyFact {
        constant: restore_fqn(fact.constant)?,
        summary,
        range: restore_range(fact.range, file_id)?,
    })
}

fn snapshot_callable_body_summary(
    summary: &CallableBodySummary,
) -> Result<SnapshotCallableBodySummary, String> {
    summary
        .validate()
        .map_err(|reason| format!("invalid callable body summary: {}", reason.code()))?;
    Ok(SnapshotCallableBodySummary {
        strict_arity: summary.strict_arity,
        parameters: summary
            .parameters
            .iter()
            .map(|parameter| {
                Ok(SnapshotCallableBodyParameter {
                    name: parameter.name.clone(),
                    kind: match parameter.kind {
                        CallableBodyParameterKind::Required => {
                            SnapshotCallableBodyParameterKind::Required
                        }
                        CallableBodyParameterKind::Optional => {
                            SnapshotCallableBodyParameterKind::Optional
                        }
                        CallableBodyParameterKind::Rest => SnapshotCallableBodyParameterKind::Rest,
                    },
                    default: parameter
                        .default
                        .as_ref()
                        .map(snapshot_callable_body_expression)
                        .transpose()?,
                })
            })
            .collect::<Result<_, String>>()?,
        captures: summary.captures.clone(),
        result: snapshot_callable_body_expression(&summary.result)?,
        node_count: summary.node_count,
    })
}

fn restore_callable_body_summary(
    summary: SnapshotCallableBodySummary,
) -> Result<CallableBodySummary, String> {
    use crate::core::callables::callable_body::{
        MAX_CALLABLE_BODY_CAPTURES, MAX_CALLABLE_BODY_NODES, MAX_CALLABLE_BODY_PARAMETERS,
    };
    if summary.parameters.len() > MAX_CALLABLE_BODY_PARAMETERS
        || summary.captures.len() > MAX_CALLABLE_BODY_CAPTURES
        || usize::from(summary.node_count) > MAX_CALLABLE_BODY_NODES
    {
        return Err("persistent callable body exceeds a fixed proof bound".to_string());
    }
    if !summary.captures.is_empty() {
        return Err("persistent project-neutral callable body contains captures".to_string());
    }
    let summary = CallableBodySummary {
        strict_arity: summary.strict_arity,
        parameters: summary
            .parameters
            .into_iter()
            .map(|parameter| {
                Ok(CallableBodyParameter {
                    name: parameter.name,
                    kind: match parameter.kind {
                        SnapshotCallableBodyParameterKind::Required => {
                            CallableBodyParameterKind::Required
                        }
                        SnapshotCallableBodyParameterKind::Optional => {
                            CallableBodyParameterKind::Optional
                        }
                        SnapshotCallableBodyParameterKind::Rest => CallableBodyParameterKind::Rest,
                    },
                    default: parameter
                        .default
                        .map(|expression| restore_callable_body_expression(expression, 0))
                        .transpose()?,
                })
            })
            .collect::<Result<_, String>>()?,
        captures: summary.captures,
        result: restore_callable_body_expression(summary.result, 0)?,
        node_count: summary.node_count,
    };
    summary
        .validate()
        .map_err(|reason| format!("invalid persistent callable body: {}", reason.code()))?;
    Ok(summary)
}

fn snapshot_callable_body_expression(
    expression: &CallableBodyExpression,
) -> Result<SnapshotCallableBodyExpression, String> {
    Ok(match expression {
        CallableBodyExpression::Literal(ruby_type) => {
            SnapshotCallableBodyExpression::Literal(snapshot_ruby_type(ruby_type)?)
        }
        CallableBodyExpression::Parameter(index) => {
            SnapshotCallableBodyExpression::Parameter(*index)
        }
        CallableBodyExpression::Capture(name) => {
            SnapshotCallableBodyExpression::Capture(name.clone())
        }
        CallableBodyExpression::Array(values) => SnapshotCallableBodyExpression::Array(
            values
                .iter()
                .map(snapshot_callable_body_expression)
                .collect::<Result<_, _>>()?,
        ),
        CallableBodyExpression::Shape(fields) => SnapshotCallableBodyExpression::Shape(
            fields
                .iter()
                .map(|(key, value)| {
                    Ok((
                        snapshot_literal_key(key),
                        snapshot_callable_body_expression(value)?,
                    ))
                })
                .collect::<Result<_, String>>()?,
        ),
        CallableBodyExpression::Call {
            receiver,
            method,
            arguments,
            literal_argument_keys,
        } => SnapshotCallableBodyExpression::Call {
            receiver: Box::new(snapshot_callable_body_expression(receiver)?),
            method: method.as_str().to_string(),
            arguments: arguments
                .iter()
                .map(snapshot_callable_body_expression)
                .collect::<Result<_, _>>()?,
            literal_argument_keys: literal_argument_keys
                .iter()
                .map(|key| key.as_ref().map(snapshot_literal_key))
                .collect(),
        },
        CallableBodyExpression::ExhaustiveUnion(values) => {
            SnapshotCallableBodyExpression::ExhaustiveUnion(
                values
                    .iter()
                    .map(snapshot_callable_body_expression)
                    .collect::<Result<_, _>>()?,
            )
        }
    })
}

fn restore_callable_body_expression(
    expression: SnapshotCallableBodyExpression,
    depth: usize,
) -> Result<CallableBodyExpression, String> {
    use crate::core::callables::callable_body::{
        MAX_CALLABLE_BODY_TYPE_DEPTH, MAX_CALLABLE_BODY_UNION_VARIANTS,
    };
    if depth > MAX_CALLABLE_BODY_TYPE_DEPTH {
        return Err("persistent callable expression exceeds the fixed type depth".to_string());
    }
    Ok(match expression {
        SnapshotCallableBodyExpression::Literal(ruby_type) => {
            CallableBodyExpression::Literal(restore_ruby_type(ruby_type, 1)?)
        }
        SnapshotCallableBodyExpression::Parameter(index) => {
            CallableBodyExpression::Parameter(index)
        }
        SnapshotCallableBodyExpression::Capture(name) => CallableBodyExpression::Capture(name),
        SnapshotCallableBodyExpression::Array(values) => CallableBodyExpression::Array(
            values
                .into_iter()
                .map(|value| restore_callable_body_expression(value, depth + 1))
                .collect::<Result<_, _>>()?,
        ),
        SnapshotCallableBodyExpression::Shape(fields) => CallableBodyExpression::Shape(
            fields
                .into_iter()
                .map(|(key, value)| {
                    Ok((
                        restore_literal_key(key),
                        restore_callable_body_expression(value, depth + 1)?,
                    ))
                })
                .collect::<Result<_, String>>()?,
        ),
        SnapshotCallableBodyExpression::Call {
            receiver,
            method,
            arguments,
            literal_argument_keys,
        } => {
            if arguments.len() != literal_argument_keys.len() {
                return Err("persistent callable call has mismatched argument metadata".to_string());
            }
            CallableBodyExpression::Call {
                receiver: Box::new(restore_callable_body_expression(*receiver, depth + 1)?),
                method: RubyMethod::new(&method).map_err(|error| {
                    format!("invalid persistent callable method `{method}`: {error}")
                })?,
                arguments: arguments
                    .into_iter()
                    .map(|value| restore_callable_body_expression(value, depth + 1))
                    .collect::<Result<_, _>>()?,
                literal_argument_keys: literal_argument_keys
                    .into_iter()
                    .map(|key| key.map(restore_literal_key))
                    .collect(),
            }
        }
        SnapshotCallableBodyExpression::ExhaustiveUnion(values) => {
            if values.len() > MAX_CALLABLE_BODY_UNION_VARIANTS {
                return Err("persistent callable union exceeds the fixed variant bound".to_string());
            }
            CallableBodyExpression::ExhaustiveUnion(
                values
                    .into_iter()
                    .map(|value| restore_callable_body_expression(value, depth + 1))
                    .collect::<Result<_, _>>()?,
            )
        }
    })
}

pub(super) fn snapshot_callable_signature(
    signature: &crate::core::callables::callable_signature::CallableSignature,
) -> Result<SnapshotCallableSignature, String> {
    Ok(SnapshotCallableSignature {
        receiver_type_parameters: ustr_strings(&signature.receiver_type_parameters),
        type_parameters: ustr_strings(&signature.type_parameters),
        parameters: signature
            .parameters
            .iter()
            .map(|parameter| {
                Ok(SnapshotCallableParameterTemplate {
                    kind: snapshot_param_kind(parameter.kind),
                    ruby_type: snapshot_callable_template(&parameter.ruby_type)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
        block: SnapshotCallableBlockTemplate {
            parameters: signature
                .block
                .parameters
                .iter()
                .map(snapshot_callable_template)
                .collect::<Result<Vec<_>, _>>()?,
            return_type: snapshot_callable_template(&signature.block.return_type)?,
            required: signature.block.required,
        },
        return_type: snapshot_callable_template(&signature.return_type)?,
    })
}

fn snapshot_callable_template(
    template: &crate::core::callables::callable_signature::CallableTypeTemplate,
) -> Result<SnapshotCallableTypeTemplate, String> {
    use crate::core::callables::callable_signature::CallableTypeTemplate;
    Ok(match template {
        CallableTypeTemplate::Concrete(ruby_type) => {
            SnapshotCallableTypeTemplate::Concrete(snapshot_ruby_type(ruby_type)?)
        }
        CallableTypeTemplate::Receiver => SnapshotCallableTypeTemplate::Receiver,
        CallableTypeTemplate::Variable(name) => {
            SnapshotCallableTypeTemplate::Variable(name.to_string())
        }
        CallableTypeTemplate::Array(element) => {
            SnapshotCallableTypeTemplate::Array(Box::new(snapshot_callable_template(element)?))
        }
        CallableTypeTemplate::Hash(key, value) => SnapshotCallableTypeTemplate::Hash(
            Box::new(snapshot_callable_template(key)?),
            Box::new(snapshot_callable_template(value)?),
        ),
        CallableTypeTemplate::Union(members) => SnapshotCallableTypeTemplate::Union(
            members
                .iter()
                .map(snapshot_callable_template)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        CallableTypeTemplate::Unconstrained => SnapshotCallableTypeTemplate::Unconstrained,
    })
}

pub(super) fn restore_callable_signature(
    signature: SnapshotCallableSignature,
) -> Result<crate::core::callables::callable_signature::CallableSignature, String> {
    Ok(
        crate::core::callables::callable_signature::CallableSignature {
            receiver_type_parameters: interned_strings(&signature.receiver_type_parameters),
            type_parameters: interned_strings(&signature.type_parameters),
            parameters: signature
                .parameters
                .into_iter()
                .map(|parameter| {
                    Ok(
                        crate::core::callables::callable_signature::CallableParameterTemplate {
                            kind: restore_param_kind(parameter.kind),
                            ruby_type: restore_callable_template(parameter.ruby_type)?,
                        },
                    )
                })
                .collect::<Result<Vec<_>, String>>()?,
            block: crate::core::callables::callable_signature::CallableBlockTemplate {
                parameters: signature
                    .block
                    .parameters
                    .into_iter()
                    .map(restore_callable_template)
                    .collect::<Result<Vec<_>, _>>()?,
                return_type: restore_callable_template(signature.block.return_type)?,
                required: signature.block.required,
            },
            return_type: restore_callable_template(signature.return_type)?,
        },
    )
}

fn restore_callable_template(
    template: SnapshotCallableTypeTemplate,
) -> Result<crate::core::callables::callable_signature::CallableTypeTemplate, String> {
    use crate::core::callables::callable_signature::CallableTypeTemplate;
    Ok(match template {
        SnapshotCallableTypeTemplate::Concrete(ruby_type) => {
            CallableTypeTemplate::Concrete(restore_ruby_type(ruby_type, 1)?)
        }
        SnapshotCallableTypeTemplate::Receiver => CallableTypeTemplate::Receiver,
        SnapshotCallableTypeTemplate::Variable(name) => {
            CallableTypeTemplate::Variable(Ustr::from(&name))
        }
        SnapshotCallableTypeTemplate::Array(element) => {
            CallableTypeTemplate::Array(Box::new(restore_callable_template(*element)?))
        }
        SnapshotCallableTypeTemplate::Hash(key, value) => CallableTypeTemplate::Hash(
            Box::new(restore_callable_template(*key)?),
            Box::new(restore_callable_template(*value)?),
        ),
        SnapshotCallableTypeTemplate::Union(members) => CallableTypeTemplate::Union(
            members
                .into_iter()
                .map(restore_callable_template)
                .collect::<Result<Vec<_>, _>>()?,
        ),
        SnapshotCallableTypeTemplate::Unconstrained => CallableTypeTemplate::Unconstrained,
    })
}

pub(super) fn ustr_strings(names: &[Ustr]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

pub(super) fn interned_strings(names: &[String]) -> Vec<Ustr> {
    names.iter().map(|name| Ustr::from(name)).collect()
}
