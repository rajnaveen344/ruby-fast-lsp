//! Rename targets and their safety checks for methods and constants.

use crate::invariant::ExpectInvariant;

use super::lookup_chain::method_lookup_chain;
use super::ConstantRenameTarget;
use crate::core::storage::reference_store::StoredMethodReferenceCandidate;
use crate::core::storage::reference_store::StoredReferenceCandidateRef;
use crate::core::{
    FullyQualifiedName, RubyConstant, RubyMethod, SourceFileId, SymbolKind, TextRange,
};
use crate::engine::diagnostics::policy::AncestryCompleteness;
use crate::engine::queries::View;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct MethodRenameIdentity {
    owner: FullyQualifiedName,
    method: RubyMethod,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodRenameTarget {
    pub owner: FullyQualifiedName,
    pub current_name: RubyMethod,
    pub ranges: Vec<TextRange>,
}

impl<'a> View<'a> {
    /// Resolve a method declaration or call at a byte offset into a safe,
    /// project-editable rename target.
    ///
    /// Method identity includes the namespace kind, so `User#name` and
    /// `User.name` never share an edit set. Declarations without an exact name
    /// token (aliases, delegates, generated macros, signatures, and external
    /// sources) are deliberately rejected.
    pub fn method_rename_target_at(
        &self,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<MethodRenameTarget> {
        let identity = self.method_rename_identity_at(file_id, byte_offset)?;
        self.method_rename_target(identity, None)
    }

    pub fn method_rename_target_for_name_at(
        &self,
        file_id: SourceFileId,
        byte_offset: u32,
        new_name: RubyMethod,
    ) -> Option<MethodRenameTarget> {
        let identity = self.method_rename_identity_at(file_id, byte_offset)?;
        self.method_rename_target(identity, Some(new_name))
    }

    fn method_rename_identity_at(
        &self,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> Option<MethodRenameIdentity> {
        let mut identities = self
            .method_facts_in_file(file_id)
            .into_iter()
            .filter(|fact| fact.name_range.contains_offset(file_id, byte_offset))
            .filter_map(|fact| {
                let FullyQualifiedName::Method(_, method) = fact.fqn else {
                    return None;
                };
                Some(MethodRenameIdentity {
                    owner: fact.owner,
                    method,
                })
            })
            .collect::<Vec<_>>();

        for candidate in self.engine.uses.candidates().candidates_in_file(file_id) {
            if !candidate.range.contains_offset(file_id, byte_offset) {
                continue;
            }
            let crate::core::storage::reference_store::StoredReferenceCandidateKind::Method(
                candidate,
            ) = &candidate.kind
            else {
                continue;
            };
            identities.extend(self.method_candidate_rename_identities(candidate));
        }

        identities.sort();
        identities.dedup();
        (identities.len() == 1).then(|| {
            identities.pop().expect_invariant(
                "method rename identity disappeared after length validation",
                "the local identity vector is not mutated between the check and pop",
                "keep identity selection atomic",
            )
        })
    }

    fn method_rename_target(
        &self,
        identity: MethodRenameIdentity,
        new_name: Option<RubyMethod>,
    ) -> Option<MethodRenameTarget> {
        if identity.owner.has_generated_owner() {
            return None;
        }
        if !method_name_is_refactorable(identity.method) {
            return None;
        }
        if new_name.is_some_and(|new_name| new_name == identity.method) {
            return None;
        }
        if new_name.is_some_and(|new_name| {
            !method_name_is_refactorable(new_name)
                || identity.method.as_str().ends_with('=') != new_name.as_str().ends_with('=')
        }) {
            return None;
        }
        // The same completeness walk that suppresses `unresolved-method`
        // claims: an unknown lookup edge leaves the collision proof open.
        if !AncestryCompleteness::new(self.engine)
            .chain(self.engine, &identity.owner, |_| false)
            .is_missing()
        {
            return None;
        }
        if let Some(new_name) = new_name {
            let target_chain = method_lookup_chain(self.engine, &identity.owner);
            let collision = target_chain.iter().any(|owner| {
                self.method_facts_matching_owner_name(&owner, &new_name)
                    .into_iter()
                    .any(|fact| {
                        self.file(fact.range.file_id)
                            .is_some_and(|file| file.kind != crate::core::SourceKind::Signature)
                    })
            }) || self.method_facts_named(new_name).any(|fact| {
                self.file(fact.range.file_id)
                    .is_some_and(|file| file.kind != crate::core::SourceKind::Signature)
                    && (target_chain.contains(&fact.owner)
                        || method_lookup_chain(self.engine, &fact.owner).contains(&identity.owner))
            });
            if collision {
                return None;
            }
        }

        let method_fqn =
            FullyQualifiedName::method(identity.owner.namespace_parts(), identity.method);
        let all_method_facts = self.method_facts_for(&method_fqn);
        let declaration_facts = all_method_facts
            .iter()
            .filter(|fact| fact.owner == identity.owner)
            .filter(|fact| {
                self.file(fact.range.file_id)
                    .is_some_and(|file| file.kind != crate::core::SourceKind::Signature)
            })
            .collect::<Vec<_>>();
        if declaration_facts.is_empty()
            || declaration_facts.iter().any(|fact| {
                !self
                    .file(fact.range.file_id)
                    .is_some_and(|file| file.kind.is_editable())
                    || fact
                        .name_range
                        .end_byte
                        .checked_sub(fact.name_range.start_byte)
                        != u32::try_from(identity.method.as_str().len()).ok()
                    || fact.range == fact.name_range
            })
        {
            return None;
        }

        // One Ruby declaration can materialize multiple semantic owners (for
        // example `module_function`). Renaming only one of those identities
        // would lie about the resulting program, so reject the coupled token.
        if declaration_facts.iter().any(|declaration| {
            self.method_facts_where(|other, names| {
                (other.name_range() == declaration.name_range || other.range == declaration.range)
                    && names.fqn(other.owner) != Some(&identity.owner)
            })
            .next()
            .is_some()
        }) {
            return None;
        }

        let mut ranges = declaration_facts
            .into_iter()
            .map(|fact| fact.name_range)
            .collect::<Vec<_>>();
        ranges.extend(
            self.method_visibility_overrides_matching_owner_name(&identity.owner, &identity.method)
                .into_iter()
                .filter(|fact| {
                    self.file(fact.range.file_id)
                        .is_some_and(|file| file.kind.is_editable())
                })
                .map(|fact| fact.range),
        );

        for candidate in self.engine.uses.candidates().iter_candidates() {
            match candidate {
                StoredReferenceCandidateRef::Method(candidate)
                    if candidate.method() == identity.method =>
                {
                    let targets = self.method_candidate_rename_identities(candidate);
                    let caller_is_target = candidate.caller().is_some_and(|caller| {
                        self.engine.names.fqn(caller).is_some_and(|caller| {
                            matches!(
                                caller,
                                FullyQualifiedName::Method(parts, method)
                                    if *method == identity.method
                                        && parts.as_slice()
                                            == identity.owner.namespace_parts_slice()
                            )
                        })
                    });
                    if candidate.is_super() && (targets.contains(&identity) || caller_is_target) {
                        return None;
                    }
                    if targets.contains(&identity) {
                        if targets.len() != 1 {
                            return None;
                        }
                        if let Some(new_name) = new_name {
                            let collision_candidate = candidate.renamed(new_name);
                            if !self
                                .method_candidate_rename_identities(&collision_candidate)
                                .is_empty()
                            {
                                return None;
                            }
                        }
                        if self
                            .file(candidate.range.file_id)
                            .is_some_and(|file| file.kind.is_editable())
                        {
                            ranges.push(candidate.range);
                        }
                    }
                }
                StoredReferenceCandidateRef::Resolved(candidate) => {
                    let Some(target) = self.engine.names.fqn(candidate.target) else {
                        unreachable_invariant!(
                            what = "resolved rename candidate points to a missing FQN",
                            why = "resolved candidates retain interned targets",
                            fix = "retain interned names for the candidate lifetime",
                        );
                    };
                    if target != &method_fqn {
                        continue;
                    }
                    let mut owners = all_method_facts
                        .iter()
                        .map(|fact| fact.owner.clone())
                        .collect::<Vec<_>>();
                    owners.sort_by_key(ToString::to_string);
                    owners.dedup();
                    if owners != vec![identity.owner.clone()] {
                        return None;
                    }
                    if self
                        .file(candidate.range.file_id)
                        .is_some_and(|file| file.kind.is_editable())
                    {
                        ranges.push(candidate.range);
                    }
                }
                StoredReferenceCandidateRef::Constant(_)
                | StoredReferenceCandidateRef::Method(_) => {}
            }
        }

        ranges.sort_by_key(|range| (range.file_id, range.start_byte, range.end_byte));
        ranges.dedup();
        Some(MethodRenameTarget {
            owner: identity.owner,
            current_name: identity.method,
            ranges,
        })
    }
}

impl<'a> View<'a> {
    fn method_candidate_rename_identities(
        &self,
        candidate: &StoredMethodReferenceCandidate,
    ) -> Vec<MethodRenameIdentity> {
        let callees = self.method_candidate_callees(candidate);
        let mut identities = callees
            .into_iter()
            .filter(|callee| {
                callee.resolution == crate::core::MethodCalleeResolution::Exact
                    && callee.method == candidate.method()
                    && !callee.definition_ranges.is_empty()
            })
            .map(|callee| MethodRenameIdentity {
                owner: callee.owner,
                method: callee.method,
            })
            .collect::<Vec<_>>();
        identities.sort();
        identities.dedup();
        identities
    }
}

impl<'a> View<'a> {
    /// Resolve a constant-like symbol and return every editable project range.
    ///
    /// Definition token boundaries come from indexer facts; references come
    /// from the engine's centralized constant resolution. External sources are
    /// intentionally excluded because an editor rename must never edit gems,
    /// stdlib, or generated stubs.
    pub fn constant_rename_target(
        &self,
        parts: &[RubyConstant],
        context: &[RubyConstant],
    ) -> Option<ConstantRenameTarget> {
        let fqn = self.resolve_constant_in_context(parts, context)?;
        if fqn.has_generated_owner() {
            return None;
        }
        let current_name = *fqn.namespace_parts_slice().last()?;
        let symbol_facts = self
            .symbol_facts_for(&fqn)
            .into_iter()
            .filter(|fact| {
                matches!(
                    fact.kind,
                    SymbolKind::Class | SymbolKind::Module | SymbolKind::Constant
                ) && fact
                    .name_range
                    .end_byte
                    .checked_sub(fact.name_range.start_byte)
                    == u32::try_from(current_name.as_str().len()).ok()
                    && self
                        .file(fact.range.file_id)
                        .is_some_and(|file| file.kind.is_editable())
            })
            .collect::<Vec<_>>();
        if symbol_facts.is_empty() {
            return None;
        }
        let mut ranges = symbol_facts
            .into_iter()
            .map(|fact| fact.name_range)
            .chain(
                self.reference_facts_for(&fqn)
                    .iter()
                    .filter(|fact| {
                        self.file(fact.range.file_id)
                            .is_some_and(|file| file.kind.is_editable())
                    })
                    .filter_map(|fact| {
                        constant_reference_name_range(self.engine, fact.range, current_name)
                    }),
            )
            .collect::<Vec<_>>();
        ranges.sort_by_key(|range| (range.file_id, range.start_byte, range.end_byte));
        ranges.dedup();

        Some(ConstantRenameTarget {
            fqn,
            current_name,
            ranges,
        })
    }

    pub fn constant_rename_target_for_name(
        &self,
        parts: &[RubyConstant],
        context: &[RubyConstant],
        new_name: RubyConstant,
    ) -> Option<ConstantRenameTarget> {
        let target = self.constant_rename_target(parts, context)?;
        if target.current_name == new_name
            || constant_name_collides(self.engine, &target.fqn, new_name)
        {
            return None;
        }
        Some(target)
    }
}

fn method_name_is_refactorable(method: RubyMethod) -> bool {
    !matches!(
        method.as_str(),
        "+" | "-"
            | "*"
            | "/"
            | "%"
            | "**"
            | "+@"
            | "-@"
            | "<<"
            | ">>"
            | "&"
            | "|"
            | "^"
            | "~"
            | "<=>"
            | "<"
            | "<="
            | ">"
            | ">="
            | "=="
            | "==="
            | "!="
            | "=~"
            | "!~"
            | "[]"
            | "[]="
            | "`"
    )
}

fn constant_reference_name_range(
    engine: &crate::engine::Project,
    range: TextRange,
    name: RubyConstant,
) -> Option<TextRange> {
    let file = engine.view().file(range.file_id)?;
    let start = usize::try_from(range.start_byte).ok()?;
    let end = usize::try_from(range.end_byte).ok()?;
    if let Some(source) = file.source_text() {
        let text = source.get(start..end)?;
        if !text.as_bytes().ends_with(name.as_str().as_bytes()) {
            return None;
        }
    } else {
        invariant!(
            file.line_index.is_ascii(),
            what = "source text was discarded for a non-ASCII file",
            why = "exact rename validation requires retained non-ASCII source",
            fix = "retain SourceFile::source whenever SourceLineIndex::is_ascii is false",
        );
    }
    let name_start = end.checked_sub(name.as_str().len())?;
    if name_start < start {
        return None;
    }
    Some(TextRange::new(
        range.file_id,
        u32::try_from(name_start).ok()?,
        range.end_byte,
    ))
}

fn constant_name_collides(
    engine: &crate::engine::Project,
    target: &FullyQualifiedName,
    new_name: RubyConstant,
) -> bool {
    let mut parts = target.namespace_parts();
    let last = parts.last_mut().expect_invariant(
        "rename target has no constant path component",
        "constant_rename_target only returns constant-like FQNs",
        "reject empty constant paths before constructing a rename target",
    );
    *last = new_name;

    let namespace = FullyQualifiedName::namespace(parts.clone());
    let constant = FullyQualifiedName::constant(parts);
    engine.view().has_symbol_facts(&namespace)
        || engine.view().has_graph_node(&namespace)
        || engine.view().has_symbol_facts(&constant)
}
