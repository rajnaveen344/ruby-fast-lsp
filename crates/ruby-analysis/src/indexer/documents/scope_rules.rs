//! Scope rules every declaration walk shares: lexical constant lookup, the
//! namespace a static receiver names, and the execution context a block call
//! opens. Walks differ only in which namespaces they know, so each rule takes
//! that knowledge as a predicate or receiver resolver.

use crate::core::{
    FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphNodeKind, MethodVisibility,
    NamespaceKind, RubyConstant, RubyMethod, RubyType,
};
use crate::invariant::ExpectInvariant;
use ruby_prism::{CallNode, Location, MultiWriteNode, Node};
use std::collections::HashSet;

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
    // `def (A::B).x` needs parentheses around a qualified receiver.
    if let Some(parentheses) = receiver.as_parentheses_node() {
        let body = parentheses.body()?;
        let expression = match body.as_statements_node() {
            Some(statements) => {
                let [expression] = statements
                    .body()
                    .iter()
                    .collect::<Vec<_>>()
                    .try_into()
                    .ok()?;
                expression
            }
            None => body,
        };
        return resolve_receiver_namespace(&expression, self_namespace, lexical_context, is_known);
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

/// The namespace whose singleton side a `def self.name` defines on. `self`
/// must be a class or module object: in an instance method it is an instance,
/// and in a `class << self` body it is the singleton class, whose own
/// singleton side the index does not model.
pub fn self_definition_namespace(scope_tracker: &ScopeTracker) -> Option<Vec<RubyConstant>> {
    if scope_tracker.implicit_receiver_is_singleton_class() {
        return None;
    }
    implicit_singleton_namespace(scope_tracker)
}

/// The side of the receiver where a `def` lands in an eval call's block,
/// or `None` when `node` is not an eval call.
pub fn eval_definition_kind(node: &CallNode<'_>) -> Option<NamespaceKind> {
    match node.name().as_slice() {
        b"class_eval" | b"module_eval" | b"class_exec" | b"module_exec" => {
            Some(NamespaceKind::Instance)
        }
        b"instance_eval" | b"instance_exec" => Some(NamespaceKind::Singleton),
        _ => None,
    }
}

/// `Target.class_eval do … end` and its `module_*`/`instance_*` forms.
/// `resolve_receiver` is the walk's knowledge of constant and `self`
/// receivers.
pub fn eval_block(
    node: &CallNode<'_>,
    scope_tracker: &ScopeTracker,
    resolve_receiver: impl Fn(&Node<'_>) -> Option<Vec<RubyConstant>>,
) -> Option<BlockExecution> {
    let definition_kind = eval_definition_kind(node)?;
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

/// A Concern `class_methods do … end` block. It is sent to `self`, so it
/// defines methods on the `ClassMethods` module of the namespace `self`
/// names (`self_namespace`), which inside an eval block is the receiver. The
/// block keeps the lexical scope.
pub fn class_methods_block(
    node: &CallNode<'_>,
    self_namespace: Option<Vec<RubyConstant>>,
) -> Option<BlockExecution> {
    if node.receiver().is_some() || node.name().as_slice() != b"class_methods" {
        return None;
    }
    node.block()?;
    let mut target = self_namespace.filter(|namespace| !namespace.is_empty())?;
    target.push(RubyConstant::new("ClassMethods").expect_invariant(
        "static Concern ClassMethods constant is invalid",
        "`ClassMethods` is a valid Ruby constant",
        "inspect RubyConstant validation",
    ));
    Some(BlockExecution {
        implicit_namespace: target.clone(),
        implicit_kind: NamespaceKind::Singleton,
        definition_namespace: target,
        definition_kind: NamespaceKind::Instance,
    })
}

/// What one `def` declares on its owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MethodDeclaration {
    pub method: RubyMethod,
    pub kind: NamespaceKind,
    pub visibility: MethodVisibility,
    /// The `def` follows a bare `module_function`, so the module also gets a
    /// public singleton copy.
    pub module_function_copy: bool,
}

/// The current visibility state a `def` sees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DefinitionVisibility {
    pub default: MethodVisibility,
    pub module_function_mode: bool,
}

/// What a `def` of `method` declares on the `kind` side of its owner.
///
/// A receiverless `initialize` on the instance side of a proven class is the
/// class's constructor, recorded as public singleton `new`. A module has no
/// `new`, so its `initialize` stays an instance method. Ruby makes
/// `initialize`, `initialize_copy`, `initialize_clone`, `initialize_dup`, and
/// `respond_to_missing?` private on any non-singleton owner, whatever the
/// current default visibility. After a bare `module_function`, a receiverless
/// instance method is private and the module gets a public singleton copy.
pub fn method_declaration(
    method: RubyMethod,
    receiverless: bool,
    kind: NamespaceKind,
    proven_class: bool,
    current: DefinitionVisibility,
) -> MethodDeclaration {
    let declared = |method, kind, visibility, module_function_copy| MethodDeclaration {
        method,
        kind,
        visibility,
        module_function_copy,
    };
    if kind == NamespaceKind::Singleton {
        return declared(method, kind, current.default, false);
    }
    let name = method.as_str();
    if name == "initialize" && receiverless && proven_class {
        let new = RubyMethod::new("new").expect_invariant(
            "`new` must be a valid Ruby method name",
            "constructor normalization relies on RubyMethod validation",
            "update RubyMethod validation to accept `new`",
        );
        return declared(
            new,
            NamespaceKind::Singleton,
            MethodVisibility::Public,
            false,
        );
    }
    let module_function_copy = receiverless && current.module_function_mode;
    let always_private = matches!(
        name,
        "initialize"
            | "initialize_copy"
            | "initialize_clone"
            | "initialize_dup"
            | "respond_to_missing?"
    );
    let visibility = if always_private || module_function_copy {
        MethodVisibility::Private
    } else {
        current.default
    };
    declared(method, kind, visibility, module_function_copy)
}

/// What a receiverless `private`, `protected`, or `public` call changes.
#[derive(Debug)]
pub enum VisibilityCall<'pr> {
    /// A call without arguments: later definitions in the current scope take
    /// the visibility.
    Default(MethodVisibility),
    /// The named methods take the visibility and the default is unchanged.
    /// Ruby evaluates the arguments first, so a walk applies these after it
    /// has visited them: a `def` argument names the method it just defined.
    Methods(MethodVisibility, Vec<(String, Location<'pr>)>),
}

/// The visibility change a `private`, `protected`, or `public` call makes.
/// Symbol and string arguments name methods, as does a receiverless `def`
/// argument; other arguments name nothing a walk can see.
pub fn visibility_call<'pr>(node: &CallNode<'pr>) -> Option<VisibilityCall<'pr>> {
    let visibility = match node.name().as_slice() {
        b"private" => MethodVisibility::Private,
        b"protected" => MethodVisibility::Protected,
        b"public" => MethodVisibility::Public,
        _ => return None,
    };
    let arguments = node
        .arguments()
        .map(|arguments| arguments.arguments().iter().collect::<Vec<_>>())
        .unwrap_or_default();
    if arguments.is_empty() {
        return Some(VisibilityCall::Default(visibility));
    }
    let methods = arguments
        .iter()
        .filter_map(|argument| {
            if let Some(symbol) = argument.as_symbol_node() {
                let location = symbol.value_loc().unwrap_or_else(|| symbol.location());
                return Some((static_name(argument)?, location));
            }
            if let Some(string) = argument.as_string_node() {
                return Some((static_name(argument)?, string.content_loc()));
            }
            let definition = argument.as_def_node()?;
            definition.receiver().is_none().then(|| {
                (
                    String::from_utf8_lossy(definition.name().as_slice()).to_string(),
                    definition.name_loc(),
                )
            })
        })
        .collect();
    Some(VisibilityCall::Methods(visibility, methods))
}

/// Whether every same-file declaration of a namespace is a class. With no
/// same-file declaration, `known_kind` is the walk's project knowledge.
pub fn namespace_is_proven_class(
    same_file_kinds: impl IntoIterator<Item = GraphNodeKind>,
    known_kind: impl FnOnce() -> Option<GraphNodeKind>,
) -> bool {
    let mut declared = false;
    for kind in same_file_kinds {
        if kind != GraphNodeKind::Class {
            return false;
        }
        declared = true;
    }
    declared || known_kind() == Some(GraphNodeKind::Class)
}

/// The text of a symbol or string literal.
pub fn static_name(node: &Node<'_>) -> Option<String> {
    if let Some(symbol) = node.as_symbol_node() {
        return Some(String::from_utf8_lossy(symbol.unescaped()).to_string());
    }
    let string = node.as_string_node()?;
    Some(String::from_utf8_lossy(string.unescaped()).to_string())
}

/// The constant a `Parent::NAME` write or assignment target declares. Ruby
/// writes `::NAME` at the top level and looks `Parent` up like any constant
/// receiver, so `Shapes::Inner::NAME` lands on the `Inner` that lexical lookup
/// finds. A parent no walk knows stays where a declaration of the same path
/// would be named; a `self` whose namespace is unknown declares nothing.
pub fn constant_path_write_target(
    parent: Option<&Node<'_>>,
    name: &[u8],
    self_namespace: Option<&[RubyConstant]>,
    lexical_context: &[RubyConstant],
    is_known: &impl Fn(&FullyQualifiedName) -> bool,
) -> Option<FullyQualifiedName> {
    let constant = RubyConstant::new(&String::from_utf8_lossy(name)).ok()?;
    let mut parts = match parent {
        None => Vec::new(),
        Some(parent) => {
            match resolve_receiver_namespace(parent, self_namespace, lexical_context, is_known) {
                Some(owner) => owner,
                None => {
                    let reference = mixin_ref_from_node(parent)?;
                    declaration_candidates(&reference.parts, reference.absolute, lexical_context)
                        .swap_remove(0)
                }
            }
        }
    };
    parts.push(constant);
    Some(FullyQualifiedName::constant(parts))
}

/// Every target a multiple assignment writes, each with the value Ruby
/// assigns to it when syntax alone decides that value: a leading target of a
/// literal array without splats. Nested, splat, and trailing targets, and
/// targets of any other value, get `None`.
pub fn multi_write_targets<'pr>(node: &MultiWriteNode<'pr>) -> Vec<(Node<'pr>, Option<Node<'pr>>)> {
    let mut values = node
        .value()
        .as_array_node()
        .map(|array| array.elements().iter().collect::<Vec<_>>())
        .filter(|elements| {
            elements
                .iter()
                .all(|element| element.as_splat_node().is_none())
        })
        .unwrap_or_default()
        .into_iter();
    let mut targets = Vec::new();
    for left in node.lefts().iter() {
        let value = values.next();
        if left.as_multi_target_node().is_some() {
            push_unvalued_targets(left, &mut targets);
        } else {
            targets.push((left, value));
        }
    }
    if let Some(rest) = node.rest() {
        push_unvalued_targets(rest, &mut targets);
    }
    for right in node.rights().iter() {
        push_unvalued_targets(right, &mut targets);
    }
    targets
}

fn push_unvalued_targets<'pr>(
    target: Node<'pr>,
    targets: &mut Vec<(Node<'pr>, Option<Node<'pr>>)>,
) {
    if let Some(nested) = target.as_multi_target_node() {
        for left in nested.lefts().iter() {
            push_unvalued_targets(left, targets);
        }
        if let Some(rest) = nested.rest() {
            push_unvalued_targets(rest, targets);
        }
        for right in nested.rights().iter() {
            push_unvalued_targets(right, targets);
        }
        return;
    }
    if let Some(splat) = target.as_splat_node() {
        if let Some(expression) = splat.expression() {
            push_unvalued_targets(expression, targets);
        }
        return;
    }
    if target.as_implicit_rest_node().is_some() {
        return;
    }
    targets.push((target, None));
}

/// What a declaration walk does with a resolved namespace edge, given the
/// edges the file has declared so far.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeAdmission<'a> {
    /// The edge is new and consistent with the file's ancestry.
    Admit,
    /// The same edge is already declared.
    Duplicate,
    /// The class already inherits this other superclass.
    ConflictingSuperclass(&'a FullyQualifiedName),
    /// The target already has the source among its ancestors.
    Cycle,
}

/// Admit `source -kind-> target` only when it is new, gives a class at most
/// one superclass, and keeps the file's ancestry acyclic.
pub fn edge_admission<'a>(
    edges: &'a [GraphEdgeFact],
    source: &FullyQualifiedName,
    target: &FullyQualifiedName,
    kind: GraphEdgeKind,
) -> EdgeAdmission<'a> {
    if edges
        .iter()
        .any(|edge| &edge.source == source && &edge.target == target && edge.kind == kind)
    {
        return EdgeAdmission::Duplicate;
    }
    if kind == GraphEdgeKind::Superclass {
        if let Some(existing) = edges
            .iter()
            .find(|edge| &edge.source == source && edge.kind == GraphEdgeKind::Superclass)
        {
            return EdgeAdmission::ConflictingSuperclass(&existing.target);
        }
    }
    if ancestry_edge_kind(kind) && (source == target || ancestry_path_exists(edges, target, source))
    {
        return EdgeAdmission::Cycle;
    }
    EdgeAdmission::Admit
}

/// Whether `edges` lead from `start` to `destination` through ancestry edges.
fn ancestry_path_exists(
    edges: &[GraphEdgeFact],
    start: &FullyQualifiedName,
    destination: &FullyQualifiedName,
) -> bool {
    let mut pending = vec![start];
    let mut visited = HashSet::new();
    while let Some(current) = pending.pop() {
        if current == destination {
            return true;
        }
        if !visited.insert(current) {
            continue;
        }
        pending.extend(
            edges
                .iter()
                .filter(|edge| &edge.source == current && ancestry_edge_kind(edge.kind))
                .map(|edge| &edge.target),
        );
    }
    false
}

/// Whether an edge joins its source's own ancestor chain. `extend` adds to
/// the singleton class's ancestors instead, so `extend self` is no cycle.
fn ancestry_edge_kind(kind: GraphEdgeKind) -> bool {
    match kind {
        GraphEdgeKind::Superclass | GraphEdgeKind::Include | GraphEdgeKind::Prepend => true,
        GraphEdgeKind::Extend | GraphEdgeKind::ExecutionContextApplication => false,
    }
}
