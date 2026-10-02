//! Method lookup: one request type over every receiver, access, and product.

use std::sync::Arc;

use super::{LookupUnknown, MethodAnswer};
use crate::core::{
    FullyQualifiedName, MethodFact, NamespaceKind, ResolvedMethodCallee, RubyMethod, RubyType,
};
use crate::engine::queries::cache::{AnalysisQueryCache, MethodReturnQueryAccess};
use crate::engine::resolution::{
    method_facts_in_chain, method_lookup_chain,
    method_lookup_chain_has_unresolved_dependency_from_graph, module_instance_receivers,
    namespace_target_exists, MethodLookupChainCache,
};
use crate::engine::{ReceiverAccess, View};

/// Where a method lookup starts.
#[derive(Debug, Clone, Copy)]
pub enum LookupReceiver<'a> {
    /// Ordinary dispatch on a namespace. A module instance namespace with
    /// known includers dispatches to each includer.
    Namespace(&'a FullyQualifiedName),
    /// `instance_method(:name)` reflection: the namespace's own ancestor
    /// chain, without dispatch to module includers.
    Reflection(&'a FullyQualifiedName),
    /// Dispatch on every member of a receiver type; each member must prove a
    /// winner.
    Type(&'a RubyType),
    /// `super` inside `owner`'s definition of the requested method: the
    /// first definition after `owner` on its ancestor chain.
    Super { owner: &'a FullyQualifiedName },
    /// Implicit-self dispatch at file top level, on the root instance
    /// namespace.
    TopLevel,
}

/// The product a method lookup returns when it finds a winner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MethodWant {
    /// Every resolved callee with its resolution kind and definition ranges.
    Callees,
    /// The winning owner and its visible facts on the receiver's own chain.
    Facts,
    /// The proven return type.
    Return,
    /// The single referenced definition, for navigation and diagnostics.
    Reference,
    /// The facts whose parameters describe the call: RBS signatures when
    /// present, otherwise the winning definitions.
    Signatures,
}

/// One method lookup.
#[derive(Debug, Clone, Copy)]
pub struct MethodRequest<'a> {
    pub receiver: LookupReceiver<'a>,
    pub method: RubyMethod,
    pub access: ReceiverAccess<'a>,
    pub want: MethodWant,
}

/// A found method product; the variant matches [`MethodRequest::want`].
#[derive(Debug, Clone, PartialEq)]
pub enum MethodFound {
    Callees(Vec<ResolvedMethodCallee>),
    Facts {
        owner: FullyQualifiedName,
        facts: Vec<MethodFact>,
    },
    Return(RubyType),
    Reference(Arc<MethodFact>),
    Signatures(Arc<Vec<MethodFact>>),
}

impl MethodFound {
    fn want(&self) -> MethodWant {
        match self {
            MethodFound::Callees(_) => MethodWant::Callees,
            MethodFound::Facts { .. } => MethodWant::Facts,
            MethodFound::Return(_) => MethodWant::Return,
            MethodFound::Reference(_) => MethodWant::Reference,
            MethodFound::Signatures(_) => MethodWant::Signatures,
        }
    }
}

/// Answer `request` against `view`.
pub fn method(view: &View<'_>, request: MethodRequest<'_>) -> MethodAnswer {
    answer(view, request, None)
}

/// Answer `request` against `view`, reusing `cache` for callees, return
/// types, and signatures. The cache binds itself to one engine identity.
pub fn method_cached(
    view: &View<'_>,
    request: MethodRequest<'_>,
    cache: &AnalysisQueryCache,
) -> MethodAnswer {
    answer(view, request, Some(cache))
}

/// A receiver with top level resolved to its namespace.
#[derive(Clone, Copy)]
enum Target<'a> {
    Namespace(&'a FullyQualifiedName),
    Reflection(&'a FullyQualifiedName),
    Type(&'a RubyType),
    Super(&'a FullyQualifiedName),
}

fn answer(
    view: &View<'_>,
    request: MethodRequest<'_>,
    cache: Option<&AnalysisQueryCache>,
) -> MethodAnswer {
    let root = FullyQualifiedName::namespace_with_kind(Vec::new(), NamespaceKind::Instance);
    let target = match request.receiver {
        LookupReceiver::Namespace(namespace) => Target::Namespace(namespace),
        LookupReceiver::Reflection(namespace) => Target::Reflection(namespace),
        LookupReceiver::Type(receiver_type) => Target::Type(receiver_type),
        LookupReceiver::Super { owner } => Target::Super(owner),
        LookupReceiver::TopLevel => Target::Namespace(&root),
    };
    let method = &request.method;
    let access = request.access;
    match request.want {
        MethodWant::Callees => callees(view, target, method, access, cache),
        MethodWant::Facts => facts(view, target, method, access),
        MethodWant::Return => return_type(view, target, method, access, cache),
        MethodWant::Reference => reference(view, target, method, access),
        MethodWant::Signatures => signatures(view, target, method, access, cache),
    }
}

fn callees(
    view: &View<'_>,
    target: Target<'_>,
    method: &RubyMethod,
    access: ReceiverAccess<'_>,
    cache: Option<&AnalysisQueryCache>,
) -> MethodAnswer {
    let (allow_private, protected_caller) = access.visibility();
    let resolved = match target {
        Target::Namespace(namespace) => {
            let compute = || {
                view.resolve_method_callees_inner(
                    namespace,
                    method,
                    allow_private,
                    protected_caller,
                    None,
                )
            };
            match cache {
                Some(cache) => cache.method_callees(
                    view.query_cache_identity(),
                    namespace,
                    *method,
                    memo_access(access),
                    compute,
                ),
                None => compute(),
            }
        }
        Target::Type(receiver_type) => view.resolve_method_callees_for_type_inner(
            receiver_type,
            method,
            allow_private,
            protected_caller,
            None,
        ),
        Target::Reflection(namespace) => {
            if !is_any(access) {
                return MethodAnswer::Unknown(LookupUnknown::Unsupported);
            }
            return match view.resolve_reflected_method_callee(namespace, method) {
                Some(callee) => MethodAnswer::Found(MethodFound::Callees(vec![callee])),
                None => absence(view, namespace),
            };
        }
        Target::Super(owner) => {
            if !is_any(access) {
                return MethodAnswer::Unknown(LookupUnknown::Unsupported);
            }
            return match view.resolve_super_method_callee(owner, method) {
                Some(callee) => MethodAnswer::Found(MethodFound::Callees(vec![callee])),
                None => absence(view, owner),
            };
        }
    };
    match resolved {
        Some(callees) => MethodAnswer::Found(MethodFound::Callees(callees)),
        None => MethodAnswer::Unknown(LookupUnknown::Receiver),
    }
}

fn facts(
    view: &View<'_>,
    target: Target<'_>,
    method: &RubyMethod,
    access: ReceiverAccess<'_>,
) -> MethodAnswer {
    let namespace = match target {
        Target::Namespace(namespace) | Target::Reflection(namespace) => namespace,
        Target::Type(_) | Target::Super(_) => {
            return MethodAnswer::Unknown(LookupUnknown::Unsupported);
        }
    };
    let (allow_private, protected_caller) = access.visibility();
    let chain = method_lookup_chain(view.engine, namespace);
    if let Some((owner, facts)) =
        method_facts_in_chain(view.engine, &chain, method, allow_private, protected_caller)
    {
        return MethodAnswer::Found(MethodFound::Facts { owner, facts });
    }
    // A module's own chain does not show the methods its includers supply.
    if matches!(target, Target::Namespace(_))
        && !module_instance_receivers(view.engine, namespace).is_empty()
    {
        return MethodAnswer::Unknown(LookupUnknown::Receiver);
    }
    absence(view, namespace)
}

fn return_type(
    view: &View<'_>,
    target: Target<'_>,
    method: &RubyMethod,
    access: ReceiverAccess<'_>,
    cache: Option<&AnalysisQueryCache>,
) -> MethodAnswer {
    let found = match target {
        Target::Namespace(namespace) => match cache {
            Some(cache) => {
                view.method_return_type_for_receiver_memo(namespace, method, access, cache)
            }
            None => view.method_return_type_for_receiver_access(namespace, method, access),
        },
        Target::Super(owner) => {
            if !is_any(access) {
                return MethodAnswer::Unknown(LookupUnknown::Unsupported);
            }
            let Some(callee) = view.resolve_super_method_callee(owner, method) else {
                return absence(view, owner);
            };
            view.method_return_type_for_callee(&callee)
        }
        Target::Type(_) | Target::Reflection(_) => {
            return MethodAnswer::Unknown(LookupUnknown::Unsupported);
        }
    };
    match found {
        Some(return_type) => MethodAnswer::Found(MethodFound::Return(return_type)),
        None => MethodAnswer::Unknown(LookupUnknown::NoEvidence),
    }
}

fn signatures(
    view: &View<'_>,
    target: Target<'_>,
    method: &RubyMethod,
    access: ReceiverAccess<'_>,
    cache: Option<&AnalysisQueryCache>,
) -> MethodAnswer {
    let (allow_private, protected_caller) = access.visibility();
    let facts = match target {
        Target::Namespace(namespace) => view.resolve_method_signature_facts_maybe_cached(
            namespace,
            method,
            allow_private,
            protected_caller,
            cache,
        ),
        Target::Type(receiver_type) => {
            Arc::new(view.resolve_method_signature_facts_for_type_inner(
                receiver_type,
                method,
                allow_private,
                protected_caller,
                cache,
            ))
        }
        Target::Reflection(_) | Target::Super(_) => {
            return MethodAnswer::Unknown(LookupUnknown::Unsupported);
        }
    };
    if facts.is_empty() {
        return MethodAnswer::Unknown(LookupUnknown::NoEvidence);
    }
    MethodAnswer::Found(MethodFound::Signatures(facts))
}

fn reference(
    view: &View<'_>,
    target: Target<'_>,
    method: &RubyMethod,
    access: ReceiverAccess<'_>,
) -> MethodAnswer {
    if !is_any(access) {
        return MethodAnswer::Unknown(LookupUnknown::Unsupported);
    }
    let (namespace, result) = match target {
        Target::Namespace(namespace) => {
            (namespace, view.resolve_method_reference(namespace, method))
        }
        Target::Reflection(namespace) => (
            namespace,
            view.resolve_instance_method_reference_with_chain_cache(
                namespace,
                method,
                &mut MethodLookupChainCache::new(),
            ),
        ),
        Target::Super(owner) => (owner, view.resolve_super_method_reference(owner, method)),
        Target::Type(_) => return MethodAnswer::Unknown(LookupUnknown::Unsupported),
    };
    // Reference resolution reports its own unknown edges; a plain miss is
    // still re-checked because the super path stops at a missing callee
    // without classifying why.
    match result.map_found(MethodFound::Reference) {
        MethodAnswer::Missing => absence(view, namespace),
        answer @ (MethodAnswer::Found(_)
        | MethodAnswer::Ambiguous { .. }
        | MethodAnswer::Unknown(_)) => answer,
    }
}

/// Classify a lookup that selected nothing on `namespace`'s chain: absence is
/// proven only when the namespace is indexed and its ancestry is complete.
fn absence(view: &View<'_>, namespace: &FullyQualifiedName) -> MethodAnswer {
    if !namespace_target_exists(view.engine, namespace) {
        return MethodAnswer::Unknown(LookupUnknown::Receiver);
    }
    if method_lookup_chain_has_unresolved_dependency_from_graph(view.engine, namespace) {
        return MethodAnswer::Unknown(LookupUnknown::IncompleteChain);
    }
    MethodAnswer::Missing
}

fn is_any(access: ReceiverAccess<'_>) -> bool {
    match access {
        ReceiverAccess::Any => true,
        ReceiverAccess::Protected { .. } | ReceiverAccess::Public => false,
    }
}

fn memo_access(access: ReceiverAccess<'_>) -> MethodReturnQueryAccess {
    match access {
        ReceiverAccess::Any => MethodReturnQueryAccess::Private,
        ReceiverAccess::Public => MethodReturnQueryAccess::Public,
        ReceiverAccess::Protected { caller } => MethodReturnQueryAccess::Protected(caller.clone()),
    }
}

impl MethodAnswer {
    /// The callees of a [`MethodWant::Callees`] lookup; `None` when no
    /// definition was selected.
    pub fn into_callees(self) -> Option<Vec<ResolvedMethodCallee>> {
        match self {
            MethodAnswer::Found(MethodFound::Callees(callees)) => Some(callees),
            MethodAnswer::Missing | MethodAnswer::Unknown(_) => None,
            MethodAnswer::Found(
                found @ (MethodFound::Facts { .. }
                | MethodFound::Return(_)
                | MethodFound::Reference(_)
                | MethodFound::Signatures(_)),
            ) => wrong_payload(MethodWant::Callees, &found),
            MethodAnswer::Ambiguous { owner, method } => {
                unexpected_ambiguity(MethodWant::Callees, &owner, method)
            }
        }
    }

    /// The winning owner and facts of a [`MethodWant::Facts`] lookup.
    pub fn into_facts(self) -> Option<(FullyQualifiedName, Vec<MethodFact>)> {
        match self {
            MethodAnswer::Found(MethodFound::Facts { owner, facts }) => Some((owner, facts)),
            MethodAnswer::Missing | MethodAnswer::Unknown(_) => None,
            MethodAnswer::Found(
                found @ (MethodFound::Callees(_)
                | MethodFound::Return(_)
                | MethodFound::Reference(_)
                | MethodFound::Signatures(_)),
            ) => wrong_payload(MethodWant::Facts, &found),
            MethodAnswer::Ambiguous { owner, method } => {
                unexpected_ambiguity(MethodWant::Facts, &owner, method)
            }
        }
    }

    /// The proven return type of a [`MethodWant::Return`] lookup.
    pub fn into_return_type(self) -> Option<RubyType> {
        match self {
            MethodAnswer::Found(MethodFound::Return(return_type)) => Some(return_type),
            MethodAnswer::Missing | MethodAnswer::Unknown(_) => None,
            MethodAnswer::Found(
                found @ (MethodFound::Callees(_)
                | MethodFound::Facts { .. }
                | MethodFound::Reference(_)
                | MethodFound::Signatures(_)),
            ) => wrong_payload(MethodWant::Return, &found),
            MethodAnswer::Ambiguous { owner, method } => {
                unexpected_ambiguity(MethodWant::Return, &owner, method)
            }
        }
    }

    /// The facts of a [`MethodWant::Signatures`] lookup; empty when none is
    /// proven.
    pub fn into_signatures(self) -> Arc<Vec<MethodFact>> {
        match self {
            MethodAnswer::Found(MethodFound::Signatures(facts)) => facts,
            MethodAnswer::Missing | MethodAnswer::Unknown(_) => Arc::new(Vec::new()),
            MethodAnswer::Found(
                found @ (MethodFound::Callees(_)
                | MethodFound::Facts { .. }
                | MethodFound::Return(_)
                | MethodFound::Reference(_)),
            ) => wrong_payload(MethodWant::Signatures, &found),
            MethodAnswer::Ambiguous { owner, method } => {
                unexpected_ambiguity(MethodWant::Signatures, &owner, method)
            }
        }
    }

    /// The outcome of a [`MethodWant::Reference`] lookup.
    pub fn into_reference(self) -> MethodAnswer<Arc<MethodFact>> {
        self.map_found(|found| match found {
            MethodFound::Reference(fact) => fact,
            found @ (MethodFound::Callees(_)
            | MethodFound::Facts { .. }
            | MethodFound::Return(_)
            | MethodFound::Signatures(_)) => wrong_payload(MethodWant::Reference, &found),
        })
    }
}

fn wrong_payload(want: MethodWant, found: &MethodFound) -> ! {
    unreachable_invariant!(
        what = "a {:?} method lookup answered with a {:?} payload",
        why = "each want produces exactly its own payload variant",
        fix = "build the answer in the want's branch of lookup::method",
        want,
        found.want(),
    );
}

fn unexpected_ambiguity(want: MethodWant, owner: &FullyQualifiedName, method: RubyMethod) -> ! {
    unreachable_invariant!(
        what = "a {:?} method lookup reported ambiguity at {}#{}",
        why = "only reference lookups select a single definition and can be ambiguous",
        fix = "return the ambiguity only from the reference branch of lookup::method",
        want,
        owner,
        method,
    );
}
