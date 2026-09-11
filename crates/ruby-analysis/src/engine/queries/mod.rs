pub(in crate::engine) mod cache;
pub(in crate::engine) mod definitions;
pub(in crate::engine) mod hierarchy;
pub(in crate::engine) mod lookup;
pub(in crate::engine) mod namespace_tree;
pub(in crate::engine) mod type_query;
pub(in crate::engine) mod workspace_symbols;

use std::path::Path;

use crate::core::MethodVisibilityOverrideFact;
use crate::core::{
    DiagnosticFact, ExecutionContextFact, FullyQualifiedName, GraphEdgeFact, GraphNodeFact,
    GraphNodeKind, MethodCalleeResolution, MethodFact, ReferenceFact, RubyType, SourceFileId,
    StoredReferenceCandidateKind, SymbolFact, TextRange, TypeFact, TypeResolution, TypeSubject,
    UnknownReason,
};

use crate::engine::{AnalysisEngine, SourceFile};

pub struct AnalysisQuery<'a> {
    pub(crate) engine: &'a AnalysisEngine,
}

impl<'a> AnalysisQuery<'a> {
    pub fn new(engine: &'a AnalysisEngine) -> Self {
        Self { engine }
    }

    pub(crate) fn query_cache_identity(&self) -> (u64, u64) {
        self.engine.query_cache_identity()
    }

    pub fn file_id(&self, path: impl AsRef<Path>) -> Option<SourceFileId> {
        self.engine.file_id(path)
    }

    pub fn file(&self, file_id: SourceFileId) -> Option<&'a SourceFile> {
        self.engine.file(file_id)
    }

    pub fn execution_context_at(
        &self,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<&'a ExecutionContextFact> {
        self.engine.execution_context_at(file_id, byte_offset)
    }

    pub(crate) fn constant_callable_body(
        &self,
        constant: &FullyQualifiedName,
    ) -> Option<Result<crate::core::CallableBodySummary, UnknownReason>> {
        self.engine.constant_callable_body(constant)
    }

    pub fn type_at(
        &self,
        subject: &TypeSubject,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> TypeResolution {
        self.engine.type_at(subject, file_id, byte_offset)
    }

    pub fn type_facts_for(&self, subject: &TypeSubject) -> Vec<TypeFact> {
        self.engine.type_facts_for(subject)
    }

    /// All stored type facts, detached from the engine's internal indexes.
    pub fn all_type_facts(&self) -> Vec<TypeFact> {
        self.engine.type_store().all_facts()
    }

    pub fn type_facts_in_file(&self, file_id: SourceFileId) -> Vec<TypeFact> {
        self.engine.type_store().facts_in_file(file_id)
    }

    pub fn symbol_facts_in_file(&self, file_id: SourceFileId) -> Vec<SymbolFact> {
        self.engine.symbol_facts_in_file(file_id)
    }

    pub fn all_symbol_facts(&self) -> Vec<SymbolFact> {
        self.engine.all_symbol_facts()
    }

    pub fn has_symbols(&self) -> bool {
        !self.engine.all_symbol_facts().is_empty()
    }

    pub fn symbols_for_fqn(&self, fqn: &FullyQualifiedName) -> Vec<SymbolFact> {
        self.engine.symbol_facts_for(fqn)
    }

    pub fn references_for_fqn(&self, fqn: &FullyQualifiedName) -> &'a [ReferenceFact] {
        self.engine.reference_facts_for(fqn)
    }

    pub fn methods_for_fqn(&self, fqn: &FullyQualifiedName) -> Vec<MethodFact> {
        self.engine.method_facts_for(fqn)
    }

    pub fn method_facts_in_file(&self, file_id: SourceFileId) -> Vec<MethodFact> {
        self.engine.method_facts_in_file(file_id)
    }

    pub fn method_visibility_overrides_in_file(
        &self,
        file_id: SourceFileId,
    ) -> Vec<MethodVisibilityOverrideFact> {
        self.engine.method_visibility_overrides_in_file(file_id)
    }

    pub fn all_method_facts(&self) -> Vec<MethodFact> {
        self.engine.all_method_facts()
    }

    pub fn references_in_file(&self, file_id: SourceFileId) -> Vec<ReferenceFact> {
        self.engine.reference_store().facts_in_file(file_id)
    }

    /// A module call's references follow its proven concrete receiver identity.
    /// `Some([])` must not trigger a lexical fallback in the protocol adapter.
    pub fn module_call_reference_ranges_at(
        &self,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<Vec<TextRange>> {
        match self.module_call_reference_lookup_at(file_id, byte_offset)? {
            crate::engine::MethodLookupResult::Unique(fact) => {
                let method = crate::engine::resolution::method_name_from_fact(&fact);
                Some(self.method_reference_ranges_for_exact_target(&fact.owner, &method, &fact.fqn))
            }
            crate::engine::MethodLookupResult::Missing
            | crate::engine::MethodLookupResult::Ambiguous { .. } => Some(Vec::new()),
        }
    }

    /// Same identity as references, with candidate traversal confined to this file.
    pub fn module_call_highlight_ranges_at(
        &self,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<Vec<TextRange>> {
        let crate::engine::MethodLookupResult::Unique(fact) =
            self.module_call_reference_lookup_at(file_id, byte_offset)?
        else {
            return Some(Vec::new());
        };
        let method = crate::engine::resolution::method_name_from_fact(&fact);
        let mut ranges = self
            .reference_ranges_for_fqn(&fact.fqn)
            .into_iter()
            .filter(|range| range.file_id == file_id)
            .collect::<Vec<_>>();
        for candidate in self
            .engine
            .reference_candidate_store()
            .method_candidates_in_file(file_id)
            .filter(|candidate| candidate.method == method)
        {
            if self
                .method_candidate_callees(candidate)
                .iter()
                .any(|callee| {
                    callee.resolution == MethodCalleeResolution::Exact
                        && callee.owner == fact.owner
                        && callee.method == method
                        && !callee.definition_ranges.is_empty()
                })
            {
                ranges.push(candidate.range);
            }
        }
        ranges.sort_by_key(|range| (range.start_byte, range.end_byte));
        ranges.dedup();
        Some(ranges)
    }

    // Declarations and reflective operands retain their own namespace lookup.
    fn module_call_reference_lookup_at(
        &self,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<crate::engine::MethodLookupResult> {
        let mut lookups = Vec::new();
        for candidate in self
            .engine
            .reference_candidate_store()
            .candidates_in_file(file_id)
        {
            if !candidate.range.contains_offset(file_id, byte_offset) {
                continue;
            }
            let StoredReferenceCandidateKind::Method {
                owner,
                owner_kind,
                method,
                is_super: false,
                access: crate::core::MethodReferenceAccess::Normal,
                ..
            } = candidate.kind
            else {
                continue;
            };
            let owner = self.engine.names.const_lookup(owner).expect(
                "INVARIANT VIOLATED: module call has no interned owner. This is a bug because candidates retain owner identity. Fix: intern the owner before storing its candidate.");
            let owner = FullyQualifiedName::namespace_with_kind(owner.path.to_vec(), owner_kind);
            if crate::engine::resolution::module_instance_receivers(self.engine, &owner).is_empty()
            {
                continue;
            }
            let lookup = (owner, method);
            if !lookups.contains(&lookup) {
                lookups.push(lookup);
            }
        }
        if lookups.is_empty() {
            return None;
        }
        let [(owner, method)] = lookups.as_slice() else {
            return Some(crate::engine::MethodLookupResult::Missing);
        };
        Some(self.resolve_method_reference(owner, method))
    }

    pub fn navigation_must_fail_closed_at(
        &self,
        file_id: SourceFileId,
        byte_offset: u32,
        exact_target_proven: bool,
    ) -> bool {
        let candidates = self
            .engine
            .reference_candidate_store()
            .candidates_in_file(file_id);
        let exact_non_method_reference = exact_target_proven
            && candidates.iter().any(|candidate| {
                candidate.range.contains_offset(file_id, byte_offset)
                    && matches!(
                        candidate.kind,
                        StoredReferenceCandidateKind::Resolved { .. }
                            | StoredReferenceCandidateKind::Constant { .. }
                    )
            });
        if exact_non_method_reference {
            return false;
        }
        let unknown_reason_blocks_dispatch = |reason| {
            matches!(
                reason,
                UnknownReason::NoReachingAssignment
                    | UnknownReason::UnresolvedAssignmentValue
                    | UnknownReason::AmbiguousReachingAssignment
                    | UnknownReason::UnknownReceiver
                    | UnknownReason::InvalidMethodName
                    | UnknownReason::IncompleteUnionMember
            )
        };
        let candidate_barrier = self
            .engine
            .reference_candidate_store()
            .candidates_in_file(file_id)
            .iter()
            .filter(|candidate| candidate.range.contains_offset(file_id, byte_offset))
            .any(|candidate| {
                let StoredReferenceCandidateKind::Method {
                    method,
                    access,
                    call_expression_range: _,
                    diagnostics,
                    ..
                } = &candidate.kind
                else {
                    return false;
                };
                if *access == crate::core::MethodReferenceAccess::InstanceMethodReflection
                    && !exact_target_proven
                {
                    // A failed namespace inspection must not fall back to
                    // ordinary calls on possible including objects.
                    return true;
                }
                let receiver_unknown = diagnostics.as_deref().is_some_and(|diagnostics| {
                    diagnostics.receiver_expression_range.is_some_and(|range| {
                        self.local_read_type_at(range.file_id, range.start_byte)
                            == Some(RubyType::Unknown)
                            || self.exact_expression_unknown_reason(range).is_some()
                    })
                });
                let resolved_to_fallback = self
                    .engine
                    .reference_store()
                    .targets_for_exact_range(candidate.range)
                    .into_iter()
                    .filter_map(|target| self.engine.fqn_for_id(target))
                    .any(|target| {
                        matches!(
                            target,
                            FullyQualifiedName::Method(_, resolved_method)
                                if resolved_method != method
                        )
                    });
                receiver_unknown || resolved_to_fallback
            });
        candidate_barrier
            || (!exact_target_proven
                && self
                    .expression_unknown_reason_at(file_id, byte_offset)
                    .is_some_and(unknown_reason_blocks_dispatch))
    }

    pub fn graph_nodes_for(&self, fqn: &FullyQualifiedName) -> Vec<GraphNodeFact> {
        self.engine.graph_nodes_for(fqn)
    }

    pub fn has_graph_node(&self, fqn: &FullyQualifiedName) -> bool {
        self.engine.has_graph_node(fqn)
    }

    pub fn first_graph_node_kind(&self, fqn: &FullyQualifiedName) -> Option<GraphNodeKind> {
        self.engine.first_graph_node_kind(fqn)
    }

    pub fn latest_graph_node_kind(&self, fqn: &FullyQualifiedName) -> Option<GraphNodeKind> {
        self.engine.latest_graph_node_kind(fqn)
    }

    pub fn graph_edges_from(&self, fqn: &FullyQualifiedName) -> Vec<GraphEdgeFact> {
        self.engine.graph_edges_from(fqn)
    }

    pub fn all_graph_edges(&self) -> Vec<GraphEdgeFact> {
        self.engine.all_graph_edges()
    }

    pub fn diagnostic_facts_in_file(&self, file_id: SourceFileId) -> Vec<DiagnosticFact> {
        self.engine.diagnostic_facts_in_file(file_id)
    }

    pub fn all_diagnostic_facts(&self) -> Vec<DiagnosticFact> {
        self.engine.all_diagnostic_facts()
    }

    pub fn graph_nodes_in_file(&self, file_id: SourceFileId) -> Vec<GraphNodeFact> {
        self.engine.graph_nodes_in_file(file_id)
    }

    pub fn graph_edges_in_file(&self, file_id: SourceFileId) -> Vec<GraphEdgeFact> {
        self.engine.graph_edges_in_file(file_id)
    }
}
