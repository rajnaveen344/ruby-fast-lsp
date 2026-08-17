//! Prism-to-domain lowering for statically visible callable literals.
//!
//! Lowering happens against the already parsed tree. The returned value is a
//! compact AST-free summary; no node, source slice, or parser lifetime crosses
//! this boundary.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use ruby_prism::Node;

use crate::core::{
    CallableBodyExpression, CallableBodyParameter, CallableBodyParameterKind, CallableBodySummary,
    RubyMethod, RubyType, UnknownReason,
};
use crate::inference::control_flow::{self, Exit, Reachability};
use crate::inference::r#type::literal::{literal_key, LiteralAnalyzer};

pub(crate) fn is_static_callable_literal(value: &Node<'_>) -> bool {
    if value.as_lambda_node().is_some() {
        return true;
    }
    let Some(call) = value.as_call_node() else {
        return false;
    };
    let name = call.name().as_slice();
    matches!((call.receiver(), name), (None, b"lambda" | b"proc"))
        || matches!((call.receiver(), name), (Some(receiver), b"new") if receiver
            .as_constant_read_node()
            .is_some_and(|constant| constant.name().as_slice() == b"Proc"))
}

pub(crate) fn lower_callable_literal(
    value: &Node<'_>,
) -> Result<CallableBodySummary, UnknownReason> {
    lower_callable_literal_with_outer_locals(value, std::iter::empty::<String>())
}

pub(crate) fn lower_callable_literal_with_outer_locals(
    value: &Node<'_>,
    outer_locals: impl IntoIterator<Item = String>,
) -> Result<CallableBodySummary, UnknownReason> {
    let (strict_arity, parameters, body) = if let Some(lambda) = value.as_lambda_node() {
        (true, lambda.parameters(), lambda.body())
    } else {
        let call = value
            .as_call_node()
            .ok_or(UnknownReason::UnsupportedCallableBody)?;
        let name = String::from_utf8_lossy(call.name().as_slice());
        let strict_arity = match (call.receiver(), name.as_ref()) {
            (None, "lambda") => true,
            (None, "proc") => false,
            (Some(receiver), "new")
                if receiver
                    .as_constant_read_node()
                    .is_some_and(|constant| constant.name().as_slice() == b"Proc") =>
            {
                false
            }
            _ => return Err(UnknownReason::UnsupportedCallableBody),
        };
        let block = call
            .block()
            .and_then(|block| block.as_block_node())
            .ok_or(UnknownReason::UnsupportedCallableBody)?;
        (strict_arity, block.parameters(), block.body())
    };

    let mut state = LoweringState {
        outer_locals: outer_locals.into_iter().collect(),
        ..LoweringState::default()
    };
    let parameters = state.lower_parameters(parameters)?;
    let result = match body {
        Some(body) => state.lower_expression(&body, strict_arity)?,
        None => state.make(CallableBodyExpression::Literal(RubyType::nil_class()))?,
    };
    let captures = state.captures.into_iter().collect::<Vec<_>>();
    if captures.len() > crate::core::callable_body::MAX_CALLABLE_BODY_CAPTURES {
        return Err(UnknownReason::CallableBodyBoundExceeded);
    }
    let expanded_node_count = parameters
        .iter()
        .filter_map(|parameter| parameter.default.as_ref())
        .try_fold(expression_node_count(&result)?, |total, expression| {
            total
                .checked_add(expression_node_count(expression)?)
                .ok_or(UnknownReason::CallableBodyBoundExceeded)
        })?;
    if expanded_node_count > crate::core::callable_body::MAX_CALLABLE_BODY_NODES {
        return Err(UnknownReason::CallableBodyBoundExceeded);
    }
    let node_count =
        u8::try_from(expanded_node_count).map_err(|_| UnknownReason::CallableBodyBoundExceeded)?;
    let summary = CallableBodySummary {
        strict_arity,
        parameters,
        captures,
        result,
        node_count,
    };
    summary.validate()?;
    Ok(summary)
}

#[derive(Default)]
struct LoweringState {
    parameters: HashMap<String, usize>,
    locals: HashMap<String, CallableBodyExpression>,
    outer_locals: BTreeSet<String>,
    captures: BTreeSet<String>,
    node_count: usize,
}

impl LoweringState {
    fn lower_branch(
        &mut self,
        statements: Option<ruby_prism::StatementsNode<'_>>,
        strict_arity: bool,
    ) -> Result<Option<CallableBodyExpression>, UnknownReason> {
        let Some(statements) = statements else {
            return self
                .make(CallableBodyExpression::Literal(RubyType::nil_class()))
                .map(Some);
        };
        match control_flow::analyze(&statements.as_node()) {
            Reachability::Falls => self
                .lower_expression(&statements.as_node(), strict_arity)
                .map(Some),
            Reachability::Diverges(exits)
                if exits.contains(Exit::Break)
                    || exits.contains(Exit::Redo)
                    || exits.contains(Exit::Retry) =>
            {
                Err(UnknownReason::UnsupportedCallableFlow)
            }
            Reachability::Diverges(exits)
                if exits.contains(Exit::Return) || exits.contains(Exit::Next) =>
            {
                self.lower_expression(&statements.as_node(), strict_arity)
                    .map(Some)
            }
            Reachability::Diverges(_) => Ok(None),
        }
    }

    fn merge_branch_locals(
        &mut self,
        left: HashMap<String, CallableBodyExpression>,
        right: HashMap<String, CallableBodyExpression>,
    ) -> Result<(), UnknownReason> {
        let names = left
            .keys()
            .chain(right.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut merged = HashMap::with_capacity(names.len());
        for name in names {
            let left_value = left
                .get(&name)
                .cloned()
                .unwrap_or_else(|| CallableBodyExpression::Literal(RubyType::nil_class()));
            let right_value = right
                .get(&name)
                .cloned()
                .unwrap_or_else(|| CallableBodyExpression::Literal(RubyType::nil_class()));
            let value = if left_value == right_value {
                left_value
            } else {
                self.make(CallableBodyExpression::ExhaustiveUnion(vec![
                    left_value,
                    right_value,
                ]))?
            };
            merged.insert(name, value);
        }
        self.locals = merged;
        Ok(())
    }

    fn make(
        &mut self,
        expression: CallableBodyExpression,
    ) -> Result<CallableBodyExpression, UnknownReason> {
        self.node_count = self
            .node_count
            .checked_add(1)
            .ok_or(UnknownReason::CallableBodyBoundExceeded)?;
        if self.node_count > crate::core::callable_body::MAX_CALLABLE_BODY_NODES {
            return Err(UnknownReason::CallableBodyBoundExceeded);
        }
        Ok(expression)
    }

    fn lower_parameters(
        &mut self,
        parameters_node: Option<Node<'_>>,
    ) -> Result<Vec<CallableBodyParameter>, UnknownReason> {
        let Some(parameters_node) = parameters_node else {
            return Ok(Vec::new());
        };
        if parameters_node.as_numbered_parameters_node().is_some() {
            return Err(UnknownReason::UnsupportedCallableBody);
        }
        let parameters = parameters_node
            .as_block_parameters_node()
            .and_then(|node| node.parameters())
            .ok_or(UnknownReason::UnsupportedCallableBody)?;
        if parameters.keywords().iter().next().is_some()
            || parameters.keyword_rest().is_some()
            || parameters.block().is_some()
            || parameters.posts().iter().next().is_some()
        {
            return Err(UnknownReason::UnsupportedCallableBody);
        }

        let total = parameters.requireds().iter().count()
            + parameters.optionals().iter().count()
            + usize::from(parameters.rest().is_some());
        if total > crate::core::callable_body::MAX_CALLABLE_BODY_PARAMETERS {
            return Err(UnknownReason::CallableBodyBoundExceeded);
        }

        let mut lowered = Vec::with_capacity(total);
        for required in parameters.requireds().iter() {
            let parameter = required
                .as_required_parameter_node()
                .ok_or(UnknownReason::UnsupportedCallableBody)?;
            let name = String::from_utf8_lossy(parameter.name().as_slice()).to_string();
            self.insert_parameter(&name, lowered.len())?;
            lowered.push(CallableBodyParameter {
                name,
                kind: CallableBodyParameterKind::Required,
                default: None,
            });
        }
        for optional in parameters.optionals().iter() {
            let parameter = optional
                .as_optional_parameter_node()
                .ok_or(UnknownReason::UnsupportedCallableBody)?;
            let name = String::from_utf8_lossy(parameter.name().as_slice()).to_string();
            let default = self.lower_expression(&parameter.value(), true)?;
            self.insert_parameter(&name, lowered.len())?;
            lowered.push(CallableBodyParameter {
                name,
                kind: CallableBodyParameterKind::Optional,
                default: Some(default),
            });
        }
        if let Some(rest) = parameters.rest() {
            let parameter = rest
                .as_rest_parameter_node()
                .ok_or(UnknownReason::UnsupportedCallableBody)?;
            let name = parameter
                .name()
                .map(|name| String::from_utf8_lossy(name.as_slice()).to_string())
                .ok_or(UnknownReason::UnsupportedCallableBody)?;
            self.insert_parameter(&name, lowered.len())?;
            lowered.push(CallableBodyParameter {
                name,
                kind: CallableBodyParameterKind::Rest,
                default: None,
            });
        }
        Ok(lowered)
    }

    fn insert_parameter(&mut self, name: &str, index: usize) -> Result<(), UnknownReason> {
        if self.parameters.insert(name.to_string(), index).is_some() {
            return Err(UnknownReason::UnsupportedCallableBody);
        }
        Ok(())
    }

    fn lower_expression(
        &mut self,
        node: &Node<'_>,
        strict_arity: bool,
    ) -> Result<CallableBodyExpression, UnknownReason> {
        if let Some(statements) = node.as_statements_node() {
            let mut statements = statements.body().iter();
            let Some(first) = statements.next() else {
                return self.make(CallableBodyExpression::Literal(RubyType::nil_class()));
            };
            let mut result = self.lower_expression(&first, strict_arity)?;
            if control_flow::diverges(&first) {
                return Ok(result);
            }
            for statement in statements {
                result = self.lower_expression(&statement, strict_arity)?;
                if control_flow::diverges(&statement) {
                    return Ok(result);
                }
            }
            return Ok(result);
        }
        if let Some(write) = node.as_local_variable_write_node() {
            let value = self.lower_expression(&write.value(), strict_arity)?;
            let name = String::from_utf8_lossy(write.name().as_slice()).to_string();
            if self.parameters.contains_key(&name) || self.outer_locals.contains(&name) {
                return Err(UnknownReason::UnsupportedCallableFlow);
            }
            self.locals.insert(name, value.clone());
            return Ok(value);
        }
        if let Some(read) = node.as_local_variable_read_node() {
            let name = String::from_utf8_lossy(read.name().as_slice()).to_string();
            if let Some(index) = self.parameters.get(&name).copied() {
                return self.make(CallableBodyExpression::Parameter(index));
            }
            if let Some(value) = self.locals.get(&name).cloned() {
                return self.make(value);
            }
            self.captures.insert(name.clone());
            return self.make(CallableBodyExpression::Capture(name));
        }
        if let Some(array) = node.as_array_node() {
            let mut values = Vec::new();
            for element in array.elements().iter() {
                if element.as_splat_node().is_some() {
                    return Err(UnknownReason::UnsupportedCallableBody);
                }
                values.push(self.lower_expression(&element, strict_arity)?);
            }
            return self.make(CallableBodyExpression::Array(values));
        }
        if let Some(hash) = node.as_hash_node() {
            let mut fields = BTreeMap::new();
            for element in hash.elements().iter() {
                let association = element
                    .as_assoc_node()
                    .ok_or(UnknownReason::UnsupportedCallableBody)?;
                let key = literal_key(&association.key())
                    .ok_or(UnknownReason::UnsupportedCallableBody)?;
                if fields.contains_key(&key) {
                    return Err(UnknownReason::UnsupportedCallableBody);
                }
                fields.insert(
                    key,
                    self.lower_expression(&association.value(), strict_arity)?,
                );
            }
            return self.make(CallableBodyExpression::Shape(fields.into_iter().collect()));
        }
        if let Some(if_node) = node.as_if_node() {
            let entry_locals = self.locals.clone();
            self.locals = entry_locals.clone();
            let then_value = self.lower_branch(if_node.statements(), strict_arity)?;
            let then_locals = self.locals.clone();
            self.locals = entry_locals;
            let else_value = match if_node.subsequent() {
                Some(subsequent) => {
                    if let Some(else_node) = subsequent.as_else_node() {
                        self.lower_branch(else_node.statements(), strict_arity)?
                    } else if subsequent.as_if_node().is_some() {
                        Some(self.lower_expression(&subsequent, strict_arity)?)
                    } else {
                        return Err(UnknownReason::UnsupportedCallableFlow);
                    }
                }
                None => Some(self.make(CallableBodyExpression::Literal(RubyType::nil_class()))?),
            };
            let else_locals = self.locals.clone();
            return match (then_value, else_value) {
                (Some(then_value), Some(else_value)) => {
                    self.merge_branch_locals(then_locals, else_locals)?;
                    self.make(CallableBodyExpression::ExhaustiveUnion(vec![
                        then_value, else_value,
                    ]))
                }
                (Some(value), None) => {
                    self.locals = then_locals;
                    Ok(value)
                }
                (None, Some(value)) => {
                    self.locals = else_locals;
                    Ok(value)
                }
                (None, None) => Err(UnknownReason::UnsupportedCallableFlow),
            };
        }
        if let Some(unless_node) = node.as_unless_node() {
            let entry_locals = self.locals.clone();
            self.locals = entry_locals.clone();
            let then_value = self.lower_branch(unless_node.statements(), strict_arity)?;
            let then_locals = self.locals.clone();
            self.locals = entry_locals;
            let else_statements = unless_node
                .else_clause()
                .and_then(|else_node| else_node.statements());
            let else_value = self.lower_branch(else_statements, strict_arity)?;
            let else_locals = self.locals.clone();
            return match (then_value, else_value) {
                (Some(then_value), Some(else_value)) => {
                    self.merge_branch_locals(then_locals, else_locals)?;
                    self.make(CallableBodyExpression::ExhaustiveUnion(vec![
                        then_value, else_value,
                    ]))
                }
                (Some(value), None) => {
                    self.locals = then_locals;
                    Ok(value)
                }
                (None, Some(value)) => {
                    self.locals = else_locals;
                    Ok(value)
                }
                (None, None) => Err(UnknownReason::UnsupportedCallableFlow),
            };
        }
        if let Some(return_node) = node.as_return_node() {
            if !strict_arity {
                return Err(UnknownReason::UnsupportedCallableFlow);
            }
            let values = return_node
                .arguments()
                .map(|arguments| arguments.arguments().iter().collect::<Vec<_>>())
                .unwrap_or_default();
            return match values.as_slice() {
                [] => self.make(CallableBodyExpression::Literal(RubyType::nil_class())),
                [value] => self.lower_expression(value, strict_arity),
                _ => Err(UnknownReason::UnsupportedCallableBody),
            };
        }
        if let Some(next_node) = node.as_next_node() {
            let values = next_node
                .arguments()
                .map(|arguments| arguments.arguments().iter().collect::<Vec<_>>())
                .unwrap_or_default();
            return match values.as_slice() {
                [] => self.make(CallableBodyExpression::Literal(RubyType::nil_class())),
                [value] => self.lower_expression(value, strict_arity),
                _ => Err(UnknownReason::UnsupportedCallableBody),
            };
        }
        if let Some(begin) = node.as_begin_node() {
            if begin.rescue_clause().is_some()
                || begin.else_clause().is_some()
                || begin.ensure_clause().is_some()
            {
                return Err(UnknownReason::UnsupportedCallableFlow);
            }
            return match begin.statements() {
                Some(statements) => self.lower_expression(&statements.as_node(), strict_arity),
                None => self.make(CallableBodyExpression::Literal(RubyType::nil_class())),
            };
        }
        if let Some(call) = node.as_call_node() {
            if call.block().is_some() {
                return Err(UnknownReason::UnsupportedCallableBody);
            }
            let receiver = call
                .receiver()
                .ok_or(UnknownReason::UnsupportedCallableBody)?;
            let receiver = self.lower_expression(&receiver, strict_arity)?;
            let method = RubyMethod::new(String::from_utf8_lossy(call.name().as_slice()).as_ref())
                .map_err(|_| UnknownReason::UnsupportedCallableBody)?;
            let argument_nodes = call
                .arguments()
                .map(|arguments| arguments.arguments().iter().collect::<Vec<_>>())
                .unwrap_or_default();
            let literal_argument_keys = argument_nodes.iter().map(literal_key).collect::<Vec<_>>();
            let mut arguments = Vec::with_capacity(argument_nodes.len());
            for argument in argument_nodes {
                arguments.push(self.lower_expression(&argument, strict_arity)?);
            }
            return self.make(CallableBodyExpression::Call {
                receiver: Box::new(receiver),
                method,
                arguments,
                literal_argument_keys,
            });
        }

        let literal = LiteralAnalyzer::new()
            .analyze_literal(node)
            .filter(|ruby_type| !RubyType::contains_unknown(ruby_type))
            .ok_or(UnknownReason::UnsupportedCallableBody)?;
        self.make(CallableBodyExpression::Literal(literal))
    }
}

fn expression_node_count(expression: &CallableBodyExpression) -> Result<usize, UnknownReason> {
    let children = match expression {
        CallableBodyExpression::Literal(_)
        | CallableBodyExpression::Parameter(_)
        | CallableBodyExpression::Capture(_) => 0,
        CallableBodyExpression::Array(values) | CallableBodyExpression::ExhaustiveUnion(values) => {
            values.iter().try_fold(0usize, |total, value| {
                total
                    .checked_add(expression_node_count(value)?)
                    .ok_or(UnknownReason::CallableBodyBoundExceeded)
            })?
        }
        CallableBodyExpression::Shape(fields) => {
            fields.iter().try_fold(0usize, |total, (_, value)| {
                total
                    .checked_add(expression_node_count(value)?)
                    .ok_or(UnknownReason::CallableBodyBoundExceeded)
            })?
        }
        CallableBodyExpression::Call {
            receiver,
            arguments,
            ..
        } => arguments
            .iter()
            .try_fold(expression_node_count(receiver)?, |total, value| {
                total
                    .checked_add(expression_node_count(value)?)
                    .ok_or(UnknownReason::CallableBodyBoundExceeded)
            })?,
    };
    children
        .checked_add(1)
        .ok_or(UnknownReason::CallableBodyBoundExceeded)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lower(source: &str) -> Result<CallableBodySummary, UnknownReason> {
        let parse = ruby_prism::parse(source.as_bytes());
        let program = parse.node();
        let statement = program
            .as_program_node()
            .expect("fixture must parse as a program")
            .statements()
            .body()
            .iter()
            .next()
            .expect("fixture must contain one assignment");
        let value = statement
            .as_local_variable_write_node()
            .expect("fixture must assign one callable")
            .value();
        lower_callable_literal(&value)
    }

    #[test]
    fn lowers_parameter_calls_without_retaining_prism() {
        let summary = lower("convert = ->(value) { value.to_s }").unwrap();
        assert_eq!(summary.parameters.len(), 1);
        assert!(summary.captures.is_empty());
        assert!(matches!(
            summary.result,
            CallableBodyExpression::Call { .. }
        ));
    }

    #[test]
    fn rejects_parameter_boundary_plus_one() {
        assert_eq!(
            lower("convert = ->(a, b, c, d, e) { a }").unwrap_err(),
            UnknownReason::CallableBodyBoundExceeded
        );
    }
}
