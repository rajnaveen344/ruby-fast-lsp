//! The type of an instance variable, proven from every assignment that can
//! reach it.
//!
//! An object's variable may be assigned by any method the object responds
//! to: its class, every ancestor, and every subclass an instance may really
//! be. The type is proven only when each of those namespaces is fully
//! visible and each assignment found there is itself proven. One unproven
//! assignment, or one namespace this project cannot read completely, leaves
//! the variable unknown.

use std::collections::{HashMap, HashSet, VecDeque};

use std::collections::BTreeMap;

use crate::core::{
    FullyQualifiedName, GraphEdgeKind, GraphNodeKind, InferenceEvidence,
    InstanceVariableDependency, InstanceVariableWrite, NamespaceKind, RubyMethod, RubyType,
    SourceFileId, SourceKind,
};
use crate::engine::View;

use super::{method_lookup_chain, method_lookup_chain_has_unresolved_dependency_from_graph};

/// The most subclasses one variable's proof may span. A larger family keeps
/// the variable unknown.
const MAX_HIERARCHY_CLASSES: usize = 64;

/// One memoising assignment: its file and its number there.
pub(in crate::engine) type MemoSite = (SourceFileId, u32);

/// The proven type of each instance variable a method-return equation
/// returns or was evaluated with; `None` where the project does not prove
/// one. A variable keyed with a memoising assignment is typed from every
/// other assignment.
pub(in crate::engine) type InstanceVariableTypes =
    BTreeMap<InstanceVariableDependency, BTreeMap<Option<MemoSite>, Option<RubyType>>>;

/// Resolve every instance variable the method-return equations in
/// `evidence` rely on against the project as it is now.
pub(in crate::engine) fn resolve_instance_variables<'a>(
    view: &View<'_>,
    evidence: impl Iterator<Item = (&'a SourceFileId, &'a InferenceEvidence)> + Clone,
) -> InstanceVariableTypes {
    let mut by_owner: HashMap<_, Vec<_>> = HashMap::new();
    for (file_id, evidence) in evidence.clone() {
        for write in &evidence.instance_variable_writes {
            by_owner
                .entry(&write.owner)
                .or_default()
                .push((*file_id, write));
        }
    }
    let writes = InstanceVariableWrites { by_owner };
    let mut reach_by_owner: HashMap<&FullyQualifiedName, Option<OwnerReach>> = HashMap::new();
    let mut resolved = InstanceVariableTypes::new();
    for (file_id, evidence) in evidence {
        for equation in &evidence.method_return_equations {
            let observed = equation
                .instance_variable_observations()
                .iter()
                .map(|observation| {
                    let except = observation.except_memo.map(|memo| (*file_id, memo));
                    (&observation.variable, except)
                });
            let returned = equation
                .instance_variable_dependencies()
                .iter()
                .map(|variable| (variable, None));
            for (variable, except) in returned.chain(observed) {
                if resolved
                    .get(variable)
                    .is_some_and(|types| types.contains_key(&except))
                {
                    continue;
                }
                let reach = reach_by_owner
                    .entry(&variable.owner)
                    .or_insert_with(|| owner_reach(view, &variable.owner));
                let ruby_type = reach
                    .as_ref()
                    .and_then(|reach| instance_variable_type(&writes, reach, variable, except));
                resolved
                    .entry(variable.clone())
                    .or_default()
                    .insert(except, ruby_type);
            }
        }
    }
    resolved
}

/// Every recorded assignment in the project, grouped by owning namespace.
struct InstanceVariableWrites<'a> {
    by_owner: HashMap<&'a FullyQualifiedName, Vec<(SourceFileId, &'a InstanceVariableWrite)>>,
}

/// Everything that can assign a variable of one class's instances, shared by
/// every variable the class owns.
struct OwnerReach {
    /// Each namespace an instance may take methods from, all fully visible.
    namespaces: Vec<FullyQualifiedName>,
    /// Per class an instance may really be: the namespace whose constructor
    /// builds it, or `None` when no single constructor is proven to run.
    constructors: Vec<Option<FullyQualifiedName>>,
}

/// The reach of `owner`, or `None` when part of it cannot be read completely.
fn owner_reach(view: &View<'_>, owner: &FullyQualifiedName) -> Option<OwnerReach> {
    if owner.namespace_kind() != Some(NamespaceKind::Instance)
        || view.namespace_node_kind(owner) != Some(GraphNodeKind::Class)
    {
        return None;
    }
    let classes = class_family(view, owner)?;

    let mut namespaces = Vec::new();
    let mut seen = HashSet::new();
    let mut constructors = Vec::with_capacity(classes.len());
    for class in &classes {
        if method_lookup_chain_has_unresolved_dependency_from_graph(view.engine, class) {
            return None;
        }
        let chain = method_lookup_chain(view.engine, class);
        constructors.push(constructor_namespace(view, &chain));
        for namespace in chain {
            if seen.insert(namespace.clone()) {
                namespaces.push(namespace);
            }
        }
    }
    namespaces
        .iter()
        .all(|namespace| namespace_is_fully_visible(view, namespace))
        .then_some(OwnerReach {
            namespaces,
            constructors,
        })
}

/// The proven type of `dependency`'s variable as a reader method returns it.
/// With `except`, the type every assignment but that memoising one leaves:
/// a variable nothing else assigns holds nil until the memo runs.
fn instance_variable_type(
    writes: &InstanceVariableWrites<'_>,
    reach: &OwnerReach,
    dependency: &InstanceVariableDependency,
    except: Option<MemoSite>,
) -> Option<RubyType> {
    let is_excepted = |file_id: SourceFileId, write: &InstanceVariableWrite| {
        except.is_some() && write.memo.map(|memo| (file_id, memo)) == except
    };
    let mut members = Vec::new();
    for namespace in &reach.namespaces {
        for (file_id, write) in writes.by_owner.get(namespace).into_iter().flatten() {
            if is_excepted(*file_id, write) {
                continue;
            }
            if write.name.as_str() == InstanceVariableWrite::ANY_NAME {
                return None;
            }
            if write.name != dependency.name {
                continue;
            }
            members.push(write.proven.clone()?);
        }
    }
    if members.is_empty() && except.is_none() {
        return None;
    }
    // The constructor must assign the variable on every completed run.
    let always_assigned = reach.constructors.iter().all(|constructor| {
        constructor.as_ref().is_some_and(|namespace| {
            writes
                .by_owner
                .get(namespace)
                .into_iter()
                .flatten()
                .any(|(file_id, write)| {
                    write.in_constructor
                        && write.name == dependency.name
                        && !is_excepted(*file_id, write)
                })
        })
    });
    if !always_assigned {
        members.push(RubyType::nil_class());
    }
    Some(RubyType::union(members))
}

/// `class` and every class that inherits from it, or `None` past the bound.
fn class_family(view: &View<'_>, class: &FullyQualifiedName) -> Option<Vec<FullyQualifiedName>> {
    let mut family = vec![class.clone()];
    let mut seen = HashSet::from([class.clone()]);
    let mut queue = VecDeque::from([class.clone()]);
    while let Some(current) = queue.pop_front() {
        for edge in view.graph_edges_to(&current) {
            if edge.kind != GraphEdgeKind::Superclass || !seen.insert(edge.source.clone()) {
                continue;
            }
            if family.len() == MAX_HIERARCHY_CLASSES {
                return None;
            }
            family.push(edge.source.clone());
            queue.push_back(edge.source);
        }
    }
    Some(family)
}

/// Whether every source that can add methods to `namespace` was read by this
/// project. Runtime-provided roots are described completely by their stubs.
fn namespace_is_fully_visible(view: &View<'_>, namespace: &FullyQualifiedName) -> bool {
    let nodes = view.graph_nodes_for(namespace);
    !nodes.is_empty()
        && nodes.iter().all(|node| {
            view.file(node.range.file_id)
                .is_some_and(|file| match file.kind {
                    SourceKind::Project | SourceKind::Signature | SourceKind::Stub => true,
                    SourceKind::Excluded
                    | SourceKind::External
                    | SourceKind::Stdlib
                    | SourceKind::Gem => false,
                })
        })
}

/// The namespace whose constructor builds an instance of the class owning
/// `chain`, or `None` when no single constructor is proven to run.
fn constructor_namespace(
    view: &View<'_>,
    chain: &[FullyQualifiedName],
) -> Option<FullyQualifiedName> {
    let (Ok(new), Ok(initialize)) = (RubyMethod::new("new"), RubyMethod::new("initialize")) else {
        return None;
    };
    let singleton = |namespace: &FullyQualifiedName| {
        FullyQualifiedName::namespace_with_kind(
            namespace.namespace_parts(),
            NamespaceKind::Singleton,
        )
    };
    // A replaced `new` may build instances without running the constructor.
    let replaces_new = chain.iter().any(|namespace| {
        view.method_facts_matching_owner_name(&singleton(namespace), &new)
            .into_iter()
            .any(|fact| !fact.is_normalized_from_initialize())
    });
    if replaces_new {
        return None;
    }
    for namespace in chain {
        let generated = view
            .method_facts_matching_owner_name(&singleton(namespace), &new)
            .into_iter()
            .filter(|fact| fact.is_normalized_from_initialize())
            .count();
        // A module's constructor may stay an instance method of the module.
        let constructors = generated.max(
            view.method_facts_matching_owner_name(namespace, &initialize)
                .len(),
        );
        match constructors {
            0 => continue,
            1 => return Some(namespace.clone()),
            // Reopened constructors: the last one loaded wins, and load
            // order is not proven.
            _ => return None,
        }
    }
    None
}
