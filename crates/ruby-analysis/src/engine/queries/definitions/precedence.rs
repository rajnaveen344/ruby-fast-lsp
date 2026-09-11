//! Turn participating Ruby lookup chains into a consistent partial order.
//!
//! Conflicting receiver orders are strongly connected components, hence ties.
//! Never pass a pairwise ancestry comparator to `sort`: it is not a total order.

use std::collections::{BTreeSet, HashMap};

use crate::core::FullyQualifiedName;

/// Each returned layer contains tied target-owner indexes. Earlier layers have
/// lookup priority. Unmentioned/shadowed owners never become destinations.
pub(super) fn layers(
    owners: &[FullyQualifiedName],
    chains: &[Vec<FullyQualifiedName>],
) -> Vec<Vec<usize>> {
    let ids = owners
        .iter()
        .enumerate()
        .map(|(id, owner)| (owner, id))
        .collect::<HashMap<_, _>>();
    let mut edges = vec![BTreeSet::new(); owners.len()];
    for chain in chains {
        let selected = chain
            .iter()
            .filter_map(|owner| ids.get(owner).copied())
            .collect::<Vec<_>>();
        for pair in selected.windows(2) {
            if pair[0] != pair[1] {
                edges[pair[0]].insert(pair[1]);
            }
        }
    }
    let edges = edges
        .into_iter()
        .map(|edges| edges.into_iter().collect::<Vec<_>>())
        .collect::<Vec<_>>();
    let components = components(&edges);
    let mut component_of = vec![0; owners.len()];
    for (id, members) in components.iter().enumerate() {
        for &member in members {
            component_of[member] = id;
        }
    }
    let mut outgoing = vec![BTreeSet::new(); components.len()];
    let mut incoming = vec![0; components.len()];
    for (owner, targets) in edges.iter().enumerate() {
        for &target in targets {
            let (from, to) = (component_of[owner], component_of[target]);
            if from != to && outgoing[from].insert(to) {
                incoming[to] += 1;
            }
        }
    }
    let mut ready = incoming
        .iter()
        .enumerate()
        .filter_map(|(id, &count)| (count == 0).then_some(id))
        .collect::<Vec<_>>();
    let mut layers = Vec::new();
    let mut emitted = 0;
    while !ready.is_empty() {
        let mut layer = Vec::new();
        let mut next = Vec::new();
        for component in ready {
            layer.extend(components[component].iter().copied());
            for &target in &outgoing[component] {
                incoming[target] -= 1;
                if incoming[target] == 0 {
                    next.push(target);
                }
            }
        }
        emitted += layer.len();
        layers.push(layer);
        ready = next;
    }
    assert_eq!(emitted, owners.len(), "INVARIANT VIOLATED: definition ranking lost a target owner. This is a bug because the component graph must be acyclic. Fix: preserve every owner when collapsing conflicting lookup orders.");
    layers
}

/// Iterative Kosaraju traversal keeps deeply nested input off the call stack.
fn components(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let mut reverse = vec![Vec::new(); edges.len()];
    for (from, targets) in edges.iter().enumerate() {
        for &to in targets {
            reverse[to].push(from);
        }
    }
    let mut seen = vec![false; edges.len()];
    let mut finished = Vec::new();
    for root in 0..edges.len() {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let mut stack = vec![(root, 0)];
        while let Some((node, next)) = stack.last_mut() {
            if let Some(&child) = edges[*node].get(*next) {
                *next += 1;
                if !seen[child] {
                    seen[child] = true;
                    stack.push((child, 0));
                }
            } else {
                finished.push(*node);
                stack.pop();
            }
        }
    }
    seen.fill(false);
    let mut result = Vec::new();
    for root in finished.into_iter().rev() {
        if seen[root] {
            continue;
        }
        seen[root] = true;
        let mut stack = vec![root];
        let mut members = Vec::new();
        while let Some(node) = stack.pop() {
            members.push(node);
            for &child in &reverse[node] {
                if !seen[child] {
                    seen[child] = true;
                    stack.push(child);
                }
            }
        }
        result.push(members);
    }
    result
}
