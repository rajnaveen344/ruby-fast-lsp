//! Method facts, visibility, and `method_missing` within a lookup chain.

use super::lookup_chain::method_lookup_chain;
use super::method_name_from_fact;
use crate::core::MethodVisibility;
use crate::core::{
    FullyQualifiedName, GraphEdgeKind, MethodCalleeResolution, MethodFact, ResolvedMethodCallee,
    RubyConstant, RubyMethod,
};
use crate::engine::state::EffectiveMethodFactMatch;
use crate::invariant::ExpectInvariant;

pub(in crate::engine) fn execution_context_application_targets(
    engine: &crate::engine::Project,
    template: &FullyQualifiedName,
) -> Vec<FullyQualifiedName> {
    let mut targets = engine
        .graph_stored_edges_from_kind(template, GraphEdgeKind::ExecutionContextApplication)
        .into_iter()
        .map(|edge| engine.names.expand_interned_fqn(edge.target))
        .collect::<Vec<_>>();
    targets.sort_by_key(ToString::to_string);
    targets.dedup();
    targets
}

pub(super) fn method_callee_in_chain(
    engine: &crate::engine::Project,
    ancestor_chain: &[FullyQualifiedName],
    method: &RubyMethod,
    resolution: MethodCalleeResolution,
    allow_private: bool,
    protected_caller: Option<&FullyQualifiedName>,
) -> Option<ResolvedMethodCallee> {
    let (owner, facts) = method_facts_in_chain(
        engine,
        ancestor_chain,
        method,
        allow_private,
        protected_caller,
    )?;
    Some(ResolvedMethodCallee {
        owner,
        method: *method,
        resolution,
        definition_ranges: facts.into_iter().map(|fact| fact.range).collect(),
    })
}

pub(in crate::engine) fn method_facts_in_chain(
    engine: &crate::engine::Project,
    ancestor_chain: &[FullyQualifiedName],
    method: &RubyMethod,
    allow_private: bool,
    protected_caller: Option<&FullyQualifiedName>,
) -> Option<(FullyQualifiedName, Vec<MethodFact>)> {
    for ancestor in ancestor_chain {
        let mut facts = engine
            .view()
            .method_facts_matching_owner_name(ancestor, method)
            .into_iter()
            .filter(|fact| {
                ancestor_chain.iter().any(|chain_fqn| {
                    chain_fqn.namespace_parts() == fact.owner.namespace_parts()
                        && chain_fqn.namespace_kind() == fact.owner.namespace_kind()
                }) && {
                    let (visibility, owner) =
                        effective_method_visibility_for_chain(engine, ancestor_chain, fact, method);
                    method_visibility_allowed(
                        engine,
                        visibility,
                        &owner,
                        allow_private,
                        protected_caller,
                    )
                }
            })
            .collect::<Vec<_>>();

        if facts.iter().any(|fact| {
            engine
                .view()
                .file(fact.range.file_id)
                .expect_invariant(
                    "method fact references an unregistered source file",
                    "engine facts must never outlive their file metadata",
                    "register the file before replacing method facts",
                )
                .kind
                != crate::core::SourceKind::Signature
        }) {
            facts.retain(|fact| {
                engine
                    .view()
                    .file(fact.range.file_id)
                    .expect_invariant(
                        "method fact references an unregistered source file",
                        "source precedence requires valid file metadata",
                        "remove facts through the per-file replacement lifecycle",
                    )
                    .kind
                    != crate::core::SourceKind::Signature
            });
        }

        if !facts.is_empty() {
            facts.sort_by_key(|fact| {
                (
                    fact.range.file_id,
                    fact.range.start_byte,
                    fact.range.end_byte,
                    fact.fqn.to_string(),
                )
            });
            facts.dedup();
            return Some((ancestor.clone(), facts));
        }
    }

    None
}

pub(super) fn private_method_in_chain(
    engine: &crate::engine::Project,
    ancestor_chain: &[FullyQualifiedName],
    method: &RubyMethod,
) -> bool {
    ancestor_chain.iter().any(|ancestor| {
        engine
            .view()
            .method_facts_matching_owner_name(ancestor, method)
            .iter()
            .any(|fact| {
                ancestor_chain.iter().any(|chain_fqn| {
                    chain_fqn.namespace_parts() == fact.owner.namespace_parts()
                        && chain_fqn.namespace_kind() == fact.owner.namespace_kind()
                }) && effective_method_visibility_for_chain(engine, ancestor_chain, fact, method).0
                    != MethodVisibility::Public
            })
    })
}

pub(in crate::engine) fn effective_method_visibility_for_chain(
    engine: &crate::engine::Project,
    ancestor_chain: &[FullyQualifiedName],
    fact: &crate::core::MethodFact,
    method: &RubyMethod,
) -> (MethodVisibility, FullyQualifiedName) {
    if let Some(override_fact) =
        method_visibility_override_for_chain(engine, ancestor_chain, &fact.owner, method)
    {
        return (override_fact.visibility, override_fact.owner);
    }
    (fact.visibility, fact.owner.clone())
}

fn method_visibility_override_for_chain(
    engine: &crate::engine::Project,
    ancestor_chain: &[FullyQualifiedName],
    method_owner: &FullyQualifiedName,
    method: &RubyMethod,
) -> Option<crate::core::MethodVisibilityOverrideFact> {
    for ancestor in ancestor_chain {
        let mut overrides = engine
            .view()
            .method_visibility_overrides_matching_owner_name(ancestor, method);
        overrides.sort_by_key(|fact| {
            (
                fact.range.file_id,
                fact.range.start_byte,
                fact.range.end_byte,
            )
        });
        if let Some(override_fact) = overrides.pop() {
            return Some(override_fact);
        }
        if ancestor.namespace_parts() == method_owner.namespace_parts()
            && ancestor.namespace_kind() == method_owner.namespace_kind()
        {
            break;
        }
    }
    None
}

pub(super) fn global_visibility_override_for_method_owner(
    engine: &crate::engine::Project,
    method_owner: &FullyQualifiedName,
    method: &RubyMethod,
) -> Option<crate::core::MethodVisibilityOverrideFact> {
    let mut public_overrides = Vec::new();
    let mut non_public_overrides = Vec::new();
    for override_fact in engine.view().method_visibility_overrides_named(*method) {
        if !method_lookup_chain(engine, &override_fact.owner)
            .iter()
            .any(|ancestor| {
                ancestor.namespace_parts() == method_owner.namespace_parts()
                    && ancestor.namespace_kind() == method_owner.namespace_kind()
            })
        {
            continue;
        }
        if override_fact.visibility == MethodVisibility::Public {
            public_overrides.push(override_fact);
        } else {
            non_public_overrides.push(override_fact);
        }
    }
    let sort_key = |fact: &crate::core::MethodVisibilityOverrideFact| {
        (
            fact.range.file_id,
            fact.range.start_byte,
            fact.range.end_byte,
        )
    };
    public_overrides.sort_by_key(sort_key);
    non_public_overrides.sort_by_key(sort_key);
    non_public_overrides
        .pop()
        .or_else(|| public_overrides.pop())
}

pub(super) fn global_visibility_override_for_method_owner_matching(
    engine: &crate::engine::Project,
    method_owner: &FullyQualifiedName,
    method: &RubyMethod,
    visibility: MethodVisibility,
) -> Option<crate::core::MethodVisibilityOverrideFact> {
    let mut overrides = engine
        .view()
        .method_visibility_overrides_named(*method)
        .filter(|override_fact| {
            override_fact.visibility == visibility
                && method_lookup_chain(engine, &override_fact.owner)
                    .iter()
                    .any(|ancestor| {
                        ancestor.namespace_parts() == method_owner.namespace_parts()
                            && ancestor.namespace_kind() == method_owner.namespace_kind()
                    })
        })
        .collect::<Vec<_>>();
    overrides.sort_by_key(|fact| {
        (
            fact.range.file_id,
            fact.range.start_byte,
            fact.range.end_byte,
        )
    });
    overrides.pop()
}

fn method_visibility_allowed(
    engine: &crate::engine::Project,
    visibility: MethodVisibility,
    owner: &FullyQualifiedName,
    allow_private: bool,
    protected_caller: Option<&FullyQualifiedName>,
) -> bool {
    match visibility {
        MethodVisibility::Public => true,
        MethodVisibility::Private => allow_private,
        MethodVisibility::Protected => {
            allow_private
                || protected_caller
                    .is_some_and(|caller| protected_method_visible_from(engine, owner, caller))
        }
    }
}

pub(in crate::engine) fn protected_method_visible_from(
    engine: &crate::engine::Project,
    protected_owner: &FullyQualifiedName,
    caller_namespace: &FullyQualifiedName,
) -> bool {
    method_lookup_chain(engine, caller_namespace)
        .iter()
        .any(|ancestor| {
            ancestor.namespace_parts() == protected_owner.namespace_parts()
                && ancestor.namespace_kind() == protected_owner.namespace_kind()
        })
}

pub(super) fn receiver_only_callee(
    owner: FullyQualifiedName,
    method: &RubyMethod,
) -> ResolvedMethodCallee {
    ResolvedMethodCallee {
        owner,
        method: *method,
        resolution: MethodCalleeResolution::ReceiverOnly,
        definition_ranges: Vec::new(),
    }
}

pub(super) fn method_missing_callee_in_chain(
    engine: &crate::engine::Project,
    ancestor_chain: &[FullyQualifiedName],
) -> Option<ResolvedMethodCallee> {
    let method_missing = method_missing_method();
    let callee = method_callee_in_chain(
        engine,
        ancestor_chain,
        &method_missing,
        MethodCalleeResolution::MethodMissing,
        true,
        None,
    )?;
    if default_basic_object_method_missing_callee(engine, &callee) {
        return None;
    }
    Some(callee)
}

pub(super) fn default_basic_object_method_missing_fact(
    engine: &crate::engine::Project,
    fact: &MethodFact,
) -> bool {
    fact.owner == basic_object_instance_fqn()
        && method_name_from_fact(fact) == method_missing_method()
        && engine.view().file(fact.range.file_id).is_some_and(|file| {
            matches!(
                file.kind,
                crate::core::SourceKind::Stub | crate::core::SourceKind::Signature
            )
        })
}

fn default_basic_object_method_missing_callee(
    engine: &crate::engine::Project,
    callee: &ResolvedMethodCallee,
) -> bool {
    callee.owner == basic_object_instance_fqn()
        && !callee.definition_ranges.is_empty()
        && callee.definition_ranges.iter().all(|range| {
            engine.view().file(range.file_id).is_some_and(|file| {
                matches!(
                    file.kind,
                    crate::core::SourceKind::Stub | crate::core::SourceKind::Signature
                )
            })
        })
}

fn basic_object_instance_fqn() -> FullyQualifiedName {
    FullyQualifiedName::namespace_with_kind(
        vec![RubyConstant::new("BasicObject").expect_invariant(
            "`BasicObject` is not a valid Ruby constant",
            "ruby core class names must be valid constants",
            "update RubyConstant validation to accept core Ruby class names",
        )],
        crate::core::NamespaceKind::Instance,
    )
}

pub(super) fn method_callee_after_owner(
    engine: &crate::engine::Project,
    ancestor_chain: &[FullyQualifiedName],
    owner: &FullyQualifiedName,
    method: &RubyMethod,
) -> Option<ResolvedMethodCallee> {
    let mut seen_owner = false;
    for ancestor in ancestor_chain {
        if !seen_owner {
            seen_owner = ancestor == owner;
            continue;
        }

        let candidate_chain = std::slice::from_ref(ancestor);
        if let Some(callee) = method_callee_in_chain(
            engine,
            candidate_chain,
            method,
            MethodCalleeResolution::Exact,
            true,
            None,
        ) {
            return Some(callee);
        }
    }

    None
}

pub(in crate::engine) fn method_missing_method() -> RubyMethod {
    RubyMethod::new("method_missing").expect_invariant(
        "`method_missing` is not a valid Ruby method name",
        "ruby's fallback dispatch method must be representable",
        "update RubyMethod validation to accept core Ruby method names",
    )
}

pub(in crate::engine) fn chain_has_custom_method_missing(
    engine: &crate::engine::Project,
    ancestor_chain: &[FullyQualifiedName],
) -> bool {
    let method = method_missing_method();
    let basic_object = basic_object_instance_fqn();
    ancestor_chain.iter().any(|owner| {
        *owner != basic_object
            && !matches!(
                engine.effective_method_fact_matching_owner_name(owner, &method),
                EffectiveMethodFactMatch::Missing
            )
    })
}
