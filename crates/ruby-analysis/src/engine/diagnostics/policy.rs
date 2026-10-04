//! Diagnostic policy: the code and severity of every diagnostic the engine
//! derives, and the rules that decide when a diagnostic may be claimed
//! (lookup-chain completeness shared with rename, explicit absence contracts,
//! dynamic mixin hooks, exception classes, arity, keyword suggestions, and
//! spelling distance).

use std::collections::{HashMap, HashSet};
use std::convert::Infallible;

use crate::core::{
    DiagnosticFact, DiagnosticSeverity, FullyQualifiedName, GraphEdgeKind, GraphNodeKind,
    MethodCallSignatureCandidate, MethodParamFact, MethodParamKind, NamespaceKind, RubyConstant,
    RubyMethod, TextRange,
};
use crate::engine::lookup::{LookupUnknown, MethodAnswer};
use crate::engine::Project;
use crate::invariant::ExpectInvariant;

/// The stable code and severity of one engine-derived diagnostic family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::engine) struct DiagnosticRule {
    code: &'static str,
    severity: DiagnosticSeverity,
}

impl DiagnosticRule {
    const fn new(code: &'static str, severity: DiagnosticSeverity) -> Self {
        Self { code, severity }
    }

    /// A diagnostic of this family at `range`.
    pub(in crate::engine) fn fact(
        self,
        range: TextRange,
        message: impl Into<String>,
    ) -> DiagnosticFact {
        DiagnosticFact::new(range, self.severity, self.code, message)
    }
}

pub(in crate::engine) const UNRESOLVED_CONSTANT: DiagnosticRule =
    DiagnosticRule::new("unresolved-constant", DiagnosticSeverity::Error);
pub(in crate::engine) const UNRESOLVED_METHOD: DiagnosticRule =
    DiagnosticRule::new("unresolved-method", DiagnosticSeverity::Warning);
pub(in crate::engine) const UNSUPPORTED_RUNTIME_API: DiagnosticRule =
    DiagnosticRule::new("unsupported-runtime-api", DiagnosticSeverity::Warning);
pub(in crate::engine) const WRONG_ARITY: DiagnosticRule =
    DiagnosticRule::new("wrong-arity", DiagnosticSeverity::Warning);
pub(in crate::engine) const UNKNOWN_KWARG: DiagnosticRule =
    DiagnosticRule::new("unknown-kwarg", DiagnosticSeverity::Warning);
pub(in crate::engine) const MISSING_KWARG: DiagnosticRule =
    DiagnosticRule::new("missing-kwarg", DiagnosticSeverity::Warning);
pub(in crate::engine) const RAISE_NON_EXCEPTION: DiagnosticRule =
    DiagnosticRule::new("raise-non-exception", DiagnosticSeverity::Warning);
pub(in crate::engine) const BAD_SPLAT: DiagnosticRule =
    DiagnosticRule::new("bad-splat", DiagnosticSeverity::Warning);
pub(in crate::engine) const NIL_CALL: DiagnosticRule =
    DiagnosticRule::new("nil-call", DiagnosticSeverity::Warning);

/// Families that resolve passes derive from candidates. A rebuild drops
/// these and keeps every other resolved fact.
const RESOLVE_DERIVED: [DiagnosticRule; 9] = [
    UNRESOLVED_CONSTANT,
    UNRESOLVED_METHOD,
    UNSUPPORTED_RUNTIME_API,
    WRONG_ARITY,
    UNKNOWN_KWARG,
    MISSING_KWARG,
    RAISE_NON_EXCEPTION,
    BAD_SPLAT,
    NIL_CALL,
];

/// Whether a resolve pass derives diagnostics with `code`.
pub(in crate::engine) fn is_resolve_derived(code: &str) -> bool {
    RESOLVE_DERIVED.iter().any(|rule| rule.code == code)
}

/// Code of a static `require` or `require_relative` the loader cannot
/// resolve. The loader produces these facts; the engine stores them apart
/// from resolve-derived diagnostics and swaps them on their own.
pub const UNRESOLVED_REQUIRE_CODE: &str = "unresolved-require";

/// Whether every ancestry edge reachable from an owner is known, read as a
/// method lookup answer. An ancestor with an unresolved lookup edge or an
/// ambiguous superclass answers `Unknown(IncompleteChain)`: absence of a
/// method on that chain is unproven, so a rename collision check fails closed
/// and an `unresolved-method` claim is suppressed. Otherwise the answer is
/// `Missing`: the chain can prove a method absent. Rename and absence
/// diagnostics both read this one walk; one value memoizes a pass.
pub(in crate::engine) struct AncestryCompleteness {
    unresolved_sources: HashSet<Vec<RubyConstant>>,
    ambiguous_superclasses: HashMap<FullyQualifiedName, bool>,
}

impl AncestryCompleteness {
    pub(in crate::engine) fn new(project: &Project) -> Self {
        Self {
            unresolved_sources: project.unresolved_lookup_edge_sources(),
            ambiguous_superclasses: HashMap::new(),
        }
    }

    /// The completeness of `owner`'s chain. `open_ancestor` is a further,
    /// caller-specific barrier checked at each ancestor after the shared ones.
    pub(in crate::engine) fn chain(
        &mut self,
        project: &Project,
        owner: &FullyQualifiedName,
        mut open_ancestor: impl FnMut(&FullyQualifiedName) -> bool,
    ) -> MethodAnswer<Infallible> {
        let mut pending = vec![owner.clone()];
        let mut visited = HashSet::new();
        while let Some(current) = pending.pop() {
            if !visited.insert(current.clone()) {
                continue;
            }
            let ambiguous = *self
                .ambiguous_superclasses
                .entry(current.clone())
                .or_insert_with(|| project.view().superclass_is_ambiguous(&current));
            if ambiguous
                || self.unresolved_sources.contains(&current.namespace_parts())
                || open_ancestor(&current)
            {
                return MethodAnswer::Unknown(LookupUnknown::IncompleteChain);
            }
            pending.extend(
                project
                    .graph_ancestry_edges_from(&current)
                    .into_iter()
                    .map(|edge| project.names.expand_interned_fqn(edge.target)),
            );
        }
        MethodAnswer::Missing
    }
}

/// When an `unresolved-method` claim may be made, memoized for one resolve
/// pass. An explicit absence contract always permits the claim. Otherwise the
/// owner's chain must prove absence: [`AncestryCompleteness`] plus the
/// barriers that only negative proof needs. Top-level owners are open, a
/// singleton owner needs its `Class`/`Module` metaclass, and no ancestor may
/// have a dynamic mixin hook. Positive lookup, rename included, still follows
/// the static edges past those barriers.
pub(in crate::engine) struct MethodAbsenceClaims {
    ancestry: AncestryCompleteness,
    dynamic_mixin_hooks: HashMap<FullyQualifiedName, bool>,
    suppressed_owners: HashMap<FullyQualifiedName, bool>,
}

impl MethodAbsenceClaims {
    pub(in crate::engine) fn new(project: &Project) -> Self {
        Self {
            ancestry: AncestryCompleteness::new(project),
            dynamic_mixin_hooks: HashMap::new(),
            suppressed_owners: HashMap::new(),
        }
    }

    /// Whether a claim that `owner` lacks `method` must be withheld.
    pub(in crate::engine) fn suppresses(
        &mut self,
        project: &Project,
        owner: &FullyQualifiedName,
        method: RubyMethod,
    ) -> bool {
        if project.method_absence_contract_matches_owner_name(owner, &method) {
            return false;
        }
        if let Some(suppressed) = self.suppressed_owners.get(owner) {
            return *suppressed;
        }
        let suppressed = !self.chain(project, owner).is_missing();
        self.suppressed_owners.insert(owner.clone(), suppressed);
        suppressed
    }

    /// Owners whose chain completeness this pass has decided.
    pub(in crate::engine) fn decided_owner_count(&self) -> usize {
        self.suppressed_owners.len()
    }

    fn chain(&mut self, project: &Project, owner: &FullyQualifiedName) -> MethodAnswer<Infallible> {
        if owner_is_open(project, owner) {
            return MethodAnswer::Unknown(LookupUnknown::IncompleteChain);
        }
        let hooks = &mut self.dynamic_mixin_hooks;
        self.ancestry.chain(project, owner, |ancestor| {
            *hooks
                .entry(ancestor.clone())
                .or_insert_with(|| namespace_has_dynamic_mixin_hook(project, ancestor))
        })
    }
}

/// Top-level Ruby is an open execution environment. Test/framework DSLs
/// install methods on Object/Kernel at runtime, and an empty namespace has no
/// closed declaration whose absent method set can be proven. A singleton
/// owner whose `Class` or `Module` metaclass is not indexed is open as well.
fn owner_is_open(project: &Project, owner: &FullyQualifiedName) -> bool {
    if owner.namespace_parts().is_empty() {
        return true;
    }
    if owner.namespace_kind() != Some(NamespaceKind::Singleton) {
        return false;
    }
    let metaclass = match project.view().first_graph_node_kind(owner) {
        Some(GraphNodeKind::Class) => "Class",
        Some(GraphNodeKind::Module) => "Module",
        None => return false,
    };
    let constant = RubyConstant::new(metaclass).unwrap_or_else(|error| {
        unreachable_invariant!(
            what = "Ruby metaclass name `{name}` is invalid: {error}",
            why = "class and Module are universal Ruby constants",
            fix = "preserve RubyConstant support for language-defined class names",
            name = metaclass,
            error = error,
        )
    });
    !project
        .view()
        .has_graph_node(&FullyQualifiedName::namespace(vec![constant]))
}

/// An include/prepend/extend callback can install methods through arbitrary
/// Ruby code. Static edges model the common `base.extend(ClassMethods)`
/// shape, but a custom callback remains an incomplete negative-proof surface
/// unless every effect is represented. Concrete lookup may still resolve known
/// methods; only "method is absent" diagnostics fail closed.
fn namespace_has_dynamic_mixin_hook(project: &Project, namespace: &FullyQualifiedName) -> bool {
    let instance_namespace = match namespace.namespace_kind() {
        Some(NamespaceKind::Instance) => namespace.clone(),
        Some(NamespaceKind::Singleton) => namespace.to_instance_namespace().expect_invariant(
            "singleton namespace cannot produce its instance counterpart",
            "method lookup chains contain only Namespace FQNs",
            "preserve Namespace identity while traversing mixin hooks",
        ),
        None => unreachable_invariant!(
            what = "method lookup completeness received a non-namespace FQN `{namespace}`",
            why = "only namespaces own method lookup chains",
            fix = "convert receiver types to Namespace FQNs before diagnostics",
            namespace = namespace,
        ),
    };

    for edge in project.view().graph_edges_from(&instance_namespace) {
        let callbacks: &[&str] = match edge.kind {
            GraphEdgeKind::Include => &["included", "append_features"],
            GraphEdgeKind::Prepend => &["prepended", "prepend_features"],
            GraphEdgeKind::Superclass
            | GraphEdgeKind::Extend
            | GraphEdgeKind::ExecutionContextApplication => continue,
        };
        if namespace_defines_any_singleton_method(project, &edge.target, callbacks) {
            return true;
        }
    }

    project
        .view()
        .graph_edges_from(namespace)
        .into_iter()
        .any(|edge| {
            edge.kind == GraphEdgeKind::Extend
                && namespace_defines_any_singleton_method(
                    project,
                    &edge.target,
                    &["extended", "extend_object"],
                )
        })
}

fn namespace_defines_any_singleton_method(
    project: &Project,
    namespace: &FullyQualifiedName,
    names: &[&str],
) -> bool {
    let Some(singleton) = namespace.to_singleton_namespace() else {
        return false;
    };
    names.iter().any(|name| {
        let method = RubyMethod::new(name).unwrap_or_else(|error| {
            unreachable_invariant!(
                what = "Ruby lifecycle method name `{name}` is invalid: {error}",
                why = "lifecycle names are fixed Ruby identifiers",
                fix = "preserve RubyMethod support for language-defined callback names",
                name = name,
                error = error,
            )
        });
        !project
            .view()
            .method_facts_matching_owner_name(&singleton, &method)
            .is_empty()
    })
}

pub(in crate::engine) const EXCEPTION_WHITELIST: &[&str] = &[
    "Exception",
    "StandardError",
    "RuntimeError",
    "ArgumentError",
    "TypeError",
    "NameError",
    "NoMethodError",
    "IOError",
    "RangeError",
    "NotImplementedError",
    "ZeroDivisionError",
    "IndexError",
    "KeyError",
    "StopIteration",
    "SystemExit",
    "Interrupt",
    "ScriptError",
    "SyntaxError",
    "LoadError",
    "LocalJumpError",
    "FrozenError",
    "EncodingError",
    "RegexpError",
    "SystemCallError",
    "ThreadError",
    "FiberError",
    "SecurityError",
    "SignalException",
];

pub(in crate::engine) const NON_EXCEPTION_TYPES: &[&str] = &[
    "Integer",
    "Float",
    "Rational",
    "Complex",
    "Numeric",
    "Array",
    "Hash",
    "Symbol",
    "Regexp",
    "Range",
    "Proc",
    "Method",
    "UnboundMethod",
    "IO",
    "File",
    "Dir",
    "Time",
    "Struct",
    "Encoding",
    "Fiber",
    "Thread",
    "Mutex",
    "Queue",
    "TrueClass",
    "FalseClass",
    "NilClass",
    "Binding",
    "BasicObject",
    "Object",
];

pub(in crate::engine) fn suggestion_threshold(name_len: usize) -> usize {
    match name_len {
        0..=2 => 0,
        3..=8 => 2,
        _ => 3,
    }
}

pub(in crate::engine) struct MethodArity {
    pub(in crate::engine) required: usize,
    pub(in crate::engine) optional: usize,
    pub(in crate::engine) has_rest: bool,
    pub(in crate::engine) required_keywords: Vec<String>,
    pub(in crate::engine) optional_keywords: Vec<String>,
    pub(in crate::engine) has_kwrest: bool,
}

impl MethodArity {
    pub(in crate::engine) fn from_params(params: &[MethodParamFact]) -> Self {
        let mut arity = Self {
            required: 0,
            optional: 0,
            has_rest: false,
            required_keywords: Vec::new(),
            optional_keywords: Vec::new(),
            has_kwrest: false,
        };
        for param in params {
            match param.kind {
                MethodParamKind::Required => arity.required += 1,
                MethodParamKind::Optional => arity.optional += 1,
                MethodParamKind::Rest | MethodParamKind::AnonymousRest => arity.has_rest = true,
                MethodParamKind::RequiredKeyword => {
                    arity.required_keywords.push(param.name.to_string())
                }
                MethodParamKind::OptionalKeyword => {
                    arity.optional_keywords.push(param.name.to_string())
                }
                MethodParamKind::KeywordRest | MethodParamKind::AnonymousKeywordRest => {
                    arity.has_kwrest = true
                }
                MethodParamKind::Block => {}
                MethodParamKind::Forwarding => {
                    arity.has_rest = true;
                    arity.has_kwrest = true;
                }
            }
        }
        arity
    }
}

pub(in crate::engine) fn arity_mismatch(
    signature: &MethodCallSignatureCandidate,
    arity: &MethodArity,
) -> Option<(usize, Option<usize>, usize)> {
    let min = arity.required;
    let max = if arity.has_rest {
        None
    } else {
        Some(arity.required + arity.optional)
    };

    if signature.has_positional_splat {
        let too_many = max
            .map(|max| signature.positional_count > max)
            .unwrap_or(false);
        if too_many {
            return Some((min, max, signature.positional_count));
        }
        return None;
    }

    let too_few = signature.positional_count < min;
    let too_many = max
        .map(|max| signature.positional_count > max)
        .unwrap_or(false);
    if too_few || too_many {
        Some((min, max, signature.positional_count))
    } else {
        None
    }
}

pub(in crate::engine) fn closest_keyword(target: &str, declared: &[String]) -> Option<String> {
    let threshold = suggestion_threshold(target.len());
    if threshold == 0 {
        return None;
    }
    let mut best: Option<(String, usize)> = None;
    for candidate in declared {
        let dist = levenshtein(candidate, target);
        if dist > threshold {
            continue;
        }
        match &best {
            Some((_, current_dist)) if *current_dist <= dist => {}
            Some(_) | None => best = Some((candidate.clone(), dist)),
        }
    }
    best.map(|(name, _)| name)
}

pub(in crate::engine) fn levenshtein(a: &str, b: &str) -> usize {
    if a == b {
        return 0;
    }
    if a.is_empty() {
        return b.chars().count();
    }
    if b.is_empty() {
        return a.chars().count();
    }

    let b_chars = b.chars().collect::<Vec<_>>();
    let mut previous = (0..=b_chars.len()).collect::<Vec<_>>();
    let mut current = vec![0; b_chars.len() + 1];

    for (i, ca) in a.chars().enumerate() {
        current[0] = i + 1;
        for (j, cb) in b_chars.iter().enumerate() {
            let cost = usize::from(ca != *cb);
            current[j + 1] = (previous[j + 1] + 1)
                .min(current[j] + 1)
                .min(previous[j] + cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }

    previous[b_chars.len()]
}
