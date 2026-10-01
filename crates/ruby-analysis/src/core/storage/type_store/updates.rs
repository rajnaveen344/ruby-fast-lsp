//! In-place Ruby type updates for solved equation and inferred method-return targets.

use crate::core::{FullyQualifiedName, RubyType};

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
            let ruby_type = self.intern_ruby_type(ruby_type);
            let Some(fact_ids) = self.facts_by_subject.get(&subject_id).cloned() else {
                continue;
            };
            for fact_id in fact_ids {
                let fact = self.facts.get_mut(fact_id.index()).unwrap_or_else(|| {
                    panic!(
                        "INVARIANT VIOLATED: method-return type index points outside the fact arena. \
                         This is a bug because indexed type ids must reference allocated slots. \
                         Fix: update every TypeStore index when facts are removed or reused."
                    )
                });
                let fact = fact.as_mut().unwrap_or_else(|| {
                    panic!(
                        "INVARIANT VIOLATED: method-return type index points to a vacant fact slot. \
                         This is a bug because removed type facts must be removed from every index. \
                         Fix: keep TypeStore subject indexes synchronized with the fact arena."
                    )
                });
                if fact.range.file_id != file_id || fact.provenance != TypeProvenance::Inferred {
                    continue;
                }
                fact.ruby_type = ruby_type;
                updated = updated.checked_add(1).expect(
                    "INVARIANT VIOLATED: inferred method-return update count overflowed usize. \
                     This is a bug because the count cannot exceed the bounded fact arena. \
                     Fix: keep TypeStore fact counts within addressable memory.",
                );
            }
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
        let ruby_type = self.intern_ruby_type(ruby_type);
        let Some(fact_ids) = self.facts_by_subject.get(&subject_id).cloned() else {
            return 0;
        };
        let mut updated = 0usize;
        for fact_id in fact_ids {
            let fact = self.facts[fact_id.index()].as_mut().unwrap_or_else(|| {
                panic!("INVARIANT VIOLATED: a constant-equation target points to a vacant type fact. This is a bug because file replacement must update every type index atomically. Fix: remove stale ids from facts_by_subject when a file is replaced.")
            });
            if fact.range != range {
                continue;
            }
            fact.ruby_type = ruby_type;
            updated = updated.checked_add(1).expect(
                "INVARIANT VIOLATED: constant-equation update count overflowed usize. This is a bug because it cannot exceed the bounded type arena. Fix: bound retained type facts by addressable memory.",
            );
        }
        updated
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
        let ruby_type = self.intern_ruby_type(ruby_type);
        let Some(fact_ids) = self.facts_by_file.get(&range.file_id).cloned() else {
            return 0;
        };
        let mut updated = 0usize;
        for fact_id in fact_ids {
            let matches = self.fact(fact_id).is_some_and(|fact| {
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
            });
            if !matches {
                continue;
            }
            self.facts[fact_id.index()]
                .as_mut()
                .expect(
                    "INVARIANT VIOLATED: a selected local-assignment equation target became vacant. This is a bug because resolution owns the type-store write lock. Fix: keep target selection and update in one atomic pass.",
                )
                .ruby_type = ruby_type;
            updated = updated.checked_add(1).expect(
                "INVARIANT VIOLATED: local-assignment equation update count overflowed usize. This is a bug because it cannot exceed the bounded type arena. Fix: bound retained type facts by addressable memory.",
            );
        }
        updated
    }
}
