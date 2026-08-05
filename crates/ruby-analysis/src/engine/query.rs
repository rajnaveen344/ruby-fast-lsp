use std::path::Path;

use crate::core::method_store::MethodVisibilityOverrideFact;
use crate::core::{
    DiagnosticFact, ExecutionContextFact, FullyQualifiedName, GraphEdgeFact, GraphNodeFact,
    MethodFact, MethodReferenceAccess, ReferenceFact, SourceFileId, StoredReferenceCandidateKind,
    SymbolFact, TextRange, TypeFact, TypeResolution, TypeSubject,
};

use crate::{AnalysisEngine, SourceFile};

pub struct AnalysisQuery<'a> {
    pub(crate) engine: &'a AnalysisEngine,
}

impl<'a> AnalysisQuery<'a> {
    pub fn new(engine: &'a AnalysisEngine) -> Self {
        Self { engine }
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

    pub fn type_at(
        &self,
        subject: &TypeSubject,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> TypeResolution {
        self.engine.type_at(subject, file_id, byte_offset)
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

    pub fn resolved_reference_definition_ranges_at(
        &self,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Vec<TextRange> {
        let mut targets = self
            .engine
            .reference_candidate_store()
            .candidates_in_file(file_id)
            .into_iter()
            .filter_map(|candidate| {
                let contains_offset = candidate.range.file_id == file_id
                    && candidate.range.start_byte <= byte_offset
                    && byte_offset < candidate.range.end_byte;
                if !contains_offset {
                    return None;
                }
                match candidate.kind {
                    StoredReferenceCandidateKind::Resolved { target, .. } => {
                        self.engine
                            .fqn_for_id(target)
                            .cloned()
                            .map(|target| (target, None))
                    }
                    StoredReferenceCandidateKind::Method {
                        owner,
                        owner_kind,
                        method,
                        is_super,
                        access,
                        caller,
                        call_expression_range: _,
                        preferred_definition_range,
                        diagnostics,
                    } => {
                        let owner = self.engine.names.const_lookup(owner).expect(
                            "INVARIANT VIOLATED: exact method reference points to a missing owner lookup. \
                             This is a bug because candidates contain only interned lookup ids. \
                             Fix: intern method target owners before storing candidates.",
                        );
                        let owner = FullyQualifiedName::namespace_with_kind(
                            owner.path.to_vec(),
                            owner_kind,
                        );
                        if is_super {
                            return self
                                .resolve_super_method_reference(&owner, &method)
                                .reference_parts()
                                .filter(|(_resolved_owner, resolved_method, _)| {
                                    *resolved_method == method
                                })
                                .map(|(resolved_owner, resolved_method, _)| {
                                    (
                                        FullyQualifiedName::method(
                                            resolved_owner.namespace_parts(),
                                            resolved_method,
                                        ),
                                        preferred_definition_range,
                                    )
                                });
                        }

                        if diagnostics.is_none() {
                            return self
                                .resolve_method_reference(&owner, &method)
                                .reference_parts()
                                .filter(|(_resolved_owner, resolved_method, _)| {
                                    *resolved_method == method
                                })
                                .map(|(resolved_owner, resolved_method, _)| {
                                    (
                                        FullyQualifiedName::method(
                                            resolved_owner.namespace_parts(),
                                            resolved_method,
                                        ),
                                        preferred_definition_range,
                                    )
                                });
                        }

                        let callees = match access {
                            MethodReferenceAccess::Normal
                            | MethodReferenceAccess::VisibilityBypass => {
                                self.resolve_method_callees(&owner, &method)
                            }
                            MethodReferenceAccess::ExplicitReceiver => caller
                                .and_then(|caller| self.engine.fqn_for_id(caller))
                                .and_then(|caller| {
                                    let mut owners = self
                                        .engine
                                        .method_facts_for(caller)
                                        .into_iter()
                                        .map(|fact| fact.owner)
                                        .collect::<Vec<_>>();
                                    owners.sort_by_key(ToString::to_string);
                                    owners.dedup();
                                    let caller = if owners.len() == 1 {
                                        owners.pop().expect(
                                            "INVARIANT VIOLATED: one caller owner disappeared after length validation. This is a bug because protected navigation needs a stable caller namespace. Fix: keep caller-owner selection atomic.",
                                        )
                                    } else {
                                        FullyQualifiedName::namespace(caller.namespace_parts())
                                    };
                                    self.resolve_protected_method_callees(
                                        &owner,
                                        &method,
                                        &caller,
                                    )
                                })
                                .or_else(|| self.resolve_public_method_callees(&owner, &method)),
                        };
                        let mut exact_targets = callees
                            .unwrap_or_default()
                            .into_iter()
                            .filter(|callee| {
                                callee.method == method && !callee.definition_ranges.is_empty()
                            })
                            .map(|callee| {
                                FullyQualifiedName::method(
                                    callee.owner.namespace_parts(),
                                    callee.method,
                                )
                            })
                            .collect::<Vec<_>>();
                        exact_targets.sort_by_key(ToString::to_string);
                        exact_targets.dedup();
                        (exact_targets.len() == 1)
                            .then(|| exact_targets.pop())
                            .flatten()
                            .map(|target| (target, preferred_definition_range))
                    }
                    StoredReferenceCandidateKind::Constant { .. } => None,
                }
            })
            .collect::<Vec<_>>();
        targets.sort_by_key(|(target, preferred)| {
            (
                target.to_string(),
                preferred.map(|range| (range.file_id, range.start_byte, range.end_byte)),
            )
        });
        targets.dedup();
        if targets.len() != 1 {
            return Vec::new();
        }

        let (target, preferred_definition_range) = &targets[0];
        let mut ranges = match target {
            FullyQualifiedName::Method(_, _) => {
                let facts = self.engine.method_facts_for(target);
                if let Some(preferred) = preferred_definition_range {
                    if facts.iter().any(|fact| fact.range == *preferred) {
                        return vec![*preferred];
                    }
                }
                facts.into_iter().map(|fact| fact.range).collect::<Vec<_>>()
            }
            FullyQualifiedName::Namespace(_, _)
            | FullyQualifiedName::Constant(_)
            | FullyQualifiedName::LocalVariable(_)
            | FullyQualifiedName::InstanceVariable(_)
            | FullyQualifiedName::ClassVariable(_)
            | FullyQualifiedName::GlobalVariable(_) => self
                .engine
                .symbol_facts_for(target)
                .into_iter()
                .map(|fact| fact.range)
                .collect::<Vec<_>>(),
        };
        let winning_precedence = ranges
            .iter()
            .map(|range| {
                self.engine
                    .file(range.file_id)
                    .expect(
                        "INVARIANT VIOLATED: definition range references an unregistered source file. \
                         This is a bug because definition precedence requires stable source metadata. \
                         Fix: register sources before inserting symbol or method facts.",
                    )
                    .kind
                    .definition_precedence()
            })
            .min();
        if let Some(winning_precedence) = winning_precedence {
            ranges.retain(|range| {
                self.engine
                    .file(range.file_id)
                    .expect(
                        "INVARIANT VIOLATED: definition range disappeared during precedence filtering. \
                         This is a bug because definition queries hold an immutable engine borrow. \
                         Fix: keep file registration stable for the duration of an analysis query.",
                    )
                    .kind
                    .definition_precedence()
                    == winning_precedence
            });
        }
        ranges.sort_by_key(|range| (range.file_id, range.start_byte, range.end_byte));
        ranges.dedup();
        ranges
    }

    pub fn graph_nodes_for(&self, fqn: &FullyQualifiedName) -> Vec<GraphNodeFact> {
        self.engine.graph_nodes_for(fqn)
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
