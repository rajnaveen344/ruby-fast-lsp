//! Block parameters proven by the types a method documents it yields.
//!
//! Documentation types the values handed to the block, never the call: the
//! prepared set binds block parameters and leaves the call's result to the
//! method's own return evidence.

use crate::core::{
    FullyQualifiedName, GraphNodeKind, MethodFact, RubyConstant, RubyMethod, RubyType,
    UnknownReason,
};
use crate::inference::higher_order::PreparedCallableSet;
use crate::inference::rbs::has_rbs_class;
use crate::inference::semantics::Semantics;

/// Prepare a block call from documented yield types.
///
/// `UnsupportedCallable` means no reachable definition documents its yields,
/// so other block evidence may answer. Once one does, every definition the
/// call may reach must document the same types; any other outcome leaves the
/// block parameters unknown instead of falling back to weaker evidence.
pub(crate) fn prepare_documented_yield<S: Semantics + ?Sized>(
    query: &S,
    receiver_type: Option<&RubyType>,
    method: &RubyMethod,
    facts: &[MethodFact],
) -> Result<PreparedCallableSet, UnknownReason> {
    if facts.iter().all(|fact| fact.documented_yield().is_none()) {
        return Err(UnknownReason::UnsupportedCallable);
    }
    if let Some(RubyType::Union(members)) = receiver_type {
        // A member with no reachable definition may answer the call through
        // code this proof never saw.
        if members.iter().any(|member| {
            query
                .method_signature_facts_for_type(member, method)
                .is_empty()
        }) {
            return Err(UnknownReason::IncompleteBlockInput);
        }
    }
    let mut proven: Option<Vec<RubyType>> = None;
    for fact in facts {
        let Some(documented) = fact.documented_yield() else {
            return Err(UnknownReason::AmbiguousCallableOverload);
        };
        let lexical_context = fact.owner.namespace_parts();
        let resolved = documented
            .parameter_types
            .iter()
            .map(|documented_type| {
                resolve_documented_type(query, documented_type, &lexical_context)
                    .unwrap_or(RubyType::Unknown)
            })
            .collect::<Vec<_>>();
        match &proven {
            None => proven = Some(resolved),
            Some(first) if *first != resolved => {
                return Err(UnknownReason::AmbiguousCallableOverload);
            }
            Some(_) => {}
        }
    }
    let block_parameter_types = proven.ok_or(UnknownReason::UnsupportedCallable)?;
    if block_parameter_types
        .iter()
        .all(|parameter_type| *parameter_type == RubyType::Unknown)
    {
        return Err(UnknownReason::IncompleteBlockInput);
    }
    Ok(PreparedCallableSet::block_parameters_only(
        block_parameter_types,
    ))
}

/// The documented type with every class name resolved from the documenting
/// method's namespace, or `None` when a name is not a class or module the
/// project defines.
fn resolve_documented_type<S: Semantics + ?Sized>(
    query: &S,
    documented: &RubyType,
    lexical_context: &[RubyConstant],
) -> Option<RubyType> {
    let resolve_all = |types: &[RubyType]| {
        types
            .iter()
            .map(|member| resolve_documented_type(query, member, lexical_context))
            .collect::<Option<Vec<_>>>()
    };
    match documented {
        RubyType::Class(fqn) | RubyType::Module(fqn) => {
            match resolve_documented_name(query, fqn, lexical_context)? {
                (resolved, GraphNodeKind::Class) => Some(RubyType::Class(resolved)),
                (resolved, GraphNodeKind::Module) => Some(RubyType::Module(resolved)),
            }
        }
        RubyType::ClassReference(fqn) | RubyType::ModuleReference(fqn) => {
            match resolve_documented_name(query, fqn, lexical_context)? {
                (resolved, GraphNodeKind::Class) => Some(RubyType::ClassReference(resolved)),
                (resolved, GraphNodeKind::Module) => Some(RubyType::ModuleReference(resolved)),
            }
        }
        RubyType::Array(elements) => resolve_all(elements).map(RubyType::Array),
        RubyType::Union(members) => resolve_all(members).map(RubyType::union),
        RubyType::Hash(keys, values) => {
            Some(RubyType::Hash(resolve_all(keys)?, resolve_all(values)?))
        }
        RubyType::Literal(_) => Some(documented.clone()),
        // Documentation names a shape's fields without resolving their types.
        RubyType::Shape(_) | RubyType::Unknown => None,
    }
}

/// The class or module a documented name refers to from `lexical_context`: a
/// project definition, else a class the language itself defines.
fn resolve_documented_name<S: Semantics + ?Sized>(
    query: &S,
    documented: &FullyQualifiedName,
    lexical_context: &[RubyConstant],
) -> Option<(FullyQualifiedName, GraphNodeKind)> {
    let parts = documented.namespace_parts();
    match query.resolve_constant_in_context(&parts, lexical_context) {
        Some(resolved) => {
            if let Some(kind) = query.namespace_node_kind(&resolved) {
                return Some((resolved, kind));
            }
            // The name reaches a project constant that is not a class or
            // module; it does not also mean the language's class.
            if resolved.namespace_parts() != parts {
                return None;
            }
        }
        None => {}
    }
    let name = parts
        .iter()
        .map(|part| part.to_string())
        .collect::<Vec<_>>()
        .join("::");
    has_rbs_class(&name).then(|| (documented.clone(), GraphNodeKind::Class))
}
