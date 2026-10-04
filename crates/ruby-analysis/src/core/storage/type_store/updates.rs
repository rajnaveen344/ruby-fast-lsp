//! In-place Ruby type updates for solved equation and inferred method-return targets.

use crate::core::{FullyQualifiedName, RubyType};
use crate::invariant::ExpectInvariant;

use super::compact::TypeFactId;
use super::{SourceFileId, TextRange, TypeProvenance, TypeStore, TypeSubject};

impl TypeStore {
    /// Update inferred method-return facts without rebuilding the file-owned
    /// type store.
    ///
    /// SCC solving runs after a namespace has been traversed. Its results
    /// replace only the Ruby type payload of matching inferred facts; ranges,
    /// provenance, subject indexes, and facts from other files stay unchanged.
    pub fn update_inferred_method_return_types_in_file<'a>(
        &mut self,
        file_id: SourceFileId,
        updates: impl IntoIterator<Item = (&'a FullyQualifiedName, RubyType)>,
    ) -> usize {
        let mut updated = 0usize;
        for (method, ruby_type) in updates {
            let subject = TypeSubject::MethodReturn(method.clone());
            let Some(subject_id) = self.subject_id(&subject) else {
                continue;
            };
            let Some(fact_ids) = self.facts_by_subject.get(subject_id) else {
                continue;
            };
            let targets = fact_ids
                .iter()
                .copied()
                .filter(|fact_id| {
                    let fact = self.fact(*fact_id).unwrap_or_else(|| {
                        unreachable_invariant!(
                            what = "method-return type index points to a vacant fact slot",
                            why = "removed type facts must be removed from every index",
                            fix = "keep TypeStore subject indexes synchronized with the fact arena",
                        )
                    });
                    fact.range.file_id == file_id && fact.provenance == TypeProvenance::Inferred
                })
                .collect::<Vec<_>>();
            updated = updated
                .checked_add(self.set_ruby_type(&targets, ruby_type))
                .expect_invariant(
                    "inferred method-return update count overflowed usize",
                    "the count cannot exceed the bounded fact arena",
                    "keep TypeStore fact counts within addressable memory",
                );
        }
        updated
    }

    /// Update every fact with one exact file-owned semantic identity.
    pub(crate) fn update_equation_target(
        &mut self,
        subject: &TypeSubject,
        range: TextRange,
        ruby_type: RubyType,
    ) -> usize {
        let Some(subject_id) = self.subject_id(subject) else {
            return 0;
        };
        let Some(fact_ids) = self.facts_by_subject.get(subject_id) else {
            return 0;
        };
        let targets = fact_ids
            .iter()
            .copied()
            .filter(|fact_id| {
                let fact = self.fact(*fact_id).unwrap_or_else(|| {
                    unreachable_invariant!(
                        what = "a constant-equation target points to a vacant type fact",
                        why = "file replacement must update every type index atomically",
                        fix = "remove stale ids from facts_by_subject when a file is replaced",
                    )
                });
                fact.range == range
            })
            .collect::<Vec<_>>();
        self.set_ruby_type(&targets, ruby_type)
    }

    /// Update every scope projection of one exact local assignment. Scope ids
    /// are traversal-local and may change across document replacement; the
    /// source name and range are the stable file-owned identity.
    pub(crate) fn update_local_assignment_equation_target(
        &mut self,
        name: &str,
        range: TextRange,
        ruby_type: RubyType,
    ) -> usize {
        let Some(fact_ids) = self.facts_by_file.get(&range.file_id) else {
            return 0;
        };
        let targets = fact_ids
            .iter()
            .copied()
            .filter(|fact_id| {
                self.fact(*fact_id).is_some_and(|fact| {
                    if fact.range != range {
                        return false;
                    }
                    let Some(subject_id) = fact.subject.interned_id() else {
                        return false;
                    };
                    matches!(
                        self.subject(subject_id),
                        TypeSubject::Local {
                            name: fact_name,
                            ..
                        } if fact_name == name
                    )
                })
            })
            .collect::<Vec<_>>();
        self.set_ruby_type(&targets, ruby_type)
    }

    /// Write one type into the selected facts, interning it only when a fact
    /// receives it, and retire each replaced type reference.
    fn set_ruby_type(&mut self, targets: &[TypeFactId], ruby_type: RubyType) -> usize {
        if targets.is_empty() {
            return 0;
        }
        let ruby_type = self.intern_ruby_type(ruby_type);
        for fact_id in targets {
            let fact = self.facts[fact_id.index()].as_mut().expect_invariant(
                "a selected type update target became vacant",
                "resolution owns the type-store write lock",
                "keep target selection and update in one atomic pass",
            );
            if fact.ruby_type != ruby_type {
                fact.ruby_type = ruby_type;
                self.retired += 1;
            }
        }
        targets.len()
    }
}
