//! Direct-fact collection and precise merging of visitor and inferred type facts.

use crate::invariant::ExpectInvariant;
use ruby_analysis::core::{
    FullyQualifiedName, RubyType, SymbolKind as AnalysisSymbolKind, TextRange, TypeFact,
    TypeProvenance, TypeSubject,
};
use ruby_analysis::engine::{AnalysisEngine, AnalysisQuery};
use ruby_analysis::indexer::AnalysisIndexer;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub(super) fn collect_direct_facts(
    analysis_engine: &Arc<parking_lot::RwLock<AnalysisEngine>>,
    node: &ruby_prism::Node<'_>,
    content: &str,
    file_id: ruby_analysis::core::SourceFileId,
    known_namespaces: Option<&HashSet<FullyQualifiedName>>,
) -> ruby_analysis::indexer::AnalysisIndex {
    let known_namespaces = known_namespaces
        .cloned()
        .unwrap_or_else(|| collect_known_namespaces(analysis_engine));
    let known_constant_types = collect_known_constant_types(analysis_engine, file_id);
    AnalysisIndexer::with_known_semantics(file_id, known_namespaces, known_constant_types)
        .index_node_with_source(node, content)
}

pub(super) fn merge_execution_context_direct_facts(
    extension_aware: &ruby_analysis::indexer::AnalysisIndex,
    merged: &mut ruby_analysis::indexer::AnalysisIndex,
) {
    let generated_methods = extension_aware
        .methods
        .iter()
        .filter(|fact| fact.owner.has_generated_owner())
        .cloned()
        .collect::<Vec<_>>();
    for generated in generated_methods {
        merged.methods.retain(|fact| {
            fact.owner.has_generated_owner()
                || fact.range != generated.range
                || fact.fqn.name() != generated.fqn.name()
        });
        if !merged.methods.contains(&generated) {
            merged.methods.push(generated);
        }
    }

    let generated_symbols = extension_aware
        .symbols
        .iter()
        .filter(|fact| fact.kind == AnalysisSymbolKind::Method && fact.fqn.has_generated_owner())
        .cloned()
        .collect::<Vec<_>>();
    for generated in generated_symbols {
        merged.symbols.retain(|fact| {
            fact.fqn.has_generated_owner()
                || fact.kind != AnalysisSymbolKind::Method
                || fact.range != generated.range
                || fact.fqn.name() != generated.fqn.name()
        });
        if !merged.symbols.contains(&generated) {
            merged.symbols.push(generated);
        }
    }

    for generated in extension_aware
        .method_visibility_overrides
        .iter()
        .filter(|fact| fact.owner.has_generated_owner())
    {
        merged.method_visibility_overrides.retain(|fact| {
            fact.owner.has_generated_owner()
                || fact.range != generated.range
                || fact.method != generated.method
        });
        if !merged.method_visibility_overrides.contains(generated) {
            merged.method_visibility_overrides.push(generated.clone());
        }
    }

    for generated in extension_aware
        .graph_nodes
        .iter()
        .filter(|fact| fact.fqn.has_generated_owner())
    {
        if !merged.graph_nodes.contains(generated) {
            merged.graph_nodes.push(generated.clone());
        }
    }
    for generated in extension_aware
        .graph_edges
        .iter()
        .filter(|fact| fact.source.has_generated_owner() || fact.target.has_generated_owner())
    {
        if !merged.graph_edges.contains(generated) {
            merged.graph_edges.push(generated.clone());
        }
    }
}

pub(super) fn merge_runtime_direct_facts(
    runtime_aware: &ruby_analysis::indexer::AnalysisIndex,
    merged: &mut ruby_analysis::indexer::AnalysisIndex,
) {
    let runtime_types = runtime_aware
        .types
        .iter()
        .filter(|fact| fact.provenance == TypeProvenance::Runtime)
        .cloned()
        .collect::<Vec<_>>();
    let runtime_constants = runtime_types
        .iter()
        .filter_map(|fact| match &fact.subject {
            TypeSubject::Constant(fqn) => Some(fqn.clone()),
            TypeSubject::Local { .. }
            | TypeSubject::InstanceVariable { .. }
            | TypeSubject::ClassVariable { .. }
            | TypeSubject::GlobalVariable(_)
            | TypeSubject::MethodReturn(_)
            | TypeSubject::Parameter { .. }
            | TypeSubject::Expression(_) => None,
        })
        .collect::<HashSet<_>>();
    let shadowed_runtime_reopenings = merged
        .graph_nodes
        .iter()
        .filter(|fact| {
            runtime_constants
                .iter()
                .any(|constant| constant.namespace_parts() == fact.fqn.namespace_parts())
                && !runtime_aware.graph_nodes.iter().any(|runtime_fact| {
                    runtime_fact.fqn == fact.fqn && runtime_fact.range == fact.range
                })
        })
        .map(|fact| (fact.fqn.clone(), fact.range))
        .collect::<Vec<_>>();
    merged.graph_nodes.retain(|fact| {
        !shadowed_runtime_reopenings
            .iter()
            .any(|(fqn, range)| *fqn == fact.fqn && *range == fact.range)
    });
    merged.symbols.retain(|fact| {
        !shadowed_runtime_reopenings
            .iter()
            .any(|(fqn, range)| *fqn == fact.fqn && *range == fact.range)
    });
    merged.types.retain(|fact| {
        !shadowed_runtime_reopenings.iter().any(|(fqn, range)| {
            fact.range == *range
                && matches!(
                    &fact.subject,
                    TypeSubject::Constant(constant)
                        if constant.namespace_parts() == fqn.namespace_parts()
                )
        })
    });
    merged.graph_edges.retain(|edge| {
        !shadowed_runtime_reopenings.iter().any(|(fqn, range)| {
            edge.source == *fqn
                && edge.range.file_id == range.file_id
                && range.start_byte <= edge.range.start_byte
                && edge.range.end_byte <= range.end_byte
        })
    });
    merged.unresolved_graph_edges.retain(|edge| {
        !shadowed_runtime_reopenings.iter().any(|(fqn, range)| {
            edge.source == *fqn
                && edge.range.file_id == range.file_id
                && range.start_byte <= edge.range.start_byte
                && edge.range.end_byte <= range.end_byte
        })
    });

    let runtime_methods = runtime_types
        .iter()
        .filter_map(|fact| match &fact.subject {
            TypeSubject::MethodReturn(fqn) => Some(fqn.clone()),
            TypeSubject::Constant(_)
            | TypeSubject::Local { .. }
            | TypeSubject::InstanceVariable { .. }
            | TypeSubject::ClassVariable { .. }
            | TypeSubject::GlobalVariable(_)
            | TypeSubject::Parameter { .. }
            | TypeSubject::Expression(_) => None,
        })
        .collect::<HashSet<_>>();
    for method in runtime_aware
        .methods
        .iter()
        .filter(|fact| runtime_methods.contains(&fact.fqn))
    {
        merged
            .methods
            .retain(|fact| fact.range != method.range || fact.fqn.name() != method.fqn.name());
        merged.methods.push(method.clone());
    }
    for symbol in runtime_aware.symbols.iter().filter(|fact| {
        fact.kind == AnalysisSymbolKind::Method && runtime_methods.contains(&fact.fqn)
    }) {
        merged.symbols.retain(|fact| {
            fact.kind != AnalysisSymbolKind::Method
                || fact.range != symbol.range
                || fact.fqn.name() != symbol.fqn.name()
        });
        merged.symbols.push(symbol.clone());
    }
    for symbol in runtime_aware
        .symbols
        .iter()
        .filter(|fact| runtime_constants.contains(&fact.fqn))
    {
        if !merged.symbols.contains(symbol) {
            merged.symbols.push(symbol.clone());
        }
    }
    for runtime_type in runtime_types {
        merged.types.retain(|fact| {
            fact.provenance != TypeProvenance::Runtime
                || fact.range != runtime_type.range
                || fact.subject != runtime_type.subject
        });
        merged.types.push(runtime_type);
    }
}

fn rehome_execution_context_type_facts(extension_aware: &[TypeFact], merged: &mut Vec<TypeFact>) {
    for generated in extension_aware
        .iter()
        .filter(|fact| type_subject_has_generated_owner(&fact.subject))
    {
        merged.retain(|fact| {
            type_subject_has_generated_owner(&fact.subject)
                || fact.range != generated.range
                || !same_type_subject_slot(&fact.subject, &generated.subject)
        });
    }
}

pub(super) fn merge_collected_type_facts(visitor_facts: Vec<TypeFact>, merged: &mut Vec<TypeFact>) {
    rehome_execution_context_type_facts(&visitor_facts, merged);
    merge_precise_visitor_type_facts(visitor_facts, merged);
}

pub(super) fn merge_precise_visitor_type_facts(
    visitor_facts: Vec<TypeFact>,
    merged: &mut Vec<TypeFact>,
) {
    // Subject equality against the whole merged vector is O(n*m) and showed up
    // as TypeSubject PartialEq on the project-file assembly path. Index live
    // slots by subject for this merge only; the map does not outlive assembly.
    let mut slots: Vec<Option<TypeFact>> = merged.drain(..).map(Some).collect();
    let mut indexes_by_subject: HashMap<TypeSubject, Vec<usize>> = HashMap::new();
    for (index, fact) in slots.iter().enumerate() {
        let Some(fact) = fact.as_ref() else {
            continue;
        };
        indexes_by_subject
            .entry(fact.subject.clone())
            .or_default()
            .push(index);
    }

    for visitor_fact in visitor_facts {
        if visitor_fact.provenance != TypeProvenance::Runtime
            && !type_subject_is_assignment_slot(&visitor_fact.subject)
        {
            let same_slot_indexes = live_subject_slot_indexes(
                &slots,
                &indexes_by_subject,
                &visitor_fact.subject,
                |fact| {
                    !type_subject_is_definition_slot(&visitor_fact.subject)
                        || assignment_ranges_identify_same_write(fact.range, visitor_fact.range)
                },
            );

            // Direct indexing publishes a cheap syntax seed before the full
            // flow visitor runs. For one inferred method definition, the
            // visitor is the authoritative producer: it can retain structural
            // shapes and can invalidate a formerly concrete result. Replace
            // only inferred seeds. Declared/runtime facts remain independent
            // contracts and must not be silently overwritten by body flow.
            if matches!(visitor_fact.subject, TypeSubject::MethodReturn(_))
                && visitor_fact.provenance == TypeProvenance::Inferred
                && !same_slot_indexes.is_empty()
                && same_slot_indexes.iter().all(|&index| {
                    slots[index]
                        .as_ref()
                        .expect_invariant(
                            "merge selected a tombstoned type-fact slot",
                            "live_subject_slot_indexes must skip cleared entries",
                            "keep slot liveness and subject indexes in the same merge step",
                        )
                        .provenance
                        == TypeProvenance::Inferred
                })
            {
                tombstone_merged_slots(&mut slots, &same_slot_indexes);
                push_merged_type_fact(&mut slots, &mut indexes_by_subject, visitor_fact);
                continue;
            }

            if same_slot_indexes.is_empty() {
                push_merged_type_fact(&mut slots, &mut indexes_by_subject, visitor_fact);
            }
            continue;
        }
        let matching_slot_indexes =
            live_subject_slot_indexes(&slots, &indexes_by_subject, &visitor_fact.subject, |fact| {
                assignment_ranges_identify_same_write(fact.range, visitor_fact.range)
            });
        if matching_slot_indexes.is_empty() {
            // A different source range is a different write, even when the
            // variable subject is identical. Unknown writes are semantic kill
            // facts: dropping one would let a query resurrect an obsolete
            // concrete type from an earlier assignment.
            push_merged_type_fact(&mut slots, &mut indexes_by_subject, visitor_fact);
            continue;
        }
        if visitor_fact.ruby_type == RubyType::Unknown {
            continue;
        }
        if matching_slot_indexes.iter().any(|&index| {
            slots[index]
                .as_ref()
                .expect_invariant(
                    "merge selected a tombstoned type-fact slot while comparing assignment types",
                    "live_subject_slot_indexes must skip cleared entries",
                    "keep slot liveness and subject indexes in the same merge step",
                )
                .ruby_type
                == visitor_fact.ruby_type
        }) {
            continue;
        }
        if visitor_fact.provenance != TypeProvenance::Runtime {
            // Two non-runtime producers disagreeing about the same write are
            // retained as ambiguity. TypeStore queries must fail closed rather
            // than silently selecting either derivation.
            push_merged_type_fact(&mut slots, &mut indexes_by_subject, visitor_fact);
            continue;
        }
        tombstone_merged_slots(&mut slots, &matching_slot_indexes);
        push_merged_type_fact(&mut slots, &mut indexes_by_subject, visitor_fact);
    }

    *merged = slots.into_iter().flatten().collect();
}

fn live_subject_slot_indexes(
    slots: &[Option<TypeFact>],
    indexes_by_subject: &HashMap<TypeSubject, Vec<usize>>,
    subject: &TypeSubject,
    mut keep: impl FnMut(&TypeFact) -> bool,
) -> Vec<usize> {
    indexes_by_subject
        .get(subject)
        .into_iter()
        .flatten()
        .copied()
        .filter(|&index| slots[index].as_ref().is_some_and(|fact| keep(fact)))
        .collect()
}

fn push_merged_type_fact(
    slots: &mut Vec<Option<TypeFact>>,
    indexes_by_subject: &mut HashMap<TypeSubject, Vec<usize>>,
    fact: TypeFact,
) {
    let index = slots.len();
    indexes_by_subject
        .entry(fact.subject.clone())
        .or_default()
        .push(index);
    slots.push(Some(fact));
}

fn tombstone_merged_slots(slots: &mut [Option<TypeFact>], indexes: &[usize]) {
    for &index in indexes {
        invariant!(
            slots[index].is_some(),
            what = "merge tombstoned a type-fact slot twice",
            why = "each live slot may be replaced at most once per visitor fact",
            fix = "filter live indexes before replacement",
        );
        slots[index] = None;
    }
}

fn type_subject_is_assignment_slot(subject: &TypeSubject) -> bool {
    match subject {
        TypeSubject::Constant(_)
        | TypeSubject::Local { .. }
        | TypeSubject::InstanceVariable { .. }
        | TypeSubject::ClassVariable { .. }
        | TypeSubject::GlobalVariable(_) => true,
        TypeSubject::MethodReturn(_)
        | TypeSubject::Parameter { .. }
        | TypeSubject::Expression(_) => false,
    }
}

fn type_subject_is_definition_slot(subject: &TypeSubject) -> bool {
    match subject {
        TypeSubject::MethodReturn(_) | TypeSubject::Parameter { .. } => true,
        TypeSubject::Constant(_)
        | TypeSubject::Local { .. }
        | TypeSubject::InstanceVariable { .. }
        | TypeSubject::ClassVariable { .. }
        | TypeSubject::GlobalVariable(_)
        | TypeSubject::Expression(_) => false,
    }
}

fn assignment_ranges_identify_same_write(left: TextRange, right: TextRange) -> bool {
    left.file_id == right.file_id
        && ((left.start_byte <= right.start_byte && right.end_byte <= left.end_byte)
            || (right.start_byte <= left.start_byte && left.end_byte <= right.end_byte))
}

fn type_subject_has_generated_owner(subject: &TypeSubject) -> bool {
    match subject {
        TypeSubject::Constant(fqn) | TypeSubject::MethodReturn(fqn) => fqn.has_generated_owner(),
        TypeSubject::InstanceVariable { owner, .. } | TypeSubject::ClassVariable { owner, .. } => {
            owner.has_generated_owner()
        }
        TypeSubject::Parameter { method, .. } => method.has_generated_owner(),
        TypeSubject::Local { .. } | TypeSubject::GlobalVariable(_) | TypeSubject::Expression(_) => {
            false
        }
    }
}

fn same_type_subject_slot(left: &TypeSubject, right: &TypeSubject) -> bool {
    match (left, right) {
        (TypeSubject::Constant(_), TypeSubject::Constant(_))
        | (TypeSubject::MethodReturn(_), TypeSubject::MethodReturn(_)) => true,
        (
            TypeSubject::InstanceVariable { name: left, .. },
            TypeSubject::InstanceVariable { name: right, .. },
        )
        | (
            TypeSubject::ClassVariable { name: left, .. },
            TypeSubject::ClassVariable { name: right, .. },
        ) => left == right,
        (TypeSubject::Parameter { name: left, .. }, TypeSubject::Parameter { name: right, .. }) => {
            left == right
        }
        (TypeSubject::Local { .. }, TypeSubject::Local { .. })
        | (TypeSubject::GlobalVariable(_), TypeSubject::GlobalVariable(_))
        | (TypeSubject::Expression(_), TypeSubject::Expression(_)) => false,
        (TypeSubject::Constant(_), _)
        | (TypeSubject::Local { .. }, _)
        | (TypeSubject::InstanceVariable { .. }, _)
        | (TypeSubject::ClassVariable { .. }, _)
        | (TypeSubject::GlobalVariable(_), _)
        | (TypeSubject::MethodReturn(_), _)
        | (TypeSubject::Parameter { .. }, _)
        | (TypeSubject::Expression(_), _) => false,
    }
}

pub(super) fn collect_known_namespaces(
    analysis_engine: &Arc<parking_lot::RwLock<AnalysisEngine>>,
) -> HashSet<FullyQualifiedName> {
    let engine = analysis_engine.read();
    AnalysisQuery::new(&engine).known_namespace_fqns()
}

fn collect_known_constant_types(
    analysis_engine: &Arc<parking_lot::RwLock<AnalysisEngine>>,
    current_file: ruby_analysis::core::SourceFileId,
) -> HashMap<FullyQualifiedName, RubyType> {
    let engine = analysis_engine.read();
    let mut candidates = HashMap::<FullyQualifiedName, Option<RubyType>>::new();
    for fact in engine
        .query()
        .all_type_facts()
        .into_iter()
        .filter(|fact| fact.range.file_id != current_file)
    {
        let TypeSubject::Constant(constant) = fact.subject else {
            continue;
        };
        match candidates.entry(constant) {
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(Some(fact.ruby_type));
            }
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                if entry.get().as_ref() != Some(&fact.ruby_type) {
                    entry.insert(None);
                }
            }
        }
    }
    candidates
        .into_iter()
        .filter_map(|(constant, ruby_type)| ruby_type.map(|ruby_type| (constant, ruby_type)))
        .collect()
}
