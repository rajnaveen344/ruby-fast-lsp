//! Scope rules every declaration walk shares: lexical constant lookup, the
//! namespace a static receiver names, and the execution context a block call
//! opens. Walks differ only in which namespaces they know, so each rule takes
//! that knowledge as a predicate or receiver resolver.

use crate::core::{FullyQualifiedName, GraphNodeKind, NamespaceKind, RubyConstant, RubyType};
use crate::invariant::ExpectInvariant;
use ruby_prism::{CallNode, Node};

use super::scope_tracker::{mixin_ref_from_node, ScopeTracker};

/// Lexical lookup candidates for `parts`, innermost enclosing namespace first
/// and the top level last. An absolute path has the single candidate `parts`.
pub fn lexical_candidates<'a>(
    parts: &'a [RubyConstant],
    absolute: bool,
    lexical_context: &'a [RubyConstant],
) -> impl Iterator<Item = Vec<RubyConstant>> + 'a {
    let depth = if absolute { 0 } else { lexical_context.len() };
    (0..=depth).rev().map(move |len| {
        let mut candidate = lexical_context[..len].to_vec();
        candidate.extend_from_slice(parts);
        candidate
    })
}

/// Candidates a class or module declaration name reopens: the name inside the
/// enclosing namespace, then a qualified name from the top level.
pub fn declaration_candidates(
    parts: &[RubyConstant],
    absolute: bool,
    lexical_context: &[RubyConstant],
) -> Vec<Vec<RubyConstant>> {
    let mut exact = if absolute {
        Vec::new()
    } else {
        lexical_context.to_vec()
    };
    exact.extend_from_slice(parts);
    let mut candidates = vec![exact];
    if !absolute && parts.len() > 1 && !lexical_context.is_empty() {
        candidates.push(parts.to_vec());
    }
    candidates
}

/// The class or module a declaration reopens through a constant alias, when
/// its name holds another namespace object of the declared kind.
/// `constant_type` returns the value type of the first candidate constant
/// that has one. A name holding its own declaration is an ordinary reopening,
/// and a class whose explicit superclass is the alias target declares a new
/// class: reopening would make the target inherit itself.
pub fn alias_reopen_target(
    name: &Node<'_>,
    kind: GraphNodeKind,
    superclass: Option<&FullyQualifiedName>,
    lexical_context: &[RubyConstant],
    constant_type: impl FnOnce(&[Vec<RubyConstant>]) -> Option<RubyType>,
) -> Option<FullyQualifiedName> {
    let reference = mixin_ref_from_node(name)?;
    let candidates = declaration_candidates(&reference.parts, reference.absolute, lexical_context);
    let syntactic = FullyQualifiedName::namespace(candidates[0].clone());
    let target = match (kind, constant_type(&candidates)?) {
        (GraphNodeKind::Class, RubyType::ClassReference(target))
        | (GraphNodeKind::Module, RubyType::ModuleReference(target)) => {
            target.to_instance_namespace()?
        }
        (
            GraphNodeKind::Class | GraphNodeKind::Module,
            RubyType::Class(_)
            | RubyType::ClassReference(_)
            | RubyType::Module(_)
            | RubyType::ModuleReference(_)
            | RubyType::Literal(_)
            | RubyType::Array(_)
            | RubyType::Hash(_, _)
            | RubyType::Shape(_)
            | RubyType::Union(_)
            | RubyType::Unknown,
        ) => return None,
    };
    invariant!(
        !target.namespace_parts().is_empty(),
        what = "a class or module alias names the root namespace",
        why = "a Ruby class or module object has a constant identity",
        fix = "reject root namespace values before alias reopening",
    );
    (target != syntactic && superclass != Some(&target)).then_some(target)
}

/// The first known namespace that `parts` names from `lexical_context`.
pub fn resolve_lexical_namespace(
    parts: &[RubyConstant],
    absolute: bool,
    lexical_context: &[RubyConstant],
    is_known: impl Fn(&FullyQualifiedName) -> bool,
) -> Option<FullyQualifiedName> {
    if parts.is_empty() {
        return None;
    }
    lexical_candidates(parts, absolute, lexical_context)
        .map(FullyQualifiedName::namespace)
        .find(|fqn| is_known(fqn))
}

/// The known namespace a constant, `self`, or `Const.const_get(:Name)`
/// receiver names. `self_namespace` is the class or module `self` names at
/// this point, when the walk knows it is one.
pub fn resolve_receiver_namespace(
    receiver: &Node<'_>,
    self_namespace: Option<&[RubyConstant]>,
    lexical_context: &[RubyConstant],
    is_known: &impl Fn(&FullyQualifiedName) -> bool,
) -> Option<Vec<RubyConstant>> {
    if receiver.as_self_node().is_some() {
        return self_namespace.map(<[RubyConstant]>::to_vec);
    }
    if let Some(call) = receiver.as_call_node() {
        if call.name().as_slice() != b"const_get" {
            return None;
        }
        let base = resolve_receiver_namespace(
            &call.receiver()?,
            self_namespace,
            lexical_context,
            is_known,
        )?;
        let arguments = call.arguments()?;
        let name = static_name(&arguments.arguments().iter().next()?)?;
        let mut namespace = base;
        namespace.push(RubyConstant::new(&name).ok()?);
        return is_known(&FullyQualifiedName::namespace(namespace.clone())).then_some(namespace);
    }
    let receiver = mixin_ref_from_node(receiver)?;
    resolve_lexical_namespace(
        &receiver.parts,
        receiver.absolute,
        lexical_context,
        is_known,
    )
    .map(|fqn| fqn.namespace_parts())
}

/// The execution context a statically recognized block call opens: what
/// `self` is inside the block and where a `def` inside it lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockExecution {
    pub implicit_namespace: Vec<RubyConstant>,
    pub implicit_kind: NamespaceKind,
    pub definition_namespace: Vec<RubyConstant>,
    pub definition_kind: NamespaceKind,
}

impl BlockExecution {
    pub fn enter(self, scope_tracker: &mut ScopeTracker) {
        scope_tracker.push_block_execution_context(
            self.implicit_namespace,
            self.implicit_kind,
            self.definition_namespace,
            self.definition_kind,
        );
    }
}

/// The namespace `self` names at the current point, when it is a class or
/// module object.
pub fn implicit_singleton_namespace(scope_tracker: &ScopeTracker) -> Option<Vec<RubyConstant>> {
    let (namespace, receiver_kind) = scope_tracker.implicit_receiver_context();
    (receiver_kind == NamespaceKind::Singleton && !namespace.is_empty()).then_some(namespace)
}

/// `Target.class_eval do … end` and its `module_*`/`instance_*` forms.
/// `resolve_receiver` is the walk's knowledge of constant and `self`
/// receivers.
pub fn eval_block(
    node: &CallNode<'_>,
    scope_tracker: &ScopeTracker,
    resolve_receiver: impl Fn(&Node<'_>) -> Option<Vec<RubyConstant>>,
) -> Option<BlockExecution> {
    let definition_kind = match node.name().as_slice() {
        b"class_eval" | b"module_eval" | b"class_exec" | b"module_exec" => NamespaceKind::Instance,
        b"instance_eval" | b"instance_exec" => NamespaceKind::Singleton,
        _ => return None,
    };
    node.block()?;
    let namespace = match node.receiver() {
        Some(receiver) => resolve_receiver(&receiver)?,
        None => implicit_singleton_namespace(scope_tracker)?,
    };
    Some(BlockExecution {
        implicit_namespace: namespace.clone(),
        implicit_kind: NamespaceKind::Singleton,
        definition_namespace: namespace,
        definition_kind,
    })
}

/// A `define_method`/`define_singleton_method` block, called directly or
/// through `send`. `self` inside the block is the defined method's receiver;
/// a `def` inside it still lands in the enclosing definition owner.
pub fn dynamic_definition_block(
    node: &CallNode<'_>,
    scope_tracker: &ScopeTracker,
    resolve_receiver: impl Fn(&Node<'_>) -> Option<Vec<RubyConstant>>,
) -> Option<BlockExecution> {
    node.block()?;
    let (implicit_namespace, implicit_kind) = match node.receiver() {
        None => {
            let kind = match node.name().as_slice() {
                b"define_method"
                    if !scope_tracker.execution_context_active()
                        && scope_tracker.in_singleton() =>
                {
                    NamespaceKind::Singleton
                }
                b"define_method" => NamespaceKind::Instance,
                b"define_singleton_method" => NamespaceKind::Singleton,
                _ => return None,
            };
            (implicit_singleton_namespace(scope_tracker)?, kind)
        }
        Some(receiver) if node.name().as_slice() == b"define_singleton_method" => {
            (resolve_receiver(&receiver)?, NamespaceKind::Singleton)
        }
        Some(receiver)
            if matches!(
                node.name().as_slice(),
                b"send" | b"public_send" | b"__send__"
            ) =>
        {
            let arguments = node.arguments()?;
            let selector = static_name(&arguments.arguments().iter().next()?)?;
            let kind = match selector.as_str() {
                "define_method" => NamespaceKind::Instance,
                "define_singleton_method" => NamespaceKind::Singleton,
                _ => return None,
            };
            if node.name().as_slice() == b"public_send" && kind == NamespaceKind::Instance {
                return None;
            }
            (resolve_receiver(&receiver)?, kind)
        }
        Some(_) => return None,
    };
    let (definition_namespace, definition_kind) = scope_tracker.method_definition_context();
    Some(BlockExecution {
        implicit_namespace,
        implicit_kind,
        definition_namespace,
        definition_kind,
    })
}

/// A Concern `class_methods do … end` block, which defines methods on the
/// enclosing namespace's `ClassMethods` module.
pub fn class_methods_block(node: &CallNode<'_>) -> Option<RubyConstant> {
    if node.receiver().is_some() || node.name().as_slice() != b"class_methods" {
        return None;
    }
    node.block()?;
    Some(RubyConstant::new("ClassMethods").expect_invariant(
        "static Concern ClassMethods constant is invalid",
        "`ClassMethods` is a valid Ruby constant",
        "inspect RubyConstant validation",
    ))
}

/// The text of a symbol or string literal.
pub fn static_name(node: &Node<'_>) -> Option<String> {
    if let Some(symbol) = node.as_symbol_node() {
        return Some(String::from_utf8_lossy(symbol.unescaped()).to_string());
    }
    let string = node.as_string_node()?;
    Some(String::from_utf8_lossy(string.unescaped()).to_string())
}
