//! Canonical AST-free summaries for statically visible Ruby callable bodies.
//!
//! These values contain only bounded semantic constraints. Prism nodes,
//! source slices, workspace ownership, and editor protocol values are
//! deliberately excluded so the same summary can be evaluated for a direct
//! call, a higher-order call, or a file-owned constant fact.

use std::collections::BTreeSet;
use std::mem::size_of;

use super::memory_estimate::{fqn_heap_bytes, ruby_type_heap_bytes};
use super::{FullyQualifiedName, LiteralKey, RubyMethod, RubyType, TextRange};

pub(crate) const MAX_CALLABLE_BODY_PARAMETERS: usize = 4;
pub(crate) const MAX_CALLABLE_BODY_NODES: usize = 64;
pub(crate) const MAX_CALLABLE_BODY_CAPTURES: usize = 8;
pub(crate) const MAX_CALLABLE_BODY_ALIASES: usize = 8;
pub(crate) const MAX_CALLABLE_BODY_INSTANTIATIONS: usize = 8;
pub(crate) const MAX_CALLABLE_BODY_SOLVE_ITERATIONS: usize = 16;
pub(crate) const MAX_CALLABLE_BODY_UNION_VARIANTS: usize = 8;
pub(crate) const MAX_CALLABLE_BODY_TYPE_DEPTH: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum CallableBodyParameterKind {
    Required,
    Optional,
    Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct CallableBodyParameter {
    pub(crate) name: String,
    pub(crate) kind: CallableBodyParameterKind,
    /// The default is already lowered in the same parameter/capture domain as
    /// the body. Only optional parameters may carry one.
    pub(crate) default: Option<CallableBodyExpression>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum CallableBodyExpression {
    Literal(RubyType),
    Parameter(usize),
    Capture(String),
    Array(Vec<CallableBodyExpression>),
    Shape(Vec<(LiteralKey, CallableBodyExpression)>),
    Call {
        receiver: Box<CallableBodyExpression>,
        method: RubyMethod,
        arguments: Vec<CallableBodyExpression>,
        literal_argument_keys: Vec<Option<LiteralKey>>,
    },
    ExhaustiveUnion(Vec<CallableBodyExpression>),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct CallableBodySummary {
    pub(crate) strict_arity: bool,
    pub(crate) parameters: Vec<CallableBodyParameter>,
    pub(crate) captures: Vec<String>,
    pub(crate) result: CallableBodyExpression,
    pub(crate) node_count: u8,
}

impl CallableBodySummary {
    pub(crate) fn is_capture_free(&self) -> bool {
        self.captures.is_empty()
    }

    pub(crate) fn estimated_heap_bytes(&self) -> usize {
        self.parameters.capacity() * size_of::<CallableBodyParameter>()
            + self
                .parameters
                .iter()
                .map(|parameter| {
                    parameter.name.capacity()
                        + parameter
                            .default
                            .as_ref()
                            .map(expression_heap_bytes)
                            .unwrap_or(0)
                })
                .sum::<usize>()
            + self.captures.capacity() * size_of::<String>()
            + self.captures.iter().map(String::capacity).sum::<usize>()
            + expression_heap_bytes(&self.result)
    }

    pub(crate) fn validate(&self) -> Result<(), super::UnknownReason> {
        if self.parameters.len() > MAX_CALLABLE_BODY_PARAMETERS
            || self.captures.len() > MAX_CALLABLE_BODY_CAPTURES
            || usize::from(self.node_count) > MAX_CALLABLE_BODY_NODES
        {
            return Err(super::UnknownReason::CallableBodyBoundExceeded);
        }
        if self.captures.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(super::UnknownReason::UnsupportedCallableBody);
        }
        let mut parameter_names = BTreeSet::new();
        let mut rest_seen = false;
        for (index, parameter) in self.parameters.iter().enumerate() {
            if parameter.name.is_empty() || !parameter_names.insert(&parameter.name) {
                return Err(super::UnknownReason::UnsupportedCallableBody);
            }
            match parameter.kind {
                CallableBodyParameterKind::Required if parameter.default.is_none() => {}
                CallableBodyParameterKind::Optional if parameter.default.is_some() => {}
                CallableBodyParameterKind::Rest
                    if parameter.default.is_none()
                        && !rest_seen
                        && index + 1 == self.parameters.len() =>
                {
                    rest_seen = true;
                }
                CallableBodyParameterKind::Required
                | CallableBodyParameterKind::Optional
                | CallableBodyParameterKind::Rest => {
                    return Err(super::UnknownReason::UnsupportedCallableBody);
                }
            }
        }
        let mut nodes = 0usize;
        for default in self
            .parameters
            .iter()
            .filter_map(|parameter| parameter.default.as_ref())
        {
            validate_expression(default, self, 0, &mut nodes)?;
        }
        validate_expression(&self.result, self, 0, &mut nodes)?;
        if nodes != usize::from(self.node_count) {
            return Err(super::UnknownReason::UnsupportedCallableBody);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConstantCallableBodyFact {
    pub(crate) constant: FullyQualifiedName,
    pub(crate) summary: CallableBodySummary,
    pub(crate) range: TextRange,
}

impl ConstantCallableBodyFact {
    pub(crate) fn estimated_heap_bytes(&self) -> usize {
        fqn_heap_bytes(&self.constant) + self.summary.estimated_heap_bytes()
    }
}

fn expression_heap_bytes(expression: &CallableBodyExpression) -> usize {
    match expression {
        CallableBodyExpression::Literal(ruby_type) => ruby_type_heap_bytes(ruby_type),
        CallableBodyExpression::Parameter(_) => 0,
        CallableBodyExpression::Capture(name) => name.capacity(),
        CallableBodyExpression::Array(values) | CallableBodyExpression::ExhaustiveUnion(values) => {
            values.capacity() * size_of::<CallableBodyExpression>()
                + values.iter().map(expression_heap_bytes).sum::<usize>()
        }
        CallableBodyExpression::Shape(fields) => {
            fields.capacity() * size_of::<(LiteralKey, CallableBodyExpression)>()
                + fields
                    .iter()
                    .map(|(key, value)| key.heap_bytes() + expression_heap_bytes(value))
                    .sum::<usize>()
        }
        CallableBodyExpression::Call {
            receiver,
            arguments,
            literal_argument_keys,
            ..
        } => {
            expression_heap_bytes(receiver)
                + arguments.capacity() * size_of::<CallableBodyExpression>()
                + arguments.iter().map(expression_heap_bytes).sum::<usize>()
                + literal_argument_keys.capacity() * size_of::<Option<LiteralKey>>()
                + literal_argument_keys
                    .iter()
                    .flatten()
                    .map(LiteralKey::heap_bytes)
                    .sum::<usize>()
        }
    }
}

fn validate_expression(
    expression: &CallableBodyExpression,
    summary: &CallableBodySummary,
    depth: usize,
    nodes: &mut usize,
) -> Result<(), super::UnknownReason> {
    if depth > MAX_CALLABLE_BODY_TYPE_DEPTH {
        return Err(super::UnknownReason::CallableBodyBoundExceeded);
    }
    *nodes = nodes
        .checked_add(1)
        .ok_or(super::UnknownReason::CallableBodyBoundExceeded)?;
    if *nodes > MAX_CALLABLE_BODY_NODES {
        return Err(super::UnknownReason::CallableBodyBoundExceeded);
    }
    match expression {
        CallableBodyExpression::Literal(ruby_type) => {
            if RubyType::contains_unknown(ruby_type) {
                return Err(super::UnknownReason::UnsupportedCallableBody);
            }
        }
        CallableBodyExpression::Parameter(index) => {
            if *index >= summary.parameters.len() {
                return Err(super::UnknownReason::UnsupportedCallableBody);
            }
        }
        CallableBodyExpression::Capture(name) => {
            if summary.captures.binary_search(name).is_err() {
                return Err(super::UnknownReason::UnsupportedCallableBody);
            }
        }
        CallableBodyExpression::Array(values) => {
            for value in values {
                validate_expression(value, summary, depth + 1, nodes)?;
            }
        }
        CallableBodyExpression::Shape(fields) => {
            if fields.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
                return Err(super::UnknownReason::UnsupportedCallableBody);
            }
            for (_, value) in fields {
                validate_expression(value, summary, depth + 1, nodes)?;
            }
        }
        CallableBodyExpression::Call {
            receiver,
            arguments,
            literal_argument_keys,
            ..
        } => {
            if arguments.len() != literal_argument_keys.len() {
                return Err(super::UnknownReason::UnsupportedCallableBody);
            }
            validate_expression(receiver, summary, depth + 1, nodes)?;
            for argument in arguments {
                validate_expression(argument, summary, depth + 1, nodes)?;
            }
        }
        CallableBodyExpression::ExhaustiveUnion(values) => {
            if values.is_empty() || values.len() > MAX_CALLABLE_BODY_UNION_VARIANTS {
                return Err(super::UnknownReason::CallableBodyBoundExceeded);
            }
            for value in values {
                validate_expression(value, summary, depth + 1, nodes)?;
            }
        }
    }
    Ok(())
}
