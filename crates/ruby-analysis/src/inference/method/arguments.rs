//! What a call site proves about the arguments it passes.

use crate::core::RubyType;
use crate::inference::rbs::CallArguments;
use ruby_prism::{CallNode, Node};

/// The arguments of `call`, each typed by `infer`.
///
/// `None` when they cannot be matched to parameters one by one: a splat,
/// keywords, or forwarded arguments.
pub(crate) fn call_arguments(
    call: &CallNode<'_>,
    mut infer: impl FnMut(&Node<'_>) -> RubyType,
) -> Option<CallArguments> {
    let mut positional = Vec::new();
    if let Some(arguments) = call.arguments() {
        for argument in arguments.arguments().iter() {
            if argument.as_splat_node().is_some()
                || argument.as_keyword_hash_node().is_some()
                || argument.as_forwarding_arguments_node().is_some()
                || argument.as_block_argument_node().is_some()
            {
                return None;
            }
            positional.push(infer(&argument));
        }
    }
    let block = match call.block() {
        None => Some(false),
        Some(block) if block.as_block_node().is_some() => Some(true),
        // `&:name` always passes a block; any other `&value` may pass nil.
        Some(block) => block
            .as_block_argument_node()
            .and_then(|argument| argument.expression())
            .is_some_and(|expression| expression.as_symbol_node().is_some())
            .then_some(true),
    };
    Some(CallArguments { positional, block })
}
