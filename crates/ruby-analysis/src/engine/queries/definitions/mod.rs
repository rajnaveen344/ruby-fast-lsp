//! Definition target selection, source preference, and lookup-aware ordering.

mod precedence;
#[cfg(test)]
mod tests;

use super::AnalysisQuery;
use crate::core::{
    FullyQualifiedName, MethodCalleeResolution, ResolvedMethodCallee, RubyMethod, RubyType,
    SourceFileId, StoredMethodReferenceCandidate, StoredReferenceCandidateKind, SymbolKind,
    TextRange,
};

pub(in crate::engine) type DefinitionLookupChains = Vec<Vec<FullyQualifiedName>>;

impl AnalysisQuery<'_> {
    /// Exact declaration-name targets for a proven type identity. Types may
    /// retain constant-form FQNs; declarations belong to instance namespaces.
    /// Name ranges let clients resolve navigation at the actual Ruby token.
    pub fn type_name_definition_ranges(&self, name: &FullyQualifiedName) -> Vec<TextRange> {
        let Some(namespace) = name.to_instance_namespace() else {
            return Vec::new();
        };
        self.preferred_definition_ranges(
            self.engine
                .symbol_facts_for(&namespace)
                .into_iter()
                .filter(|fact| matches!(fact.kind, SymbolKind::Class | SymbolKind::Module))
                .map(|fact| fact.name_range)
                .collect(),
        )
    }

    pub fn resolved_reference_definition_ranges_at(
        &self,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Vec<TextRange> {
        let mut candidates = Vec::new();
        for candidate in self
            .engine
            .reference_candidate_store()
            .candidates_in_file(file_id)
            .into_iter()
            .filter(|candidate| candidate.range.contains_offset(file_id, byte_offset))
        {
            // Extension patches enter both direct and merged collection paths.
            // Identical candidates represent one semantic target, not an
            // ambiguity. Distinct overlapping candidates remain separate and
            // continue through the fail-closed target checks below.
            if !candidates.contains(&candidate) {
                candidates.push(candidate);
            }
        }
        if let [candidate] = candidates.as_slice() {
            if let StoredReferenceCandidateKind::Method {
                owner,
                owner_kind,
                method,
                is_super,
                access,
                caller,
                call_expression_range,
                preferred_definition_range,
                diagnostics,
            } = &candidate.kind
            {
                let candidate = StoredMethodReferenceCandidate {
                    range: candidate.range,
                    owner: *owner,
                    owner_kind: *owner_kind,
                    method: *method,
                    is_super: *is_super,
                    access: *access,
                    caller: *caller,
                    call_expression_range: *call_expression_range,
                    preferred_definition_range: *preferred_definition_range,
                    diagnostics: diagnostics.clone(),
                };
                let mut lookup_chains = Vec::new();
                let callees = self
                    .method_candidate_callees_with_navigation(&candidate, Some(&mut lookup_chains));
                return self.rank_method_definitions(
                    callees
                        .into_iter()
                        .filter(|callee| callee.method == candidate.method)
                        .collect(),
                    &lookup_chains,
                    candidate.preferred_definition_range,
                );
            }
        }
        let mut targets = candidates
            .into_iter()
            .flat_map(|candidate| match candidate.kind {
                StoredReferenceCandidateKind::Resolved { target, .. } => vec![self
                    .engine
                    .fqn_for_id(target)
                    .expect(
                        "INVARIANT VIOLATED: exact resolved reference points to a missing target FQN. This is a bug because resolved candidates contain only interned target ids. Fix: intern the target before storing the reference candidate and keep the name arena append-only.",
                    )
                    .clone()],
                StoredReferenceCandidateKind::Method { .. } => Vec::new(),
                StoredReferenceCandidateKind::Constant { lookup } => {
                    let lookup = self.engine.names.const_lookup(lookup).expect(
                        "INVARIANT VIOLATED: exact constant reference points to a missing lookup. This is a bug because candidates contain only interned lookup ids. Fix: intern constant lookups before storing reference candidates.",
                    );
                    let context = self.engine.names.fqn(lookup.context).expect(
                        "INVARIANT VIOLATED: exact constant reference lookup points to a missing context FQN. This is a bug because constant lookups must retain their interned lexical context. Fix: intern the context before storing the lookup.",
                    );
                    self.resolve_constant_in_context(
                        lookup.path.as_slice(),
                        &if lookup.absolute {
                            Vec::new()
                        } else {
                            context.namespace_parts()
                        },
                    )
                    .map(|target| vec![target])
                    .unwrap_or_default()
                }
            })
            .collect::<Vec<_>>();
        targets.sort_by_key(ToString::to_string);
        targets.dedup();
        if targets.len() > 1 {
            return Vec::new();
        }
        let mut all_ranges = Vec::new();
        for target in targets {
            let mut ranges = match &target {
                FullyQualifiedName::Method(_, _) => {
                    let facts = self.engine.method_facts_for(&target);
                    facts.into_iter().map(|fact| fact.range).collect::<Vec<_>>()
                }
                FullyQualifiedName::Namespace(_, _)
                | FullyQualifiedName::Constant(_)
                | FullyQualifiedName::LocalVariable(_)
                | FullyQualifiedName::InstanceVariable(_)
                | FullyQualifiedName::ClassVariable(_)
                | FullyQualifiedName::GlobalVariable(_) => self
                    .engine
                    .symbol_facts_for(&target)
                    .into_iter()
                    .map(|fact| fact.range)
                    .collect::<Vec<_>>(),
            };
            ranges = self.preferred_definition_ranges(ranges);
            all_ranges.extend(ranges);
        }
        self.sort_definition_ranges(&mut all_ranges);
        all_ranges.dedup();
        all_ranges
    }

    /// Definition projection of ordinary method resolution, retaining only the
    /// participating receivers' lookup chains for semantic ordering.
    pub fn method_definition_ranges(
        &self,
        owner: &FullyQualifiedName,
        method: &RubyMethod,
        allow_private: bool,
        protected_caller: Option<&FullyQualifiedName>,
    ) -> Option<Vec<TextRange>> {
        let mut chains = Vec::new();
        let callees = self.resolve_method_callees_inner(
            owner,
            method,
            allow_private,
            protected_caller,
            Some(&mut chains),
        )?;
        Some(self.rank_method_definitions(callees, &chains, None))
    }

    pub fn method_definition_ranges_for_type(
        &self,
        receiver_type: &RubyType,
        method: &RubyMethod,
        allow_private: bool,
        protected_caller: Option<&FullyQualifiedName>,
    ) -> Option<Vec<TextRange>> {
        let mut chains = Vec::new();
        let callees = self.resolve_method_callees_for_type_inner(
            receiver_type,
            method,
            allow_private,
            protected_caller,
            Some(&mut chains),
        )?;
        Some(self.rank_method_definitions(callees, &chains, None))
    }

    pub fn super_definition_ranges(
        &self,
        owner: &FullyQualifiedName,
        method: &RubyMethod,
    ) -> Option<Vec<TextRange>> {
        let callee = self.resolve_super_method_callee(owner, method)?;
        Some(self.rank_method_definitions(vec![callee], &[], None))
    }

    fn rank_method_definitions(
        &self,
        callees: Vec<ResolvedMethodCallee>,
        lookup_chains: &[Vec<FullyQualifiedName>],
        preferred: Option<TextRange>,
    ) -> Vec<TextRange> {
        let mut grouped = std::collections::BTreeMap::<FullyQualifiedName, Vec<TextRange>>::new();
        for callee in callees {
            if callee.resolution == MethodCalleeResolution::Exact {
                grouped
                    .entry(callee.owner)
                    .or_default()
                    .extend(callee.definition_ranges);
            }
        }
        let groups = grouped
            .into_iter()
            .filter_map(|(owner, mut ranges)| {
                if let Some(preferred) = preferred.filter(|preferred| ranges.contains(preferred)) {
                    ranges = vec![preferred];
                }
                let ranges = self.preferred_definition_ranges(ranges);
                (!ranges.is_empty()).then_some((owner, ranges))
            })
            .collect::<Vec<_>>();
        let owners = groups
            .iter()
            .map(|(owner, _)| owner.clone())
            .collect::<Vec<_>>();
        let mut result = Vec::new();
        for layer in precedence::layers(&owners, lookup_chains) {
            let mut ranges = layer
                .into_iter()
                .flat_map(|index| groups[index].1.iter().copied())
                .collect::<Vec<_>>();
            self.sort_definition_ranges(&mut ranges);
            result.extend(ranges);
        }
        // Several receiver identities can lead to one physical declaration.
        let mut seen = std::collections::HashSet::new();
        result.retain(|range| seen.insert(*range));
        result
    }

    /// Apply source precedence only within one selected declaration identity.
    /// Never drop a separate receiver's winner because it has only a signature.
    pub(in crate::engine) fn preferred_definition_ranges(
        &self,
        mut ranges: Vec<TextRange>,
    ) -> Vec<TextRange> {
        if let Some(priority) = ranges
            .iter()
            .map(|range| self.definition_source_priority(*range))
            .min()
        {
            ranges.retain(|range| self.definition_source_priority(*range) == priority);
        }
        self.sort_definition_ranges(&mut ranges);
        ranges.dedup();
        ranges
    }

    fn definition_source_priority(&self, range: TextRange) -> u8 {
        self.engine.file(range.file_id).expect(
            "INVARIANT VIOLATED: a definition destination has no registered source. This is a bug because navigation must retain source ownership. Fix: register sources before publishing definition facts.",
        ).kind.definition_precedence()
    }

    /// Order already selected destinations independently of source registration.
    /// This is a presentation tie-breaker, never Ruby load order or dispatch priority.
    pub(in crate::engine) fn sort_definition_ranges(&self, ranges: &mut [TextRange]) {
        ranges.sort_by_key(|range| {
            let file = self.engine.file(range.file_id).expect(
                "INVARIANT VIOLATED: a definition destination has no registered source. This is a bug because navigation must retain source ownership. Fix: register sources before publishing definition facts.",
            );
            (file.kind.definition_precedence(), file.path.as_path(), range.start_byte, range.end_byte)
        });
    }
}
