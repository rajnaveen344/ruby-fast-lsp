//! Engine-owned reads of symbol, method, type, reference, and diagnostic facts.

use std::collections::HashSet;

use crate::core::names::fqn_id::FqnId;
use crate::core::storage::method_store::StoredMethodFactMatch;
use crate::core::storage::reference_store::ReferenceCandidateStore;
use crate::core::storage::reference_store::ReferenceStore;
use crate::core::storage::symbol_store::SymbolStore;
use crate::core::storage::type_store::TypeStore;
use crate::core::{
    DiagnosticFact, ExecutionContextFact, FullyQualifiedName, MethodAvailability, MethodFact,
    MethodVisibilityOverrideFact, ReferenceFact, RubyMethod, SourceFileId, SymbolFact, TypeFact,
    TypeResolution, TypeSubject,
};

use super::AnalysisEngine;

pub(in crate::engine) enum EffectiveMethodFactMatch {
    Missing,
    Unique(MethodFact),
    Ambiguous,
}

impl AnalysisEngine {
    pub fn execution_context_at(
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

    pub fn type_at(
        &self,
        subject: &TypeSubject,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> TypeResolution {
        self.facts.types.type_at(subject, file_id, byte_offset)
    }

    pub fn type_facts_for(&self, subject: &TypeSubject) -> Vec<TypeFact> {
        self.facts.types.facts_for(subject)
    }

    pub fn symbol_facts_for(&self, fqn: &FullyQualifiedName) -> Vec<SymbolFact> {
        let Some(fqn_id) = self.names.fqn_id(fqn) else {
            return Vec::new();
        };
        self.facts
            .definitions
            .symbols
            .facts_for(fqn_id)
            .into_iter()
            .map(|fact| self.expand_symbol_fact(fact))
            .collect()
    }

    pub fn all_symbol_facts(&self) -> Vec<SymbolFact> {
        self.facts
            .definitions
            .symbols
            .all_facts()
            .into_iter()
            .map(|fact| self.expand_symbol_fact(fact))
            .collect()
    }

    pub fn symbol_facts_in_file(&self, file_id: SourceFileId) -> Vec<SymbolFact> {
        self.facts
            .definitions
            .symbols
            .facts_in_file(file_id)
            .into_iter()
            .map(|fact| self.expand_symbol_fact(fact))
            .collect()
    }

    pub fn reference_facts_for(&self, target: &FullyQualifiedName) -> &[ReferenceFact] {
        let Some(target_id) = self.names.fqn_id(target) else {
            return &[];
        };
        self.facts.references.resolved.facts_for(target_id)
    }
}

impl AnalysisEngine {
    pub fn method_facts_for(&self, fqn: &FullyQualifiedName) -> Vec<MethodFact> {
        let Some(fqn_id) = self.names.fqn_id(fqn) else {
            return Vec::new();
        };
        self.facts
            .definitions
            .methods
            .facts_for(fqn_id)
            .into_iter()
            .map(|fact| self.expand_method_fact(fact))
            .collect()
    }

    pub fn all_method_facts(&self) -> Vec<MethodFact> {
        self.facts
            .definitions
            .methods
            .all_facts()
            .into_iter()
            .map(|fact| self.expand_method_fact(fact))
            .collect()
    }

    pub fn method_facts_matching_owner(
        &self,
        owner: &FullyQualifiedName,
        partial: &str,
    ) -> Vec<MethodFact> {
        let Some(owner_id) = self.names.fqn_id(owner) else {
            return Vec::new();
        };
        let mut facts = self
            .facts
            .definitions
            .methods
            .facts_matching_owner(owner_id, partial)
            .into_iter()
            .map(|fact| self.expand_method_fact(fact))
            .collect::<Vec<_>>();
        retain_effective_method_availability(&mut facts);
        facts
    }

    pub fn method_facts_matching_owner_name(
        &self,
        owner: &FullyQualifiedName,
        method: &crate::core::RubyMethod,
    ) -> Vec<MethodFact> {
        let Some(owner_id) = self.names.fqn_id(owner) else {
            return Vec::new();
        };
        let mut facts = self
            .facts
            .definitions
            .methods
            .facts_matching_owner_name(owner_id, method)
            .into_iter()
            .map(|fact| self.expand_method_fact(fact))
            .collect::<Vec<_>>();
        retain_effective_method_availability(&mut facts);
        facts
    }

    pub(in crate::engine) fn method_absence_contract_matches_owner_name(
        &self,
        owner: &FullyQualifiedName,
        method: &crate::core::RubyMethod,
    ) -> bool {
        let Some(owner_id) = self.names.fqn_id(owner) else {
            return false;
        };
        self.facts
            .definitions
            .methods
            .facts_matching_owner_name(owner_id, method)
            .iter()
            .any(|fact| matches!(fact.availability, MethodAvailability::Absent { .. }))
    }

    pub(in crate::engine) fn effective_method_fact_matching_owner_name(
        &self,
        owner: &FullyQualifiedName,
        method: &crate::core::RubyMethod,
    ) -> EffectiveMethodFactMatch {
        let Some(owner_id) = self.names.fqn_id(owner) else {
            return EffectiveMethodFactMatch::Missing;
        };
        self.effective_method_fact_matching_owner_id(owner_id, method)
    }

    pub(in crate::engine) fn effective_method_fact_matching_owner_id(
        &self,
        owner_id: FqnId,
        method: &crate::core::RubyMethod,
    ) -> EffectiveMethodFactMatch {
        match self
            .facts
            .definitions
            .methods
            .effective_fact_matching_owner_name(owner_id, method)
        {
            StoredMethodFactMatch::Missing => EffectiveMethodFactMatch::Missing,
            StoredMethodFactMatch::Unique(fact) => {
                EffectiveMethodFactMatch::Unique(self.expand_method_fact(fact.clone()))
            }
            StoredMethodFactMatch::Ambiguous => EffectiveMethodFactMatch::Ambiguous,
        }
    }

    pub fn method_visibility_overrides_matching_owner_name(
        &self,
        owner: &FullyQualifiedName,
        method: &crate::core::RubyMethod,
    ) -> Vec<MethodVisibilityOverrideFact> {
        self.method_visibility_overrides
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
        self.method_visibility_overrides
            .iter()
            .filter(|fact| fact.range.file_id == file_id)
            .cloned()
            .collect()
    }

    pub fn all_method_visibility_overrides(&self) -> Vec<MethodVisibilityOverrideFact> {
        self.method_visibility_overrides.clone()
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

    pub(in crate::engine) fn ruby_method_names_for_owner_id(
        &self,
        owner: FqnId,
    ) -> Vec<RubyMethod> {
        self.facts
            .definitions
            .methods
            .ruby_method_names_for_owner(owner)
    }

    pub fn method_facts_in_file(&self, file_id: SourceFileId) -> Vec<MethodFact> {
        self.facts
            .definitions
            .methods
            .facts_in_file(file_id)
            .into_iter()
            .map(|fact| self.expand_method_fact(fact))
            .collect()
    }
}

impl AnalysisEngine {
    pub fn has_symbol_facts(&self, fqn: &FullyQualifiedName) -> bool {
        let Some(fqn_id) = self.names.fqn_id(fqn) else {
            return false;
        };
        self.facts.definitions.symbols.has_facts(fqn_id)
    }
}

impl AnalysisEngine {
    pub fn diagnostic_facts_in_file(&self, file_id: SourceFileId) -> Vec<DiagnosticFact> {
        self.diagnostics.facts_in_file(file_id)
    }
}

impl AnalysisEngine {
    pub fn all_diagnostic_facts(&self) -> Vec<DiagnosticFact> {
        self.diagnostics.all_facts()
    }
}

impl AnalysisEngine {
    pub(crate) fn reference_store(&self) -> &ReferenceStore {
        &self.facts.references.resolved
    }

    pub(crate) fn symbol_store(&self) -> &SymbolStore {
        &self.facts.definitions.symbols
    }

    pub(crate) fn type_store(&self) -> &TypeStore {
        &self.facts.types
    }

    pub(crate) fn reference_candidate_store(&self) -> &ReferenceCandidateStore {
        &self.facts.references.candidates
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

fn effective_method_name(fact: &MethodFact) -> crate::core::RubyMethod {
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
