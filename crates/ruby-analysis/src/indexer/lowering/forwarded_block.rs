use ruby_prism::{DefNode, Node};

use crate::core::{DirectYieldCall, ForwardedBlockCall, RubyMethod};

/// Recognize the bounded forwarding form whose method result is exactly one
/// higher-order call on an ordinary parameter with the named block parameter.
/// More complex control flow, arguments, or dynamic forwarding remains
/// unsupported rather than being summarized approximately.
pub(crate) fn direct_forwarded_block_call(node: &DefNode<'_>) -> Option<ForwardedBlockCall> {
    let parameters = node.parameters()?;
    let block_name = parameters.block()?.name()?;
    let block_name = String::from_utf8_lossy(block_name.as_slice());
    let body = sole_statement(node.body()?)?;
    let call = body.as_call_node()?;
    if call
        .arguments()
        .is_some_and(|arguments| arguments.arguments().iter().next().is_some())
    {
        return None;
    }
    let block_argument = call.block()?.as_block_argument_node()?;
    let forwarded_local = block_argument.expression()?.as_local_variable_read_node()?;
    if forwarded_local.name().as_slice() != block_name.as_bytes() {
        return None;
    }
    let receiver = call.receiver()?.as_local_variable_read_node()?;
    let receiver_parameter = String::from_utf8_lossy(receiver.name().as_slice()).to_string();
    let is_ordinary_parameter = parameters.requireds().iter().any(|parameter| {
        parameter
            .as_required_parameter_node()
            .is_some_and(|parameter| parameter.name().as_slice() == receiver.name().as_slice())
    }) || parameters.optionals().iter().any(|parameter| {
        parameter
            .as_optional_parameter_node()
            .is_some_and(|parameter| parameter.name().as_slice() == receiver.name().as_slice())
    });
    if !is_ordinary_parameter {
        return None;
    }
    let method = RubyMethod::new(&String::from_utf8_lossy(call.name().as_slice())).ok()?;
    Some(ForwardedBlockCall {
        receiver_parameter,
        method,
    })
}

/// Recognize a method whose entire result is one direct `yield` of ordinary
/// parameters. The bounded form is enough to adapt Ruby-defined yielding
/// methods into the same callable constraint model without summarizing
/// arbitrary method control flow.
pub(crate) fn direct_yield_call(node: &DefNode<'_>) -> Option<DirectYieldCall> {
    let parameters = node.parameters()?;
    let body = sole_statement(node.body()?)?;
    let yield_node = body.as_yield_node()?;
    let arguments = yield_node.arguments()?;
    let mut parameter_names = Vec::new();
    for argument in arguments.arguments().iter() {
        let local = argument.as_local_variable_read_node()?;
        let name = String::from_utf8_lossy(local.name().as_slice()).to_string();
        let is_ordinary_parameter = parameters.requireds().iter().any(|parameter| {
            parameter
                .as_required_parameter_node()
                .is_some_and(|parameter| parameter.name().as_slice() == local.name().as_slice())
        }) || parameters.optionals().iter().any(|parameter| {
            parameter
                .as_optional_parameter_node()
                .is_some_and(|parameter| parameter.name().as_slice() == local.name().as_slice())
        });
        if !is_ordinary_parameter {
            return None;
        }
        parameter_names.push(name);
    }
    Some(DirectYieldCall { parameter_names })
}

fn sole_statement(body: Node<'_>) -> Option<Node<'_>> {
    let statements = body.as_statements_node()?;
    let mut body = statements.body().iter();
    let statement = body.next()?;
    body.next().is_none().then_some(statement)
}
