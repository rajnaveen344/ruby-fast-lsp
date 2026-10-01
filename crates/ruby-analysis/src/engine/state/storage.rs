//! Source and name registries, the fact arena, conversion between domain facts
//! and their interned stored representations, and storage compaction.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::mem::size_of;
use std::path::Path;
#[cfg(test)]
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::core::storage::memory_estimate::fqn_heap_bytes;
use crate::core::{
    ConstLookup, ConstLookupId, ConstantPath, DiagnosticCandidateStore, DiagnosticStore, FqnId,
    FullyQualifiedName, GraphEdgeFact, GraphNodeFact, MethodFact, MethodStore, ReferenceCandidate,
    ReferenceCandidateKind, ReferenceCandidateStore, ReferenceStore, RubyConstant, SourceFileId,
    StoredGraphEdgeFact, StoredGraphNodeFact, StoredMethodFact, StoredReferenceCandidate,
    StoredSymbolFact, StoredUnresolvedGraphEdgeFact, SymbolFact, SymbolStore, TextRange, TypeStore,
    UnresolvedGraphEdgeFact,
};

use super::{AnalysisEngine, SourceFile};
use crate::engine::FileIdMap;
use indexmap::IndexSet;

#[derive(Debug, Clone, Default)]
pub(in crate::engine) struct SourceRegistry {
    pub(in crate::engine) ids: FileIdMap,
    pub(in crate::engine) files: HashMap<SourceFileId, SourceFile>,
}

#[derive(Debug, Default)]
pub(in crate::engine) struct NameRegistry {
    state: NameRegistryState,
    #[cfg(test)]
    fqn_lookup_count: AtomicUsize,
}

impl Clone for NameRegistry {
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            #[cfg(test)]
            fqn_lookup_count: AtomicUsize::new(self.fqn_lookup_count.load(Ordering::Relaxed)),
        }
    }
}

impl NameRegistry {
    pub(in crate::engine) fn intern_fqn(&mut self, fqn: FullyQualifiedName) -> FqnId {
        let state = &mut self.state;
        let (index, _) = state.fqns.insert_full(fqn);
        FqnId(u32::try_from(index).expect(
            "INVARIANT VIOLATED: FQN interner exceeded u32 ids. \
                 This is a bug because FqnId stores u32. \
                 Fix: widen FqnId before interning more than u32::MAX names.",
        ))
    }

    pub(in crate::engine) fn fqn_id(&self, fqn: &FullyQualifiedName) -> Option<FqnId> {
        #[cfg(test)]
        self.fqn_lookup_count.fetch_add(1, Ordering::Relaxed);
        self.state
            .fqns
            .get_index_of(fqn)
            .map(|index| FqnId(u32::try_from(index).expect(
                "INVARIANT VIOLATED: FQN interner returned an index above u32. This is a bug because every inserted index is validated before becoming an FqnId. Fix: widen FqnId and its insertion boundary together.",
            )))
    }

    pub(in crate::engine) fn fqn(&self, id: FqnId) -> Option<&FullyQualifiedName> {
        self.state.fqns.get_index(id.0 as usize)
    }

    #[cfg(test)]
    pub(super) fn reset_fqn_lookup_count_for_test(&self) {
        self.fqn_lookup_count.store(0, Ordering::Relaxed);
    }

    #[cfg(test)]
    pub(super) fn fqn_lookup_count_for_test(&self) -> usize {
        self.fqn_lookup_count.load(Ordering::Relaxed)
    }

    pub(in crate::engine) fn intern_const_lookup(&mut self, lookup: ConstLookup) -> ConstLookupId {
        let state = &mut self.state;
        let (index, _) = state.const_lookups.insert_full(lookup);
        ConstLookupId(u32::try_from(index).expect(
            "INVARIANT VIOLATED: constant lookup interner exceeded u32 ids. \
                 This is a bug because ConstLookupId stores u32. \
                 Fix: widen ConstLookupId before interning more than u32::MAX lookups.",
        ))
    }

    pub(in crate::engine) fn const_lookup(&self, id: ConstLookupId) -> Option<&ConstLookup> {
        self.state.const_lookups.get_index(id.0 as usize)
    }

    pub(super) fn estimated_heap_bytes(&self) -> usize {
        let state = &self.state;
        state.fqns.capacity() * (size_of::<FullyQualifiedName>() + size_of::<usize>() + 1)
            + state.fqns.iter().map(fqn_heap_bytes).sum::<usize>()
            + state.const_lookups.capacity() * (size_of::<ConstLookup>() + size_of::<usize>() + 1)
            + state
                .const_lookups
                .iter()
                .map(const_lookup_heap_bytes)
                .sum::<usize>()
    }
}

fn constant_path_heap_bytes(path: &ConstantPath) -> usize {
    if path.spilled() {
        path.capacity() * size_of::<RubyConstant>()
    } else {
        0
    }
}

fn const_lookup_heap_bytes(lookup: &ConstLookup) -> usize {
    constant_path_heap_bytes(&lookup.path)
}

#[derive(Debug, Clone, Default)]
struct NameRegistryState {
    fqns: IndexSet<FullyQualifiedName>,
    const_lookups: IndexSet<ConstLookup>,
}

#[derive(Debug, Clone, Default)]
pub(in crate::engine) struct FactArena {
    pub(in crate::engine) definitions: DefinitionFacts,
    pub(in crate::engine) references: ReferenceFacts,
    pub(in crate::engine) types: TypeStore,
    pub(in crate::engine) diagnostics: DiagnosticFacts,
}

#[derive(Debug, Clone, Default)]
pub(in crate::engine) struct DefinitionFacts {
    pub(in crate::engine) symbols: SymbolStore,
    pub(in crate::engine) methods: MethodStore,
}

#[derive(Debug, Clone, Default)]
pub(in crate::engine) struct ReferenceFacts {
    pub(in crate::engine) candidates: ReferenceCandidateStore,
    pub(in crate::engine) resolved: ReferenceStore,
}

#[derive(Debug, Clone, Default)]
pub(in crate::engine) struct DiagnosticFacts {
    pub(in crate::engine) candidates: DiagnosticCandidateStore,
    pub(in crate::engine) resolved: DiagnosticStore,
}

impl AnalysisEngine {
    pub fn shrink_to_fit(&mut self) {
        self.sources.ids.shrink_to_fit();
        self.sources.files.shrink_to_fit();
        for file in self.sources.files.values_mut() {
            file.path.shrink_to_fit();
            if let Some(source) = &mut file.source {
                source.shrink_to_fit();
            }
            file.line_index.shrink_to_fit();
        }

        self.names.state.fqns.shrink_to_fit();
        self.names.state.const_lookups.shrink_to_fit();

        self.facts.definitions.symbols.shrink_to_fit();
        self.facts.definitions.methods.shrink_to_fit();
        self.facts.types.shrink_to_fit();
        self.graph.shrink_to_fit();
        self.facts.references.candidates.shrink_to_fit();
        self.facts.references.resolved.shrink_to_fit();
        self.facts.diagnostics.candidates.shrink_to_fit();
        self.facts.diagnostics.resolved.shrink_to_fit();
        self.inference_by_file.shrink_to_fit();
        self.call_expression_outcomes_by_file.shrink_to_fit();
        self.local_read_types_by_file.shrink_to_fit();
    }
}

impl AnalysisEngine {
    pub fn file_id(&self, path: impl AsRef<Path>) -> Option<SourceFileId> {
        self.sources.ids.get(path)
    }

    pub fn file(&self, id: SourceFileId) -> Option<&SourceFile> {
        self.sources.files.get(&id)
    }

    pub fn file_content_matches(&self, id: SourceFileId, content: &str) -> bool {
        self.file(id)
            .is_some_and(|file| file.content_hash == source_hash(content))
    }
}

impl AnalysisEngine {
    pub fn files(&self) -> impl Iterator<Item = &SourceFile> {
        self.sources.files.values()
    }
}

impl AnalysisEngine {
    pub(crate) fn fqn_for_id(&self, id: FqnId) -> Option<&FullyQualifiedName> {
        self.names.fqn(id)
    }
}

impl AnalysisEngine {
    pub(in crate::engine) fn expand_interned_fqn(&self, id: FqnId) -> FullyQualifiedName {
        self.names
            .fqn(id)
            .expect(
                "INVARIANT VIOLATED: graph edge points to missing FQN id. \
                 This is a bug because graph edges must only store interned FQN ids. \
                 Fix: intern graph edge FQNs before inserting facts.",
            )
            .clone()
    }
}

impl AnalysisEngine {
    pub fn file_count(&self) -> usize {
        self.sources.files.len()
    }

    pub fn text_range(&self, file_id: SourceFileId, start_byte: u32, end_byte: u32) -> TextRange {
        self.assert_known_file_id(file_id, "TextRange requested for unknown source file id");
        TextRange::new(file_id, start_byte, end_byte)
    }

    pub(super) fn assert_known_file_id(&self, file_id: SourceFileId, message: &str) {
        assert!(
            self.sources.files.contains_key(&file_id),
            "INVARIANT VIOLATED: {message}. \
             This is a bug because analysis facts and ranges must only reference registered files. \
             Fix: call AnalysisEngine::register_file before adding file facts."
        );
    }

    pub(super) fn intern_reference_candidates(
        &mut self,
        candidates: Vec<ReferenceCandidate>,
    ) -> Vec<StoredReferenceCandidate> {
        candidates
            .into_iter()
            .map(|candidate| match candidate.kind {
                ReferenceCandidateKind::Constant {
                    parts,
                    current_namespace,
                } => {
                    let context = self
                        .names
                        .intern_fqn(FullyQualifiedName::namespace(current_namespace));
                    let lookup = self
                        .names
                        .intern_const_lookup(ConstLookup::new(parts, false, context));
                    StoredReferenceCandidate::constant(candidate.range, lookup)
                }
                ReferenceCandidateKind::Method {
                    owner,
                    owner_kind,
                    method,
                    is_super,
                    access,
                    caller,
                    call_expression_range,
                    preferred_definition_range,
                    diagnostics,
                } => {
                    let root = self
                        .names
                        .intern_fqn(FullyQualifiedName::namespace(Vec::new()));
                    let owner = self
                        .names
                        .intern_const_lookup(ConstLookup::new(owner, true, root));
                    let caller = caller.map(|caller| self.names.intern_fqn(caller));
                    StoredReferenceCandidate::method(
                        candidate.range,
                        owner,
                        owner_kind,
                        method,
                        is_super,
                        access,
                        caller,
                        call_expression_range,
                        preferred_definition_range,
                        diagnostics,
                    )
                }
                ReferenceCandidateKind::Resolved { target, caller } => {
                    let target = self.names.intern_fqn(target);
                    let caller = caller.map(|caller| self.names.intern_fqn(caller));
                    StoredReferenceCandidate::resolved(candidate.range, target, caller)
                }
            })
            .collect()
    }

    pub(super) fn intern_symbol_facts(&mut self, facts: Vec<SymbolFact>) -> Vec<StoredSymbolFact> {
        facts
            .into_iter()
            .map(|fact| {
                let fqn = self.names.intern_fqn(fact.fqn);
                StoredSymbolFact::new(fqn, fact.kind, fact.range).with_name_range(fact.name_range)
            })
            .collect()
    }

    pub(super) fn intern_method_facts(&mut self, facts: Vec<MethodFact>) -> Vec<StoredMethodFact> {
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
                let fqn = self.names.intern_fqn(fact.fqn);
                let owner = self.names.intern_fqn(fact.owner);
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

    pub(super) fn intern_graph_node_facts(
        &mut self,
        facts: Vec<GraphNodeFact>,
    ) -> Vec<StoredGraphNodeFact> {
        facts
            .into_iter()
            .map(|fact| {
                let fqn = self.names.intern_fqn(fact.fqn);
                StoredGraphNodeFact::new(fqn, fact.kind, fact.range)
            })
            .collect()
    }

    pub(super) fn intern_graph_edge_facts(
        &mut self,
        facts: Vec<GraphEdgeFact>,
    ) -> Vec<StoredGraphEdgeFact> {
        facts
            .into_iter()
            .map(|fact| {
                let source = self.names.intern_fqn(fact.source);
                let target = self.names.intern_fqn(fact.target);
                StoredGraphEdgeFact::new(source, target, fact.kind, fact.range)
                    .with_provenance(fact.provenance)
            })
            .collect()
    }

    pub(super) fn intern_unresolved_graph_edge_facts(
        &mut self,
        facts: Vec<UnresolvedGraphEdgeFact>,
    ) -> Vec<StoredUnresolvedGraphEdgeFact> {
        facts
            .into_iter()
            .map(|fact| {
                let source = self.names.intern_fqn(fact.source);
                let context = self.names.intern_fqn(fact.context);
                let target = self.names.intern_const_lookup(ConstLookup::new(
                    ConstantPath::from_vec(fact.target_parts),
                    fact.absolute,
                    context,
                ));
                StoredUnresolvedGraphEdgeFact::new(source, target, fact.kind, fact.range)
                    .with_provenance(fact.provenance)
            })
            .collect()
    }

    pub(super) fn expand_symbol_fact(&self, fact: StoredSymbolFact) -> SymbolFact {
        let fqn = self
            .names
            .fqn(fact.fqn)
            .expect(
                "INVARIANT VIOLATED: symbol fact points to missing FQN id. \
                 This is a bug because symbol facts must only store interned FQN ids. \
                 Fix: intern symbol FQNs before inserting facts.",
            )
            .clone();
        SymbolFact::new(fqn, fact.kind, fact.range).with_name_range(fact.name_range)
    }

    pub(super) fn expand_method_fact(&self, fact: StoredMethodFact) -> MethodFact {
        let fqn = self
            .names
            .fqn(fact.fqn)
            .expect(
                "INVARIANT VIOLATED: method fact points to missing FQN id. \
                 This is a bug because method facts must only store interned FQN ids. \
                 Fix: intern method FQNs before inserting facts.",
            )
            .clone();
        let owner = self
            .names
            .fqn(fact.owner)
            .expect(
                "INVARIANT VIOLATED: method fact points to missing owner FQN id. \
                 This is a bug because method facts must only store interned owner FQN ids. \
                 Fix: intern method owners before inserting facts.",
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

    pub(super) fn expand_graph_node_fact(&self, fact: StoredGraphNodeFact) -> GraphNodeFact {
        let fqn = self
            .names
            .fqn(fact.fqn)
            .expect(
                "INVARIANT VIOLATED: graph node points to missing FQN id. \
                 This is a bug because graph nodes must only store interned FQN ids. \
                 Fix: intern graph node FQNs before inserting facts.",
            )
            .clone();
        GraphNodeFact::new(fqn, fact.kind, fact.range)
    }

    pub(super) fn expand_graph_edge_fact(&self, fact: StoredGraphEdgeFact) -> GraphEdgeFact {
        GraphEdgeFact::new(
            self.expand_interned_fqn(fact.source),
            self.expand_interned_fqn(fact.target),
            fact.kind,
            fact.range,
        )
        .with_provenance(fact.provenance)
    }

    pub(super) fn expand_unresolved_graph_edge_fact(
        &self,
        fact: StoredUnresolvedGraphEdgeFact,
    ) -> UnresolvedGraphEdgeFact {
        let source = self
            .names
            .fqn(fact.source)
            .expect(
                "INVARIANT VIOLATED: unresolved graph edge points to missing source FQN id. \
                 This is a bug because unresolved graph edges must only store interned source FQN ids. \
                 Fix: intern unresolved graph edge sources before inserting facts.",
            )
            .clone();
        let lookup = self.names.const_lookup(fact.target).expect(
            "INVARIANT VIOLATED: unresolved graph edge points to missing constant lookup id. \
             This is a bug because unresolved graph edges must only store interned constant lookup ids. \
             Fix: intern unresolved graph edge targets before inserting facts.",
        );
        let context = self
            .names
            .fqn(lookup.context)
            .expect(
                "INVARIANT VIOLATED: unresolved graph edge lookup points to missing context FQN id. \
                 This is a bug because constant lookups must only store interned context FQN ids. \
                 Fix: intern constant lookup contexts before inserting facts.",
            )
            .clone();
        UnresolvedGraphEdgeFact::new(
            source,
            lookup.path.to_vec(),
            lookup.absolute,
            context,
            fact.kind,
            fact.range,
        )
        .with_provenance(fact.provenance)
    }
}

pub(super) fn source_hash(source: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    hasher.finish()
}
