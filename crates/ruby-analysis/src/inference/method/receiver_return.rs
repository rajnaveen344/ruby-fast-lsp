//! Methods whose every return is `self`.
//!
//! A call of such a method returns its receiver, whatever class the receiver
//! turns out to be. The rule is syntactic and all-or-nothing: one path that
//! returns anything else, or that may run with another `self`, proves nothing.

use ruby_prism::*;

/// Whether every completed path through `method` returns `self`.
///
/// The body must end in `self` and every `return` must return `self` from the
/// method's own body. A `return` inside a block is refused, because a block
/// may run with another object as `self`. So is a body with `rescue` or
/// `else`, whose clauses supply the value on other paths. A constructor's
/// value is discarded by `new`.
pub(crate) fn returns_only_receiver(method: &DefNode<'_>) -> bool {
    if method.name().as_slice() == b"initialize" {
        return false;
    }
    let Some(body) = method.body() else {
        return false;
    };
    let statements = if let Some(statements) = body.as_statements_node() {
        Some(statements)
    } else if let Some(begin) = body.as_begin_node() {
        if begin.rescue_clause().is_some() || begin.else_clause().is_some() {
            return false;
        }
        begin.statements()
    } else {
        None
    };
    let ends_in_self = statements
        .and_then(|statements| statements.body().iter().last())
        .is_some_and(|last| last.as_self_node().is_some());
    if !ends_in_self {
        return false;
    }
    let mut returns = ReturnWalk {
        block_depth: 0,
        only_receiver: true,
    };
    returns.visit(&body);
    returns.only_receiver
}

/// Checks each `return` that leaves the method being examined.
struct ReturnWalk {
    block_depth: usize,
    only_receiver: bool,
}

impl<'pr> Visit<'pr> for ReturnWalk {
    fn visit_return_node(&mut self, node: &ReturnNode<'pr>) {
        let returns_self = node.arguments().is_some_and(|arguments| {
            let mut values = arguments.arguments().iter();
            let first = values.next();
            first.is_some_and(|value| value.as_self_node().is_some()) && values.next().is_none()
        });
        if self.block_depth > 0 || !returns_self {
            self.only_receiver = false;
        }
    }

    fn visit_block_node(&mut self, node: &BlockNode<'pr>) {
        self.block_depth += 1;
        visit_block_node(self, node);
        self.block_depth -= 1;
    }

    // A lambda's `return` leaves the lambda, and a nested definition has its
    // own returns.
    fn visit_lambda_node(&mut self, _node: &LambdaNode<'pr>) {}
    fn visit_def_node(&mut self, _node: &DefNode<'pr>) {}
    fn visit_class_node(&mut self, _node: &ClassNode<'pr>) {}
    fn visit_module_node(&mut self, _node: &ModuleNode<'pr>) {}
    fn visit_singleton_class_node(&mut self, _node: &SingletonClassNode<'pr>) {}
}
