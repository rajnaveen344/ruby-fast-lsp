//! `DeclIndex`: the engine's file-owned declarations. It stores interned
//! symbol and method facts, method visibility overrides, and extension
//! execution contexts, and answers the effective-method reads built on them.

use crate::invariant::ExpectInvariant;
use std::collections::{HashMap, HashSet};
use std::convert::Infallible;

use crate::core::names::fqn_id::FqnId;
use crate::core::storage::memory_estimate::{
    fqn_heap_bytes, map_table_bytes, string_heap_bytes, vec_payload_bytes,
};
use crate::core::storage::method_store::{MethodStore, StoredMethodFact, StoredMethodFactMatch};
use crate::core::storage::symbol_store::{StoredSymbolFact, SymbolStore};
use crate::core::{
    ExecutionContextFact, FullyQualifiedName, MethodAvailability, MethodFact,
    MethodVisibilityOverrideFact, RubyMethod, SourceFileId, SymbolFact,
};

use super::names::Names;
use super::Project;
use crate::engine::lookup::MethodAnswer;
use crate::engine::View;

/// The effective declaration of one method on one owner. A single owner has
/// no unresolved lookup edges, so absence here is proven, never unknown.
pub(in crate::engine) type EffectiveMethodFactMatch = MethodAnswer<MethodFact, Infallible>;

#[derive(Debug, Clone, Default)]
pub(in crate::engine) struct DeclIndex {
    symbols: SymbolStore,
    methods: MethodStore,
    method_visibility_overrides: Vec<MethodVisibilityOverrideFact>,
    execution_contexts: HashMap<SourceFileId, Vec<ExecutionContextFact>>,
}

impl DeclIndex {
    /// Intern and replace one file's symbols, methods, visibility overrides,
    /// and execution contexts.
    pub(in crate::engine) fn replace_file(
        &mut self,
        names: &mut Names,
        file_id: SourceFileId,
        symbols: Vec<SymbolFact>,
        methods: Vec<MethodFact>,
        method_visibility_overrides: Vec<MethodVisibilityOverrideFact>,
        execution_contexts: Vec<ExecutionContextFact>,
    ) {
        let symbols = intern_symbol_facts(names, symbols);
        self.symbols.replace_file(file_id, symbols);
        let methods = intern_method_facts(names, methods);
        self.methods.replace_file(file_id, methods);
        self.method_visibility_overrides
            .retain(|fact| fact.range.file_id != file_id);
        self.method_visibility_overrides
            .extend(method_visibility_overrides);
        self.replace_execution_contexts(file_id, execution_contexts);
    }

    /// Drop one file's symbols, methods, visibility overrides, and execution
    /// contexts.
    pub(in crate::engine) fn remove_file(&mut self, file_id: SourceFileId) {
        self.symbols.remove_file(file_id);
        self.methods.remove_file(file_id);
        self.method_visibility_overrides
            .retain(|fact| fact.range.file_id != file_id);
        self.execution_contexts.remove(&file_id);
    }

    fn replace_execution_contexts(
        &mut self,
        file_id: SourceFileId,
        mut contexts: Vec<ExecutionContextFact>,
    ) {
        for context in &contexts {
            invariant_eq!(
                context.range.file_id,
                file_id,
                what = "execution context range belongs to a different file",
                why = "FileAnalysis replacement must be file-local",
                fix = "construct execution context ranges from the owning RubyDocument",
            );
            invariant!(
                context.lexical_namespace.namespace_kind().is_some()
                    && context.implicit_receiver.namespace_kind().is_some()
                    && context.method_definition_owner.namespace_kind().is_some(),
                what = "execution context contains a non-namespace semantic target",
                why = "receiver and definition ownership require namespace identities",
                fix = "validate and convert extension targets before engine ingestion",
            );
            invariant!(
                !context.extension_id.is_empty(),
                what = "execution context has empty extension provenance",
                why = "generated runtime semantics must remain attributable",
                fix = "retain the validated manifest ID in ExecutionContextFact",
            );
        }
        contexts.sort_by_key(|context| {
            (
                context.range.start_byte,
                std::cmp::Reverse(context.range.end_byte),
                context.extension_id.clone(),
            )
        });
        for pair in contexts.windows(2) {
            invariant!(
                pair[0].range != pair[1].range,
                what = "multiple execution contexts own the same block range",
                why = "extension context conflicts must be resolved before engine ingestion",
                fix = "deterministically reject incompatible contexts at the host boundary",
            );
        }
        if contexts.is_empty() {
            self.execution_contexts.remove(&file_id);
        } else {
            self.execution_contexts.insert(file_id, contexts);
        }
    }

    fn execution_context_at(
        &self,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<&ExecutionContextFact> {
        self.execution_contexts
            .get(&file_id)?
            .iter()
            .filter(|context| context.range.contains_offset(file_id, byte_offset))
            .min_by_key(|context| context.range.end_byte - context.range.start_byte)
    }

    /// Every file's execution contexts, in the store's iteration order.
    pub(in crate::engine) fn execution_contexts_by_file(
        &self,
    ) -> impl Iterator<Item = (SourceFileId, &[ExecutionContextFact])> {
        self.execution_contexts
            .iter()
            .map(|(file_id, contexts)| (*file_id, contexts.as_slice()))
    }

    pub(in crate::engine) fn method_visibility_overrides(&self) -> &[MethodVisibilityOverrideFact] {
        &self.method_visibility_overrides
    }

    fn symbol_facts_for(&self, names: &Names, fqn: &FullyQualifiedName) -> Vec<SymbolFact> {
        let Some(fqn_id) = names.fqn_id(fqn) else {
            return Vec::new();
        };
        expand_symbol_facts(names, self.symbols.facts_for(fqn_id))
    }

    pub(in crate::engine) fn has_symbol_facts(
        &self,
        names: &Names,
        fqn: &FullyQualifiedName,
    ) -> bool {
        let Some(fqn_id) = names.fqn_id(fqn) else {
            return false;
        };
        self.symbols.has_facts(fqn_id)
    }

    pub(in crate::engine) fn declares_namespace(
        &self,
        names: &Names,
        fqn: &FullyQualifiedName,
    ) -> bool {
        names
            .fqn_id(fqn)
            .is_some_and(|fqn_id| self.symbols.declares_namespace(fqn_id))
    }

    pub(in crate::engine) fn known_namespace_fqns(
        &self,
        names: &Names,
    ) -> HashSet<FullyQualifiedName> {
        self.symbols
            .known_namespace_fqns()
            .into_iter()
            .filter_map(|id| names.fqn(id).cloned())
            .collect()
    }

    fn method_facts_for(&self, names: &Names, fqn: &FullyQualifiedName) -> Vec<MethodFact> {
        let Some(fqn_id) = names.fqn_id(fqn) else {
            return Vec::new();
        };
        expand_method_facts(names, self.methods.facts_for(fqn_id))
    }

    fn method_facts_matching_owner(
        &self,
        names: &Names,
        owner: &FullyQualifiedName,
        partial: &str,
    ) -> Vec<MethodFact> {
        let Some(owner_id) = names.fqn_id(owner) else {
            return Vec::new();
        };
        let mut facts =
            expand_method_facts(names, self.methods.facts_matching_owner(owner_id, partial));
        retain_effective_method_availability(&mut facts);
        facts
    }

    fn method_facts_matching_owner_name(
        &self,
        names: &Names,
        owner: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Vec<MethodFact> {
        let Some(owner_id) = names.fqn_id(owner) else {
            return Vec::new();
        };
        let mut facts = expand_method_facts(
            names,
            self.methods.facts_matching_owner_name(owner_id, method),
        );
        retain_effective_method_availability(&mut facts);
        facts
    }

    fn method_absence_contract_matches_owner_name(
        &self,
        names: &Names,
        owner: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> bool {
        let Some(owner_id) = names.fqn_id(owner) else {
            return false;
        };
        self.methods
            .facts_matching_owner_name(owner_id, method)
            .iter()
            .any(|fact| matches!(fact.availability, MethodAvailability::Absent { .. }))
    }

    fn effective_method_fact_matching_owner_id(
        &self,
        names: &Names,
        owner_id: FqnId,
        method: &RubyMethod,
    ) -> EffectiveMethodFactMatch {
        match self
            .methods
            .effective_fact_matching_owner_name(owner_id, method)
        {
            StoredMethodFactMatch::Missing => EffectiveMethodFactMatch::Missing,
            StoredMethodFactMatch::Unique(fact) => {
                EffectiveMethodFactMatch::Found(expand_method_fact(names, fact.clone()))
            }
            StoredMethodFactMatch::Ambiguous => EffectiveMethodFactMatch::Ambiguous {
                owner: names
                    .fqn(owner_id)
                    .expect_invariant(
                        "an ambiguous method owner id is absent from the name registry",
                        "method stores key owners by ids interned in that same registry",
                        "intern method owners before storing their facts",
                    )
                    .clone(),
                method: *method,
            },
        }
    }

    pub(in crate::engine) fn symbol_count(&self) -> usize {
        self.symbols.fact_count()
    }

    pub(in crate::engine) fn method_count(&self) -> usize {
        self.methods.fact_count()
    }

    pub(in crate::engine) fn symbols_heap_bytes(&self) -> usize {
        self.symbols.estimated_heap_bytes()
    }

    pub(in crate::engine) fn methods_heap_bytes(&self) -> usize {
        self.methods.estimated_heap_bytes() + self.method_visibility_overrides_heap_bytes()
    }

    pub(in crate::engine) fn method_visibility_overrides_heap_bytes(&self) -> usize {
        vec_payload_bytes(&self.method_visibility_overrides)
            + self
                .method_visibility_overrides
                .iter()
                .map(|fact| fqn_heap_bytes(&fact.owner))
                .sum::<usize>()
    }

    pub(in crate::engine) fn execution_contexts_heap_bytes(&self) -> usize {
        map_table_bytes(&self.execution_contexts)
            + self
                .execution_contexts
                .values()
                .map(|contexts| {
                    vec_payload_bytes(contexts)
                        + contexts
                            .iter()
                            .map(|context| {
                                fqn_heap_bytes(&context.lexical_namespace)
                                    + fqn_heap_bytes(&context.implicit_receiver)
                                    + fqn_heap_bytes(&context.method_definition_owner)
                                    + string_heap_bytes(&context.extension_id)
                            })
                            .sum::<usize>()
                })
                .sum::<usize>()
    }

    pub(in crate::engine) fn shrink_to_fit(&mut self) {
        self.symbols.shrink_to_fit();
        self.methods.shrink_to_fit();
        self.method_visibility_overrides.shrink_to_fit();
        self.execution_contexts.shrink_to_fit();
        for contexts in self.execution_contexts.values_mut() {
            contexts.shrink_to_fit();
        }
    }
}

impl Project {
    pub(in crate::engine) fn method_absence_contract_matches_owner_name(
        &self,
        owner: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> bool {
        self.decls
            .method_absence_contract_matches_owner_name(&self.names, owner, method)
    }

    pub(in crate::engine) fn effective_method_fact_matching_owner_name(
        &self,
        owner: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> EffectiveMethodFactMatch {
        let Some(owner_id) = self.names.fqn_id(owner) else {
            return EffectiveMethodFactMatch::Missing;
        };
        self.effective_method_fact_matching_owner_id(owner_id, method)
    }

    pub(in crate::engine) fn effective_method_fact_matching_owner_id(
        &self,
        owner_id: FqnId,
        method: &RubyMethod,
    ) -> EffectiveMethodFactMatch {
        self.decls
            .effective_method_fact_matching_owner_id(&self.names, owner_id, method)
    }

    pub(in crate::engine) fn ruby_method_names_for_owner_id(
        &self,
        owner: FqnId,
    ) -> Vec<RubyMethod> {
        self.decls.methods.ruby_method_names_for_owner(owner)
    }
}

impl<'a> View<'a> {
    pub fn execution_context_at(
        &self,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<&'a ExecutionContextFact> {
        self.engine.decls.execution_context_at(file_id, byte_offset)
    }

    pub fn symbol_facts_for(&self, fqn: &FullyQualifiedName) -> Vec<SymbolFact> {
        self.engine.decls.symbol_facts_for(&self.engine.names, fqn)
    }

    /// Every symbol fact in store order, expanded one at a time.
    pub fn symbol_facts(&self) -> impl Iterator<Item = SymbolFact> + 'a {
        let names = &self.engine.names;
        self.engine
            .decls
            .symbols
            .facts()
            .map(move |fact| expand_symbol_fact(names, *fact))
    }

    pub fn has_symbols(&self) -> bool {
        self.engine.decls.symbols.fact_count() > 0
    }

    pub fn symbol_facts_in_file(&self, file_id: SourceFileId) -> Vec<SymbolFact> {
        let facts = self.engine.decls.symbols.facts_in_file(file_id);
        expand_symbol_facts(&self.engine.names, facts)
    }

    pub fn has_symbol_facts(&self, fqn: &FullyQualifiedName) -> bool {
        self.engine.decls.has_symbol_facts(&self.engine.names, fqn)
    }

    pub fn method_facts_for(&self, fqn: &FullyQualifiedName) -> Vec<MethodFact> {
        self.engine.decls.method_facts_for(&self.engine.names, fqn)
    }

    /// Every method fact in store order, expanded one at a time.
    pub fn method_facts(&self) -> impl Iterator<Item = MethodFact> + 'a {
        self.method_facts_where(|_, _| true)
    }

    /// Method facts named `method` on any owner, in store order.
    pub fn method_facts_named(&self, method: RubyMethod) -> impl Iterator<Item = MethodFact> + 'a {
        self.method_facts_where(move |fact, _| fact.method == Some(method))
    }

    /// Method facts in store order whose stored row passes `keep`. Rows that
    /// fail are never expanded.
    pub(in crate::engine) fn method_facts_where<'p>(
        &self,
        keep: impl Fn(&StoredMethodFact, &Names) -> bool + 'p,
    ) -> impl Iterator<Item = MethodFact> + 'p
    where
        'a: 'p,
    {
        let names = &self.engine.names;
        self.engine
            .decls
            .methods
            .facts()
            .filter(move |fact| keep(fact, names))
            .map(move |fact| expand_method_fact(names, fact.clone()))
    }

    pub fn method_facts_in_file(&self, file_id: SourceFileId) -> Vec<MethodFact> {
        let facts = self.engine.decls.methods.facts_in_file(file_id);
        expand_method_facts(&self.engine.names, facts)
    }

    pub fn method_facts_matching_owner(
        &self,
        owner: &FullyQualifiedName,
        partial: &str,
    ) -> Vec<MethodFact> {
        self.engine
            .decls
            .method_facts_matching_owner(&self.engine.names, owner, partial)
    }

    pub fn method_facts_matching_owner_name(
        &self,
        owner: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Vec<MethodFact> {
        self.engine
            .decls
            .method_facts_matching_owner_name(&self.engine.names, owner, method)
    }

    pub fn method_visibility_overrides_matching_owner_name(
        &self,
        owner: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Vec<MethodVisibilityOverrideFact> {
        self.engine
            .decls
            .method_visibility_overrides
            .iter()
            .filter(|fact| {
                fact.method == *method
                    && fact.owner.namespace_parts() == owner.namespace_parts()
                    && fact.owner.namespace_kind() == owner.namespace_kind()
            })
            .cloned()
            .collect()
    }

    pub fn method_visibility_overrides_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Vec<MethodVisibilityOverrideFact> {
        self.engine
            .decls
            .method_visibility_overrides
            .iter()
            .filter(|fact| fact.range.file_id == file_id)
            .cloned()
            .collect()
    }

    pub fn method_visibility_overrides(&self) -> &'a [MethodVisibilityOverrideFact] {
        &self.engine.decls.method_visibility_overrides
    }

    pub fn method_names_for_owner(&self, owner: &FullyQualifiedName) -> Vec<&'static str> {
        let mut names = self
            .method_facts_matching_owner(owner, "")
            .into_iter()
            .map(|fact| effective_method_name(&fact).as_str())
            .collect::<Vec<_>>();
        names.sort_unstable();
        names.dedup();
        names
    }
}

fn intern_symbol_facts(names: &mut Names, facts: Vec<SymbolFact>) -> Vec<StoredSymbolFact> {
    facts
        .into_iter()
        .map(|fact| {
            let fqn = names.intern_fqn(fact.fqn);
            StoredSymbolFact::new(fqn, fact.kind, fact.range).with_name_range(fact.name_range)
        })
        .collect()
}

fn intern_method_facts(names: &mut Names, facts: Vec<MethodFact>) -> Vec<StoredMethodFact> {
    facts
        .into_iter()
        .map(|fact| {
            let method = match &fact.fqn {
                FullyQualifiedName::Method(_, method) => Some(*method),
                FullyQualifiedName::Namespace(_, _)
                | FullyQualifiedName::Constant(_)
                | FullyQualifiedName::LocalVariable(_)
                | FullyQualifiedName::InstanceVariable(_)
                | FullyQualifiedName::ClassVariable(_)
                | FullyQualifiedName::GlobalVariable(_) => None,
            };
            let fqn = names.intern_fqn(fact.fqn);
            let owner = names.intern_fqn(fact.owner);
            StoredMethodFact {
                fqn,
                owner,
                method,
                range: fact.range,
                name_range: fact.name_range,
                params: fact.params,
                param_facts: fact.param_facts,
                parameter_shape_complete: fact.parameter_shape_complete,
                delegate_receiver: fact.delegate_receiver,
                visibility: fact.visibility,
                availability: fact.availability,
                documentation: fact.documentation,
                return_type_label: fact.return_type_label,
                higher_order: fact.higher_order,
            }
        })
        .collect()
}

fn expand_symbol_facts(names: &Names, facts: Vec<StoredSymbolFact>) -> Vec<SymbolFact> {
    facts
        .into_iter()
        .map(|fact| expand_symbol_fact(names, fact))
        .collect()
}

fn expand_symbol_fact(names: &Names, fact: StoredSymbolFact) -> SymbolFact {
    let fqn = names
        .fqn(fact.fqn)
        .expect_invariant(
            "symbol fact points to missing FQN id",
            "symbol facts must only store interned FQN ids",
            "intern symbol FQNs before inserting facts",
        )
        .clone();
    SymbolFact::new(fqn, fact.kind, fact.range).with_name_range(fact.name_range)
}

fn expand_method_facts(names: &Names, facts: Vec<StoredMethodFact>) -> Vec<MethodFact> {
    facts
        .into_iter()
        .map(|fact| expand_method_fact(names, fact))
        .collect()
}

fn expand_method_fact(names: &Names, fact: StoredMethodFact) -> MethodFact {
    let fqn = names
        .fqn(fact.fqn)
        .expect_invariant(
            "method fact points to missing FQN id",
            "method facts must only store interned FQN ids",
            "intern method FQNs before inserting facts",
        )
        .clone();
    let owner = names
        .fqn(fact.owner)
        .expect_invariant(
            "method fact points to missing owner FQN id",
            "method facts must only store interned owner FQN ids",
            "intern method owners before inserting facts",
        )
        .clone();
    MethodFact {
        fqn,
        owner,
        range: fact.range,
        name_range: fact.name_range,
        params: fact.params,
        param_facts: fact.param_facts,
        parameter_shape_complete: fact.parameter_shape_complete,
        delegate_receiver: fact.delegate_receiver,
        visibility: fact.visibility,
        availability: fact.availability,
        documentation: fact.documentation,
        return_type_label: fact.return_type_label,
        higher_order: fact.higher_order,
    }
}

fn retain_effective_method_availability(facts: &mut Vec<MethodFact>) {
    let absent = facts
        .iter()
        .filter(|fact| matches!(fact.availability, MethodAvailability::Absent { .. }))
        .map(effective_method_name)
        .collect::<HashSet<_>>();
    let unavailable = facts
        .iter()
        .filter(|fact| matches!(fact.availability, MethodAvailability::Unavailable { .. }))
        .map(effective_method_name)
        .collect::<HashSet<_>>();
    facts.retain(|fact| {
        let method = effective_method_name(fact);
        if absent.contains(&method) {
            return false;
        }
        if unavailable.contains(&method) {
            return matches!(fact.availability, MethodAvailability::Unavailable { .. });
        }
        true
    });
}

fn effective_method_name(fact: &MethodFact) -> RubyMethod {
    match &fact.fqn {
        FullyQualifiedName::Method(_, method) => *method,
        FullyQualifiedName::Namespace(_, _)
        | FullyQualifiedName::Constant(_)
        | FullyQualifiedName::LocalVariable(_)
        | FullyQualifiedName::InstanceVariable(_)
        | FullyQualifiedName::ClassVariable(_)
        | FullyQualifiedName::GlobalVariable(_) => unreachable_invariant!(
            what = "method store returned a non-method FQN",
            why = "availability composition is defined only for method identities",
            fix = "construct MethodFact with FullyQualifiedName::Method before engine insertion",
        ),
    }
}
