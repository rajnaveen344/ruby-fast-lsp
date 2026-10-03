//! Flow-local callable bindings shared by the collector walk and `TypeTracker`.
//!
//! A local bound to a static proc or lambda literal keeps the literal's body
//! summary so a later `.call` or `&local` block argument can be instantiated.
//! Both flow walks apply the same rules: aliasing keeps the identity, any other
//! assignment drops it, an escaping read invalidates every alias, and a branch
//! merge keeps a binding only when every path agrees.

use crate::core::callables::callable_body::{
    CallableBodySummary, MAX_CALLABLE_BODY_ALIASES, MAX_CALLABLE_BODY_INSTANTIATIONS,
};
use crate::core::{RubyMethod, RubyType, TypeInferenceOutcome, UnknownReason};
use crate::inference::higher_order::{KnownProcType, PreparedCallableSet};
use crate::invariant::ExpectInvariant;
use ruby_prism::*;
use std::collections::{BTreeSet, HashMap, HashSet};

impl KnownProcType {
    /// The callable for a static proc or lambda literal, or `None` for any
    /// other expression. Free reads in the body that name one of the
    /// `outer_locals` become captures; they are listed only for a literal.
    pub(crate) fn from_literal<Locals: IntoIterator<Item = String>>(
        value: &Node<'_>,
        outer_locals: impl FnOnce() -> Locals,
    ) -> Option<Self> {
        crate::inference::callable_body::is_static_callable_literal(value).then(|| Self {
            identity: u32::try_from(value.location().start_offset()).expect_invariant(
                "callable literal offset exceeded u32",
                "analysis ranges already require u32 offsets",
                "reject oversized source before callable lowering",
            ),
            summary: crate::inference::callable_body::lower_callable_literal_with_outer_locals(
                value,
                outer_locals(),
            ),
        })
    }

    /// A callable held by a constant. Constants have no flow-local identity,
    /// so every constant shares the reserved one.
    pub(crate) fn constant(summary: CallableBodySummary) -> Self {
        Self {
            identity: u32::MAX,
            summary: Ok(summary),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LocalCallables {
    bindings: HashMap<String, KnownProcType>,
}

impl LocalCallables {
    pub(crate) fn get(&self, name: &str) -> Option<&KnownProcType> {
        self.bindings.get(name)
    }

    pub(crate) fn clear(&mut self) {
        self.bindings.clear();
    }

    /// Record `name = value`. `literal` is the callable lowered from `value`
    /// when it is a proc or lambda literal; a local read keeps the aliased
    /// identity; anything else unbinds `name`.
    pub(crate) fn assign(&mut self, name: &str, literal: Option<KnownProcType>, value: &Node<'_>) {
        if let Some(callable) = literal {
            self.bind(name.to_string(), callable);
        } else if let Some(alias) = value.as_local_variable_read_node() {
            let alias_name = String::from_utf8_lossy(alias.name().as_slice());
            if let Some(callable) = self.bindings.get(alias_name.as_ref()).cloned() {
                self.bind(name.to_string(), callable);
            } else {
                self.bindings.remove(name);
            }
        } else {
            self.bindings.remove(name);
        }
    }

    fn bind(&mut self, name: String, mut callable: KnownProcType) {
        if callable
            .summary
            .as_ref()
            .is_ok_and(|summary| summary.captures.binary_search(&name).is_ok())
        {
            callable.summary = Err(UnknownReason::CallableRecursionUnsupported);
        }
        let alias_count = self
            .bindings
            .iter()
            .filter(|(existing_name, existing)| {
                existing_name.as_str() != name && existing.identity == callable.identity
            })
            .count();
        if alias_count >= MAX_CALLABLE_BODY_ALIASES {
            callable.summary = Err(UnknownReason::CallableBodyBoundExceeded);
            for existing in self.bindings.values_mut() {
                if existing.identity == callable.identity {
                    existing.summary = Err(UnknownReason::CallableBodyBoundExceeded);
                }
            }
        }
        self.bindings.insert(name, callable);
    }

    /// A callable read anywhere in an assigned value may be stored and called
    /// later with unknown inputs. Assigning the literal itself or a bare alias
    /// is not an escape.
    pub(crate) fn invalidate_escaped_in_value(&mut self, value: &Node<'_>) {
        if self.bindings.is_empty()
            || crate::inference::callable_body::is_static_callable_literal(value)
            || value.as_local_variable_read_node().is_some()
        {
            return;
        }
        let mut escaped = EscapedCallableReads::default();
        escaped.visit(value);
        self.invalidate(escaped.names);
    }

    /// A callable passed to a call, or used as a receiver other than a direct
    /// `local.call`, escapes.
    pub(crate) fn invalidate_escaped_in_call(&mut self, call: &CallNode<'_>) {
        if self.bindings.is_empty() {
            return;
        }
        let mut escaped = EscapedCallableReads::default();
        escaped.visit_call_parts(call);
        self.invalidate(escaped.names);
    }

    fn invalidate(&mut self, names: HashSet<String>) {
        for name in names {
            let Some(identity) = self.bindings.get(&name).map(|callable| callable.identity) else {
                continue;
            };
            for callable in self.bindings.values_mut() {
                if callable.identity == identity {
                    callable.summary = Err(UnknownReason::EscapedCallableValue);
                }
            }
        }
    }

    /// Join two branch states. A name bound to different callables, or bound
    /// on only one path, stays bound but is ambiguous.
    pub(crate) fn merge(left: &Self, right: &Self) -> Self {
        let names = left
            .bindings
            .keys()
            .chain(right.bindings.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut bindings = HashMap::with_capacity(names.len());
        for name in names {
            let callable = match (left.bindings.get(&name), right.bindings.get(&name)) {
                (Some(left), Some(right)) if left == right => left.clone(),
                (Some(left), Some(right)) => KnownProcType {
                    identity: left.identity.min(right.identity),
                    summary: Err(UnknownReason::AmbiguousCallableValue),
                },
                (Some(callable), None) | (None, Some(callable)) => KnownProcType {
                    identity: callable.identity,
                    summary: Err(UnknownReason::AmbiguousCallableValue),
                },
                (None, None) => unreachable_invariant!(
                    what = "callable merge key `{name}` is absent from both branches",
                    why = "keys are derived from those exact maps",
                    fix = "keep key collection and lookup atomic",
                    name = name,
                ),
            };
            bindings.insert(name, callable);
        }
        Self { bindings }
    }

    /// Instantiate `callable` with `arguments`. Nested callable captures
    /// resolve through these bindings; `stack` holds the identities being
    /// instantiated so recursion and depth stay bounded.
    pub(crate) fn instantiate(
        &self,
        callable: &KnownProcType,
        arguments: &[RubyType],
        capture_type: &dyn Fn(&str) -> Option<RubyType>,
        resolve_method: &dyn Fn(&RubyType, &RubyMethod) -> TypeInferenceOutcome,
        stack: &mut Vec<u32>,
    ) -> TypeInferenceOutcome {
        if stack.contains(&callable.identity) {
            return TypeInferenceOutcome::unknown(UnknownReason::CallableRecursionUnsupported);
        }
        if stack.len() >= MAX_CALLABLE_BODY_INSTANTIATIONS {
            return TypeInferenceOutcome::unknown(UnknownReason::CallableBodyBoundExceeded);
        }
        let summary = match &callable.summary {
            Ok(summary) => summary,
            Err(reason) => return TypeInferenceOutcome::unknown(*reason),
        };
        stack.push(callable.identity);
        let result = crate::inference::callable_body::instantiate_callable_body(
            summary,
            arguments,
            capture_type,
            |capture, nested_arguments| {
                let nested = self.bindings.get(capture)?;
                Some(self.instantiate(
                    nested,
                    nested_arguments,
                    capture_type,
                    resolve_method,
                    stack,
                ))
            },
            |receiver, method, _arguments| resolve_method(receiver, method),
        );
        let popped = stack.pop().expect_invariant(
            "callable instantiation stack underflowed",
            "every accepted callable pushes exactly one identity",
            "keep push/evaluate/pop in one function",
        );
        invariant_eq!(
            popped,
            callable.identity,
            what = "callable instantiation stack order changed during evaluation",
            why = "nested evaluation must be strictly LIFO",
            fix = "do not retain or reorder stack entries",
        );
        result
    }

    /// Finish a higher-order call whose block argument is `callable`.
    pub(crate) fn finish_block_argument(
        &self,
        prepared: PreparedCallableSet,
        callable: &KnownProcType,
        capture_type: &dyn Fn(&str) -> Option<RubyType>,
        resolve_method: &dyn Fn(&RubyType, &RubyMethod) -> TypeInferenceOutcome,
    ) -> TypeInferenceOutcome {
        let mut stack = vec![callable.identity];
        prepared.finish_known_proc(
            callable,
            capture_type,
            |capture, arguments| {
                let nested = self.bindings.get(capture)?;
                Some(self.instantiate(nested, arguments, capture_type, resolve_method, &mut stack))
            },
            |receiver, method, _arguments| resolve_method(receiver, method),
        )
    }
}

#[derive(Default)]
struct EscapedCallableReads {
    names: HashSet<String>,
}

impl EscapedCallableReads {
    fn visit_call_parts(&mut self, call: &CallNode<'_>) {
        if let Some(receiver) = call.receiver() {
            let direct_invoke = call.name().as_slice() == b"call"
                && receiver.as_local_variable_read_node().is_some();
            if !direct_invoke {
                self.visit(&receiver);
            }
        }
        if let Some(arguments) = call.arguments() {
            self.visit_arguments_node(&arguments);
        }
        if let Some(block) = call.block().filter(|block| block.as_block_node().is_some()) {
            self.visit(&block);
        }
    }
}

impl<'pr> Visit<'pr> for EscapedCallableReads {
    fn visit_local_variable_read_node(&mut self, node: &LocalVariableReadNode<'pr>) {
        self.names
            .insert(String::from_utf8_lossy(node.name().as_slice()).to_string());
    }

    fn visit_call_node(&mut self, node: &CallNode<'pr>) {
        self.visit_call_parts(node);
    }
}

/// Names a block binds positionally: required, optional, rest, and post
/// parameters, or `_1`.. for numbered parameters.
pub(crate) fn block_parameter_names(block: &BlockNode<'_>) -> Vec<String> {
    let Some(parameters_node) = block.parameters() else {
        return Vec::new();
    };
    if let Some(numbered) = parameters_node.as_numbered_parameters_node() {
        return (1..=usize::from(numbered.maximum()))
            .map(|index| format!("_{index}"))
            .collect();
    }
    let Some(parameters) = parameters_node
        .as_block_parameters_node()
        .and_then(|node| node.parameters())
    else {
        return Vec::new();
    };

    let mut names = Vec::new();
    for required in parameters.requireds().iter() {
        if let Some(param) = required.as_required_parameter_node() {
            names.push(String::from_utf8_lossy(param.name().as_slice()).to_string());
        }
    }
    for optional in parameters.optionals().iter() {
        if let Some(param) = optional.as_optional_parameter_node() {
            names.push(String::from_utf8_lossy(param.name().as_slice()).to_string());
        }
    }
    if let Some(rest) = parameters.rest() {
        if let Some(param) = rest.as_rest_parameter_node() {
            if let Some(name) = param.name() {
                names.push(String::from_utf8_lossy(name.as_slice()).to_string());
            }
        }
    }
    for post in parameters.posts().iter() {
        if let Some(param) = post.as_required_parameter_node() {
            names.push(String::from_utf8_lossy(param.name().as_slice()).to_string());
        }
    }
    names
}
