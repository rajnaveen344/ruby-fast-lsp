//! Append-only type fact store with interned subjects and Ruby types.

mod compact;
mod facts;
mod ordered_append;
mod replacement;
mod updates;

use crate::invariant::ExpectInvariant;
pub(crate) use compact::RubyTypeId;
use compact::{StoredTypeFact, StoredTypeSubject, TypeFactId, TypeSubjectId};
pub(crate) use facts::NamedTypeResolution;
pub use facts::{SourceFileId, TextRange, TypeFact, TypeProvenance, TypeResolution, TypeSubject};

use std::collections::HashMap;

use crate::core::storage::interner::SharedInterner;

use crate::core::storage::memory_estimate::{
    map_table_bytes, ruby_type_heap_bytes, type_subject_heap_bytes, vec_payload_bytes,
};
use crate::core::{FullyQualifiedName, RubyType};

/// Append-only type fact store.
#[derive(Debug, Clone)]
pub struct TypeStore {
    facts: Vec<Option<StoredTypeFact>>,
    free_facts: Vec<TypeFactId>,
    subjects: SharedInterner<TypeSubject>,
    ruby_types: SharedInterner<RubyType>,
    facts_by_subject: HashMap<TypeSubjectId, Vec<TypeFactId>>,
    facts_by_file: HashMap<SourceFileId, Vec<TypeFactId>>,
    file_owned_indexes_ordered: bool,
}

impl Default for TypeStore {
    fn default() -> Self {
        Self {
            facts: Vec::new(),
            free_facts: Vec::new(),
            subjects: SharedInterner::default(),
            ruby_types: SharedInterner::default(),
            facts_by_subject: HashMap::new(),
            facts_by_file: HashMap::new(),
            file_owned_indexes_ordered: true,
        }
    }
}

impl TypeStore {
    pub fn add(&mut self, fact: TypeFact) {
        // Append-only collectors preserve insertion order, not the file order
        // required by the replacement splice fast path. Once both APIs are
        // mixed, replacements must restore each touched bucket with a full
        // stable sort instead of assuming the existing prefix is ordered.
        self.file_owned_indexes_ordered = false;
        let file_id = fact.range.file_id;
        let subject = self.store_subject(fact.subject, fact.range);
        let ruby_type = self.intern_ruby_type(fact.ruby_type);
        let id = self.insert_fact(StoredTypeFact {
            subject,
            ruby_type,
            range: fact.range,
            provenance: fact.provenance,
        });
        if let Some(subject_id) = subject.interned_id() {
            self.facts_by_subject
                .entry(subject_id)
                .or_default()
                .push(id);
        }
        self.facts_by_file.entry(file_id).or_default().push(id);
    }

    pub fn facts_for(&self, subject: &TypeSubject) -> Vec<TypeFact> {
        match subject {
            TypeSubject::Expression(range) => self
                .facts_by_file
                .get(&range.file_id)
                .map(|ids| self.clone_expression_facts(ids, *range))
                .unwrap_or_default(),
            TypeSubject::Constant(_)
            | TypeSubject::Local { .. }
            | TypeSubject::InstanceVariable { .. }
            | TypeSubject::ClassVariable { .. }
            | TypeSubject::GlobalVariable(_)
            | TypeSubject::MethodReturn(_)
            | TypeSubject::Parameter { .. } => {
                let Some(subject_id) = self.subject_id(subject) else {
                    return Vec::new();
                };
                self.facts_by_subject
                    .get(&subject_id)
                    .map(|ids| self.clone_facts(ids))
                    .unwrap_or_default()
            }
        }
    }

    /// Return the latest non-unknown type and its source range without
    /// materializing every fact for the subject. Ordering matches the
    /// deterministic range precedence used by callers that previously called
    /// `facts_for(...).max_by_key(...)`.
    pub fn latest_non_unknown_type_with_range(
        &self,
        subject: &TypeSubject,
    ) -> Option<(&RubyType, TextRange)> {
        self.fact_ids_for_subject(subject)?
            .iter()
            .filter_map(|id| self.fact(*id))
            .filter(|fact| self.stored_subject_matches(fact, subject))
            .filter(|fact| self.ruby_type(fact.ruby_type) != &RubyType::Unknown)
            .max_by_key(|fact| {
                (
                    fact.range.file_id,
                    fact.range.start_byte,
                    fact.range.end_byte,
                )
            })
            .map(|fact| (self.ruby_type(fact.ruby_type), fact.range))
    }

    /// The one type every fact for `subject` outside `excluded` agrees on,
    /// read from the subject index without expanding unrelated facts.
    pub fn agreed_type_outside_file(
        &self,
        subject: &TypeSubject,
        excluded: SourceFileId,
    ) -> Option<&RubyType> {
        let mut agreed = None;
        for fact in self
            .fact_ids_for_subject(subject)?
            .iter()
            .filter_map(|id| self.fact(*id))
            .filter(|fact| fact.range.file_id != excluded)
            .filter(|fact| self.stored_subject_matches(fact, subject))
        {
            match agreed {
                None => agreed = Some(fact.ruby_type),
                Some(ruby_type) if ruby_type == fact.ruby_type => {}
                Some(_) => return None,
            }
        }
        agreed.map(|ruby_type| self.ruby_type(ruby_type))
    }

    pub fn all_facts(&self) -> Vec<TypeFact> {
        self.facts
            .iter()
            .filter_map(|fact| fact.as_ref())
            .map(|fact| self.expand_fact(fact))
            .collect()
    }

    /// Borrow each method-return type in fact-arena order, including Unknown.
    ///
    /// This is a domain view rather than a store exposure: callers that only
    /// need method returns must not materialize and clone unrelated type facts.
    /// Arena order matches `all_facts`, preserving deterministic duplicate-key
    /// overwrite behavior when a caller collects the iterator into a map.
    ///
    /// Unknown is retained because an in-progress file replacement must treat
    /// a newly invalidated local method as authoritative. Dropping it would let
    /// the collector fall back to the previous engine snapshot and resurrect a
    /// stale return type while deriving callers later in the same file.
    pub fn method_return_types(&self) -> impl Iterator<Item = (&FullyQualifiedName, &RubyType)> {
        self.facts.iter().filter_map(|stored| {
            let fact = stored.as_ref()?;
            let ruby_type = self.ruby_type(fact.ruby_type);
            match fact.subject.interned_id() {
                Some(subject_id) => match self.subject(subject_id) {
                    TypeSubject::MethodReturn(fqn) => Some((fqn, ruby_type)),
                    TypeSubject::Constant(_)
                    | TypeSubject::Local { .. }
                    | TypeSubject::InstanceVariable { .. }
                    | TypeSubject::ClassVariable { .. }
                    | TypeSubject::GlobalVariable(_)
                    | TypeSubject::Parameter { .. } => None,
                    TypeSubject::Expression(_) => unreachable_invariant!(
                        what = "an expression subject was inserted into the general type-subject interner",
                        why = "expressions must use their compact file-local range identity",
                        fix = "route every inserted TypeSubject through TypeStore::store_subject",
                    ),
                },
                None => None,
            }
        })
    }

    /// Borrow each value-constant type in fact-arena order, including Unknown.
    ///
    /// This is a domain view rather than a store exposure: callers that only
    /// need constant types must not materialize and clone unrelated type facts.
    /// Arena order matches `all_facts` filtered to `TypeSubject::Constant`.
    pub fn constant_type_facts(
        &self,
    ) -> impl Iterator<Item = (&FullyQualifiedName, TextRange, &RubyType)> {
        self.facts.iter().filter_map(|stored| {
            let fact = stored.as_ref()?;
            let ruby_type = self.ruby_type(fact.ruby_type);
            match fact.subject.interned_id() {
                Some(subject_id) => match self.subject(subject_id) {
                    TypeSubject::Constant(fqn) => Some((fqn, fact.range, ruby_type)),
                    TypeSubject::Local { .. }
                    | TypeSubject::InstanceVariable { .. }
                    | TypeSubject::ClassVariable { .. }
                    | TypeSubject::GlobalVariable(_)
                    | TypeSubject::MethodReturn(_)
                    | TypeSubject::Parameter { .. } => None,
                    TypeSubject::Expression(_) => unreachable_invariant!(
                        what = "an expression subject was inserted into the general type-subject interner",
                        why = "expressions must use their compact file-local range identity",
                        fix = "route every inserted TypeSubject through TypeStore::store_subject",
                    ),
                },
                None => None,
            }
        })
    }

    pub fn fact_count(&self) -> usize {
        self.facts.iter().filter(|fact| fact.is_some()).count()
    }

    pub fn facts_in_file(&self, file_id: SourceFileId) -> Vec<TypeFact> {
        self.facts_by_file
            .get(&file_id)
            .map(|ids| self.clone_facts(ids))
            .unwrap_or_default()
    }

    /// Resolve the latest named type fact selected by one file-local domain
    /// predicate without materializing unrelated facts.
    ///
    /// The predicate receives only interned, non-expression subjects together
    /// with their exact source range. Callers may therefore exclude a write
    /// whose right-hand side is still being evaluated without exposing the
    /// store's compact ids or indexes. Facts at the same latest start offset
    /// resolve when their Ruby types agree and remain ambiguous when they do
    /// not, matching the reaching-assignment proof rule.
    pub(crate) fn named_type_in_file_before_matching(
        &self,
        file_id: SourceFileId,
        byte_offset: u32,
        mut matches: impl FnMut(&TypeSubject, TextRange) -> bool,
    ) -> NamedTypeResolution<'_> {
        let Some(ids) = self.facts_by_file.get(&file_id) else {
            return NamedTypeResolution::Unresolved;
        };

        let mut latest_start = None;
        let mut latest_type = None;
        let mut ambiguous = false;
        for id in ids {
            let Some(fact) = self.fact(*id) else {
                continue;
            };
            if fact.range.start_byte > byte_offset {
                continue;
            }
            let Some(subject_id) = fact.subject.interned_id() else {
                continue;
            };
            if !matches(self.subject(subject_id), fact.range) {
                continue;
            }

            match latest_start {
                None => {
                    latest_start = Some(fact.range.start_byte);
                    latest_type = Some(fact.ruby_type);
                }
                Some(start) if fact.range.start_byte > start => {
                    latest_start = Some(fact.range.start_byte);
                    latest_type = Some(fact.ruby_type);
                    ambiguous = false;
                }
                Some(start) if fact.range.start_byte == start => {
                    if latest_type != Some(fact.ruby_type) {
                        ambiguous = true;
                    }
                }
                Some(_) => {}
            }
        }

        let Some(latest_type) = latest_type else {
            return NamedTypeResolution::Unresolved;
        };
        if ambiguous {
            return NamedTypeResolution::Ambiguous;
        }

        NamedTypeResolution::Resolved(self.ruby_type(latest_type))
    }

    /// Borrow only type payloads retained by one file. Engine telemetry uses
    /// this domain view so an observational refresh does not clone every fact
    /// on the indexing or typing path.
    pub(crate) fn ruby_types_in_file(
        &self,
        file_id: SourceFileId,
    ) -> impl Iterator<Item = &RubyType> {
        self.facts_by_file
            .get(&file_id)
            .into_iter()
            .flatten()
            .filter_map(|id| self.fact(*id))
            .map(|fact| self.ruby_type(fact.ruby_type))
    }

    pub fn estimated_heap_bytes(&self) -> usize {
        vec_payload_bytes(&self.facts)
            + vec_payload_bytes(&self.free_facts)
            + self.subjects.estimated_heap_bytes(type_subject_heap_bytes)
            + self.ruby_types.estimated_heap_bytes(ruby_type_heap_bytes)
            + map_table_bytes(&self.facts_by_subject)
            + map_table_bytes(&self.facts_by_file)
            + self
                .facts_by_subject
                .values()
                .map(vec_payload_bytes)
                .sum::<usize>()
            + self
                .facts_by_file
                .values()
                .map(vec_payload_bytes)
                .sum::<usize>()
    }

    /// Share interned subjects and types with every clone of this store.
    pub(crate) fn freeze(&mut self) {
        self.subjects.freeze();
        self.ruby_types.freeze();
    }

    pub fn shrink_to_fit(&mut self) {
        self.facts.shrink_to_fit();
        self.free_facts.shrink_to_fit();
        self.subjects.shrink_to_fit();
        self.ruby_types.shrink_to_fit();
        self.facts_by_subject.shrink_to_fit();
        self.facts_by_file.shrink_to_fit();
        for ids in self.facts_by_subject.values_mut() {
            ids.shrink_to_fit();
        }
        for ids in self.facts_by_file.values_mut() {
            ids.shrink_to_fit();
        }
    }

    pub fn type_at(
        &self,
        subject: &TypeSubject,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> TypeResolution {
        if let TypeSubject::Expression(range) = subject {
            let Some(ids) = self.facts_by_file.get(&range.file_id) else {
                return TypeResolution::Unresolved;
            };
            return self.resolution_from_expanded_facts(self.clone_expression_facts(ids, *range));
        }

        let Some(ids) = self.fact_ids_for_subject(subject) else {
            return TypeResolution::Unresolved;
        };

        let Some(latest_start) = ids
            .iter()
            .filter_map(|id| self.fact(*id))
            .filter(|fact| self.stored_subject_matches(fact, subject))
            .filter(|fact| fact.range.starts_before_or_at(file_id, byte_offset))
            .map(|fact| fact.range.start_byte)
            .max()
        else {
            return TypeResolution::Unresolved;
        };

        let candidates = ids
            .iter()
            .filter_map(|id| self.fact(*id))
            .filter(|fact| self.stored_subject_matches(fact, subject))
            .filter(|fact| fact.range.file_id == file_id && fact.range.start_byte == latest_start)
            .map(|fact| self.expand_fact(fact))
            .collect();
        self.resolution_from_expanded_facts(candidates)
    }

    fn resolution_from_expanded_facts(&self, mut candidates: Vec<TypeFact>) -> TypeResolution {
        candidates
            .sort_by_key(|fact| (fact.ruby_type.to_string(), provenance_rank(fact.provenance)));
        candidates.dedup_by(|a, b| a.ruby_type == b.ruby_type && a.provenance == b.provenance);
        match candidates.len() {
            0 => TypeResolution::Unresolved,
            1 => TypeResolution::Resolved(candidates.remove(0)),
            _ => TypeResolution::Ambiguous(candidates),
        }
    }

    fn insert_fact(&mut self, fact: StoredTypeFact) -> TypeFactId {
        if let Some(id) = self.free_facts.pop() {
            let slot = self.facts.get_mut(id.index()).expect_invariant(
                "type free list points outside fact arena",
                "free ids must come from previous arena slots",
                "only push ids returned by TypeStore::take_fact",
            );
            invariant!(
                slot.is_none(),
                what = "type free list points to occupied fact slot",
                why = "free ids must only reference removed type facts",
                fix = "push each removed type id at most once",
            );
            *slot = Some(fact);
            return id;
        }
        let id = TypeFactId::from_index(self.facts.len());
        self.facts.push(Some(fact));
        id
    }

    fn fact(&self, id: TypeFactId) -> Option<&StoredTypeFact> {
        self.facts.get(id.index()).and_then(Option::as_ref)
    }

    fn take_fact(&mut self, id: TypeFactId) -> Option<StoredTypeFact> {
        self.facts.get_mut(id.index()).and_then(Option::take)
    }

    fn clone_facts(&self, ids: &[TypeFactId]) -> Vec<TypeFact> {
        ids.iter()
            .filter_map(|id| self.fact(*id))
            .map(|fact| self.expand_fact(fact))
            .collect()
    }

    fn clone_expression_facts(&self, ids: &[TypeFactId], range: TextRange) -> Vec<TypeFact> {
        if self.file_owned_indexes_ordered {
            return self.clone_sorted_expression_facts(ids, range);
        }
        ids.iter()
            .filter_map(|id| self.fact(*id))
            .filter(|fact| fact.subject.is_expression() && fact.range == range)
            .map(|fact| self.expand_fact(fact))
            .collect()
    }

    /// Exact-range expression lookup on a file bucket ordered by
    /// `(start_byte, end_byte, provenance)`.
    ///
    /// Production `replace_file` keeps that order. Scanning every fact in the
    /// file for `TypeSubject::Expression` is the deferred-receiver resolve
    /// hotspot: `exact_expression_type` → `type_at` on example-workspace. Named subjects
    /// keep their interned buckets; expressions are range-owned and must not
    /// borrow that file-wide scan. `TypeStore::add` may unsort the bucket, so
    /// callers fall back to a linear scan when `file_owned_indexes_ordered` is
    /// false.
    fn clone_sorted_expression_facts(&self, ids: &[TypeFactId], range: TextRange) -> Vec<TypeFact> {
        let target = (range.start_byte, range.end_byte);
        let start = ids.partition_point(|id| {
            let fact = self.indexed_file_fact(*id);
            (fact.range.start_byte, fact.range.end_byte) < target
        });
        let mut facts = Vec::new();
        for id in &ids[start..] {
            let fact = *self.indexed_file_fact(*id);
            if (fact.range.start_byte, fact.range.end_byte) != target {
                break;
            }
            if fact.subject.is_expression() {
                facts.push(self.expand_fact(&fact));
            }
        }
        facts
    }

    fn indexed_file_fact(&self, id: TypeFactId) -> &StoredTypeFact {
        self.fact(id).expect_invariant(
            "type file index points to missing fact",
            "indexes must be removed before arena facts",
            "remove stale ids from every TypeStore index",
        )
    }

    fn store_subject(&mut self, subject: TypeSubject, fact_range: TextRange) -> StoredTypeSubject {
        match subject {
            TypeSubject::Expression(range) => {
                invariant!(
                    range == fact_range,
                    what = "expression subject range differs from its type fact range",
                    why = "compact expression identity reuses the fact's existing range",
                    fix = "construct the expression subject and fact from the same AST location",
                );
                StoredTypeSubject::expression()
            }
            TypeSubject::Constant(_)
            | TypeSubject::Local { .. }
            | TypeSubject::InstanceVariable { .. }
            | TypeSubject::ClassVariable { .. }
            | TypeSubject::GlobalVariable(_)
            | TypeSubject::MethodReturn(_)
            | TypeSubject::Parameter { .. } => {
                let index = self.subjects.intern(subject);
                StoredTypeSubject::interned(TypeSubjectId::from_index(index))
            }
        }
    }

    fn fact_ids_for_subject(&self, subject: &TypeSubject) -> Option<&[TypeFactId]> {
        match subject {
            TypeSubject::Expression(range) => {
                self.facts_by_file.get(&range.file_id).map(Vec::as_slice)
            }
            TypeSubject::Constant(_)
            | TypeSubject::Local { .. }
            | TypeSubject::InstanceVariable { .. }
            | TypeSubject::ClassVariable { .. }
            | TypeSubject::GlobalVariable(_)
            | TypeSubject::MethodReturn(_)
            | TypeSubject::Parameter { .. } => self
                .subject_id(subject)
                .and_then(|subject_id| self.facts_by_subject.get(&subject_id))
                .map(Vec::as_slice),
        }
    }

    fn stored_subject_matches(&self, fact: &StoredTypeFact, subject: &TypeSubject) -> bool {
        match (fact.subject.interned_id(), subject) {
            (None, TypeSubject::Expression(expected)) => fact.range == *expected,
            (Some(_), TypeSubject::Expression(_))
            | (None, TypeSubject::Constant(_))
            | (None, TypeSubject::Local { .. })
            | (None, TypeSubject::InstanceVariable { .. })
            | (None, TypeSubject::ClassVariable { .. })
            | (None, TypeSubject::GlobalVariable(_))
            | (None, TypeSubject::MethodReturn(_))
            | (None, TypeSubject::Parameter { .. }) => false,
            (Some(stored), expected) => self.subject(stored) == expected,
        }
    }

    fn subject_id(&self, subject: &TypeSubject) -> Option<TypeSubjectId> {
        self.subjects
            .get_index_of(subject)
            .map(TypeSubjectId::from_index)
    }

    fn subject(&self, id: TypeSubjectId) -> &TypeSubject {
        self.subjects.get_index(id.index()).expect_invariant(
            "type fact points to missing subject id",
            "type facts must only store interned subject ids",
            "intern type subjects before inserting facts",
        )
    }

    pub(crate) fn intern_ruby_type(&mut self, ruby_type: RubyType) -> RubyTypeId {
        let index = self.ruby_types.intern(ruby_type);
        RubyTypeId::from_index(index)
    }

    pub(crate) fn ruby_type(&self, id: RubyTypeId) -> &RubyType {
        self.ruby_types.get_index(id.index()).expect_invariant(
            "type fact points to missing Ruby type id",
            "facts reference only interned Ruby types",
            "intern types before inserting facts; keep the interner append-only",
        )
    }

    fn expand_fact(&self, fact: &StoredTypeFact) -> TypeFact {
        TypeFact {
            subject: match fact.subject.interned_id() {
                Some(subject_id) => self.subject(subject_id).clone(),
                None => TypeSubject::Expression(fact.range),
            },
            ruby_type: self.ruby_type(fact.ruby_type).clone(),
            range: fact.range,
            provenance: fact.provenance,
        }
    }
}

fn provenance_rank(provenance: TypeProvenance) -> u8 {
    match provenance {
        TypeProvenance::Literal => 0,
        TypeProvenance::Assignment => 1,
        TypeProvenance::Flow => 2,
        TypeProvenance::Rbs => 3,
        TypeProvenance::Yard => 4,
        TypeProvenance::Runtime => 5,
        TypeProvenance::Extension => 6,
        TypeProvenance::Inferred => 7,
    }
}

#[cfg(test)]
mod tests;
