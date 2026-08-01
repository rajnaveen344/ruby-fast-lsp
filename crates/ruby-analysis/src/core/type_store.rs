use std::collections::HashMap;

use super::file_owned_index::place_appended_file_facts;
use super::memory_estimate::{
    map_table_bytes, ruby_type_heap_bytes, type_subject_heap_bytes, vec_payload_bytes,
};
use crate::{FullyQualifiedName, RubyType};

/// Stable file identifier owned by the analysis layer.
///
/// Editor adapters can map this to URIs; agent adapters can map it to paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct SourceFileId(pub u32);

/// Byte range in a source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TextRange {
    pub file_id: SourceFileId,
    pub start_byte: u32,
    pub end_byte: u32,
}

impl TextRange {
    pub fn new(file_id: SourceFileId, start_byte: u32, end_byte: u32) -> Self {
        assert!(
            start_byte <= end_byte,
            "INVARIANT VIOLATED: TextRange start_byte must be <= end_byte. \
             This is a bug because byte ranges must be normalized before storage. \
             Fix: construct TextRange with sorted byte offsets."
        );
        Self {
            file_id,
            start_byte,
            end_byte,
        }
    }

    pub fn contains_offset(&self, file_id: SourceFileId, byte_offset: u32) -> bool {
        self.file_id == file_id && self.start_byte <= byte_offset && byte_offset <= self.end_byte
    }

    fn starts_before_or_at(&self, file_id: SourceFileId, byte_offset: u32) -> bool {
        self.file_id == file_id && self.start_byte <= byte_offset
    }
}

/// Typed program entity that can have facts.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeSubject {
    Constant(FullyQualifiedName),
    Local {
        scope_id: u32,
        name: String,
    },
    InstanceVariable {
        owner: FullyQualifiedName,
        name: String,
    },
    ClassVariable {
        owner: FullyQualifiedName,
        name: String,
    },
    GlobalVariable(String),
    MethodReturn(FullyQualifiedName),
    Parameter {
        method: FullyQualifiedName,
        name: String,
    },
    Expression(TextRange),
}

/// Where a type fact came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeProvenance {
    Literal,
    Assignment,
    Flow,
    Rbs,
    Yard,
    Runtime,
    Extension,
    Inferred,
}

/// One type assignment/narrowing fact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeFact {
    pub subject: TypeSubject,
    pub ruby_type: RubyType,
    pub range: TextRange,
    pub provenance: TypeProvenance,
}

impl TypeFact {
    pub fn new(
        subject: TypeSubject,
        ruby_type: RubyType,
        range: TextRange,
        provenance: TypeProvenance,
    ) -> Self {
        Self {
            subject,
            ruby_type,
            range,
            provenance,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredTypeFact {
    subject: TypeSubjectId,
    ruby_type: RubyType,
    range: TextRange,
    provenance: TypeProvenance,
}

/// Deterministic type query result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeResolution {
    Resolved(TypeFact),
    Ambiguous(Vec<TypeFact>),
    Unresolved,
}

/// Append-only type fact store.
#[derive(Debug, Clone)]
pub struct TypeStore {
    facts: Vec<Option<StoredTypeFact>>,
    free_facts: Vec<TypeFactId>,
    subjects: Vec<TypeSubject>,
    subject_ids: HashMap<TypeSubject, TypeSubjectId>,
    facts_by_subject: HashMap<TypeSubjectId, Vec<TypeFactId>>,
    facts_by_file: HashMap<SourceFileId, Vec<TypeFactId>>,
    file_owned_indexes_ordered: bool,
}

impl Default for TypeStore {
    fn default() -> Self {
        Self {
            facts: Vec::new(),
            free_facts: Vec::new(),
            subjects: Vec::new(),
            subject_ids: HashMap::new(),
            facts_by_subject: HashMap::new(),
            facts_by_file: HashMap::new(),
            file_owned_indexes_ordered: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct TypeFactId(usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct TypeSubjectId(usize);

impl TypeStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, fact: TypeFact) {
        // Append-only collectors preserve insertion order, not the file order
        // required by the replacement splice fast path. Once both APIs are
        // mixed, replacements must restore each touched bucket with a full
        // stable sort instead of assuming the existing prefix is ordered.
        self.file_owned_indexes_ordered = false;
        let file_id = fact.range.file_id;
        let subject = self.intern_subject(fact.subject);
        let id = self.insert_fact(StoredTypeFact {
            subject,
            ruby_type: fact.ruby_type,
            range: fact.range,
            provenance: fact.provenance,
        });
        self.facts_by_subject.entry(subject).or_default().push(id);
        self.facts_by_file.entry(file_id).or_default().push(id);
    }

    pub fn facts_for(&self, subject: &TypeSubject) -> Vec<TypeFact> {
        let Some(subject_id) = self.subject_ids.get(subject).copied() else {
            return Vec::new();
        };
        self.facts_by_subject
            .get(&subject_id)
            .map(|ids| self.clone_facts(ids))
            .unwrap_or_default()
    }

    /// Return the latest non-unknown type and its source range without
    /// materializing every fact for the subject. Ordering matches the
    /// deterministic range precedence used by callers that previously called
    /// `facts_for(...).max_by_key(...)`.
    pub fn latest_non_unknown_type_with_range(
        &self,
        subject: &TypeSubject,
    ) -> Option<(&RubyType, TextRange)> {
        let subject_id = self.subject_ids.get(subject).copied()?;
        self.facts_by_subject
            .get(&subject_id)?
            .iter()
            .filter_map(|id| self.fact(*id))
            .filter(|fact| fact.ruby_type != RubyType::Unknown)
            .max_by_key(|fact| {
                (
                    fact.range.file_id,
                    fact.range.start_byte,
                    fact.range.end_byte,
                )
            })
            .map(|fact| (&fact.ruby_type, fact.range))
    }

    pub fn all_facts(&self) -> Vec<TypeFact> {
        self.facts
            .iter()
            .filter_map(|fact| fact.as_ref())
            .map(|fact| self.expand_fact(fact))
            .collect()
    }

    /// Borrow each known method-return type in fact-arena order.
    ///
    /// This is a domain view rather than a store exposure: callers that only
    /// need method returns must not materialize and clone unrelated type facts.
    /// Arena order matches `all_facts`, preserving deterministic duplicate-key
    /// overwrite behavior when a caller collects the iterator into a map.
    pub fn known_method_return_types(
        &self,
    ) -> impl Iterator<Item = (&FullyQualifiedName, &RubyType)> {
        self.facts.iter().filter_map(|stored| {
            let fact = stored.as_ref()?;
            if fact.ruby_type == RubyType::Unknown {
                return None;
            }
            match self.subject(fact.subject) {
                TypeSubject::MethodReturn(fqn) => Some((fqn, &fact.ruby_type)),
                TypeSubject::Constant(_)
                | TypeSubject::Local { .. }
                | TypeSubject::InstanceVariable { .. }
                | TypeSubject::ClassVariable { .. }
                | TypeSubject::GlobalVariable(_)
                | TypeSubject::Parameter { .. }
                | TypeSubject::Expression(_) => None,
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

    pub fn remove_file(&mut self, file_id: SourceFileId) {
        let Some(stale_ids) = self.facts_by_file.remove(&file_id) else {
            return;
        };
        for stale_id in stale_ids {
            let Some(stale) = self.take_fact(stale_id) else {
                continue;
            };
            self.free_facts.push(stale_id);
            if let Some(ids) = self.facts_by_subject.get_mut(&stale.subject) {
                ids.retain(|id| *id != stale_id);
                if ids.is_empty() {
                    self.facts_by_subject.remove(&stale.subject);
                }
            }
        }
    }

    pub fn replace_file(
        &mut self,
        file_id: SourceFileId,
        facts: impl IntoIterator<Item = TypeFact>,
    ) {
        self.remove_file(file_id);
        let mut touched_subjects = Vec::new();
        for fact in facts {
            assert!(
                fact.range.file_id == file_id,
                "INVARIANT VIOLATED: replacement fact belongs to a different file id. \
                 This is a bug because TypeStore::replace_file must only receive facts for the target file. \
                 Fix: partition facts by SourceFileId before replacing."
            );
            let key = self.intern_subject(fact.subject);
            if let Some((_, appended_count)) = touched_subjects
                .iter_mut()
                .find(|(touched, _)| *touched == key)
            {
                *appended_count += 1;
            } else {
                touched_subjects.push((key, 1));
            }
            let id = self.insert_fact(StoredTypeFact {
                subject: key,
                ruby_type: fact.ruby_type,
                range: fact.range,
                provenance: fact.provenance,
            });
            self.facts_by_subject.entry(key).or_default().push(id);
            self.facts_by_file.entry(file_id).or_default().push(id);
        }
        for (subject, appended_count) in touched_subjects {
            if let Some(ids) = self.facts_by_subject.get_mut(&subject) {
                if self.file_owned_indexes_ordered {
                    place_appended_file_facts(
                        ids,
                        appended_count,
                        file_id,
                        |id| {
                            self.facts[id.0]
                                .as_ref()
                                .expect(
                                    "INVARIANT VIOLATED: type index points to missing fact. \
                                     This is a bug because indexes must be removed before arena facts. \
                                     Fix: remove stale ids from every TypeStore index.",
                                )
                                .range
                                .file_id
                        },
                        |appended| sort_type_ids(&self.facts, appended),
                    );
                } else {
                    sort_type_ids(&self.facts, ids);
                }
                ids.shrink_to_fit();
            }
        }
        if let Some(ids) = self.facts_by_file.get_mut(&file_id) {
            sort_type_ids_by_file(&self.facts, ids);
            ids.shrink_to_fit();
        }
    }

    pub fn estimated_heap_bytes(&self) -> usize {
        vec_payload_bytes(&self.facts)
            + vec_payload_bytes(&self.free_facts)
            + vec_payload_bytes(&self.subjects)
            + self
                .subjects
                .iter()
                .map(type_subject_heap_bytes)
                .sum::<usize>()
            + map_table_bytes(&self.subject_ids)
            + self
                .subject_ids
                .keys()
                .map(type_subject_heap_bytes)
                .sum::<usize>()
            + self
                .facts
                .iter()
                .filter_map(|fact| fact.as_ref())
                .map(type_fact_heap_bytes)
                .sum::<usize>()
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

    pub fn shrink_to_fit(&mut self) {
        self.facts.shrink_to_fit();
        self.free_facts.shrink_to_fit();
        self.subjects.shrink_to_fit();
        self.subject_ids.shrink_to_fit();
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
        let Some(subject_id) = self.subject_ids.get(subject).copied() else {
            return TypeResolution::Unresolved;
        };
        let Some(ids) = self.facts_by_subject.get(&subject_id) else {
            return TypeResolution::Unresolved;
        };

        let Some(latest_start) = ids
            .iter()
            .filter_map(|id| self.fact(*id))
            .filter(|fact| fact.range.starts_before_or_at(file_id, byte_offset))
            .map(|fact| fact.range.start_byte)
            .max()
        else {
            return TypeResolution::Unresolved;
        };

        let mut candidates: Vec<TypeFact> = ids
            .iter()
            .filter_map(|id| self.fact(*id))
            .filter(|fact| fact.range.file_id == file_id && fact.range.start_byte == latest_start)
            .map(|fact| self.expand_fact(fact))
            .collect();

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
            let slot = self.facts.get_mut(id.0).expect(
                "INVARIANT VIOLATED: type free list points outside fact arena. \
                 This is a bug because free ids must come from previous arena slots. \
                 Fix: only push ids returned by TypeStore::take_fact.",
            );
            assert!(
                slot.is_none(),
                "INVARIANT VIOLATED: type free list points to occupied fact slot. \
                 This is a bug because free ids must only reference removed type facts. \
                 Fix: push each removed type id at most once."
            );
            *slot = Some(fact);
            return id;
        }
        let id = TypeFactId(self.facts.len());
        self.facts.push(Some(fact));
        id
    }

    fn fact(&self, id: TypeFactId) -> Option<&StoredTypeFact> {
        self.facts.get(id.0).and_then(Option::as_ref)
    }

    fn take_fact(&mut self, id: TypeFactId) -> Option<StoredTypeFact> {
        self.facts.get_mut(id.0).and_then(Option::take)
    }

    fn clone_facts(&self, ids: &[TypeFactId]) -> Vec<TypeFact> {
        ids.iter()
            .filter_map(|id| self.fact(*id))
            .map(|fact| self.expand_fact(fact))
            .collect()
    }

    fn intern_subject(&mut self, subject: TypeSubject) -> TypeSubjectId {
        if let Some(id) = self.subject_ids.get(&subject) {
            return *id;
        }
        let id = TypeSubjectId(self.subjects.len());
        self.subjects.push(subject.clone());
        self.subject_ids.insert(subject, id);
        id
    }

    fn subject(&self, id: TypeSubjectId) -> &TypeSubject {
        self.subjects.get(id.0).expect(
            "INVARIANT VIOLATED: type fact points to missing subject id. \
             This is a bug because type facts must only store interned subject ids. \
             Fix: intern type subjects before inserting facts.",
        )
    }

    fn expand_fact(&self, fact: &StoredTypeFact) -> TypeFact {
        TypeFact {
            subject: self.subject(fact.subject).clone(),
            ruby_type: fact.ruby_type.clone(),
            range: fact.range,
            provenance: fact.provenance,
        }
    }
}

fn type_fact_heap_bytes(fact: &StoredTypeFact) -> usize {
    ruby_type_heap_bytes(&fact.ruby_type)
}

fn sort_type_ids(facts: &[Option<StoredTypeFact>], ids: &mut [TypeFactId]) {
    ids.sort_by_key(|id| {
        let fact = facts[id.0].as_ref().expect(
            "INVARIANT VIOLATED: type index points to missing fact. \
             This is a bug because indexes must be removed before arena facts. \
             Fix: remove stale ids from every TypeStore index.",
        );
        (
            fact.range.file_id,
            fact.range.start_byte,
            fact.range.end_byte,
            provenance_rank(fact.provenance),
        )
    });
}

fn sort_type_ids_by_file(facts: &[Option<StoredTypeFact>], ids: &mut [TypeFactId]) {
    ids.sort_by_key(|id| {
        let fact = facts[id.0].as_ref().expect(
            "INVARIANT VIOLATED: type file index points to missing fact. \
             This is a bug because indexes must be removed before arena facts. \
             Fix: remove stale ids from every TypeStore index.",
        );
        (
            fact.range.start_byte,
            fact.range.end_byte,
            provenance_rank(fact.provenance),
        )
    });
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
mod tests {
    use crate::{FullyQualifiedName, RubyConstant, RubyMethod};

    use super::*;

    fn file() -> SourceFileId {
        SourceFileId(1)
    }

    fn constant_subject(name: &str) -> TypeSubject {
        TypeSubject::Constant(FullyQualifiedName::constant(vec![
            RubyConstant::new(name).unwrap()
        ]))
    }

    fn method_return_subject(owner: &str, name: &str) -> TypeSubject {
        TypeSubject::MethodReturn(FullyQualifiedName::method(
            vec![RubyConstant::new(owner).unwrap()],
            RubyMethod::new(name).unwrap(),
        ))
    }

    #[test]
    fn resolves_latest_fact_before_position() {
        let subject = constant_subject("VALUE");
        let mut store = TypeStore::new();
        store.add(TypeFact::new(
            subject.clone(),
            RubyType::integer(),
            TextRange::new(file(), 0, 8),
            TypeProvenance::Literal,
        ));
        store.add(TypeFact::new(
            subject.clone(),
            RubyType::string(),
            TextRange::new(file(), 20, 32),
            TypeProvenance::Literal,
        ));

        assert!(matches!(
            store.type_at(&subject, file(), 12),
            TypeResolution::Resolved(TypeFact {
                ruby_type: RubyType::Class(_),
                ..
            })
        ));

        match store.type_at(&subject, file(), 40) {
            TypeResolution::Resolved(fact) => assert_eq!(fact.ruby_type, RubyType::string()),
            other => panic!("expected resolved latest fact, got {other:?}"),
        }
    }

    #[test]
    fn unresolved_when_no_fact_exists() {
        let store = TypeStore::new();
        assert_eq!(
            store.type_at(&constant_subject("MISSING"), file(), 0),
            TypeResolution::Unresolved
        );
    }

    #[test]
    fn latest_non_unknown_type_with_range_returns_only_the_winning_fact() {
        let subject = constant_subject("VALUE");
        let mut store = TypeStore::new();
        store.add(TypeFact::new(
            subject.clone(),
            RubyType::string(),
            TextRange::new(file(), 0, 8),
            TypeProvenance::Assignment,
        ));
        store.add(TypeFact::new(
            subject.clone(),
            RubyType::Unknown,
            TextRange::new(file(), 30, 38),
            TypeProvenance::Inferred,
        ));
        store.add(TypeFact::new(
            subject.clone(),
            RubyType::integer(),
            TextRange::new(file(), 20, 28),
            TypeProvenance::Assignment,
        ));

        let (ruby_type, range) = store
            .latest_non_unknown_type_with_range(&subject)
            .expect("the latest known type must be returned");
        assert_eq!(*ruby_type, RubyType::integer());
        assert_eq!(range, TextRange::new(file(), 20, 28));
        assert!(store
            .latest_non_unknown_type_with_range(&constant_subject("MISSING"))
            .is_none());
    }

    #[test]
    fn known_method_return_types_borrows_only_known_returns_in_arena_order() {
        let first = method_return_subject("First", "call");
        let unknown = method_return_subject("Unknown", "call");
        let second = method_return_subject("Second", "call");
        let mut store = TypeStore::new();
        store.add(TypeFact::new(
            first.clone(),
            RubyType::string(),
            TextRange::new(SourceFileId(1), 0, 8),
            TypeProvenance::Inferred,
        ));
        store.add(TypeFact::new(
            constant_subject("IGNORED"),
            RubyType::integer(),
            TextRange::new(SourceFileId(1), 10, 18),
            TypeProvenance::Assignment,
        ));
        store.add(TypeFact::new(
            unknown,
            RubyType::Unknown,
            TextRange::new(SourceFileId(1), 20, 28),
            TypeProvenance::Inferred,
        ));
        store.add(TypeFact::new(
            second.clone(),
            RubyType::integer(),
            TextRange::new(SourceFileId(2), 0, 8),
            TypeProvenance::Inferred,
        ));

        let returns = store.known_method_return_types().collect::<Vec<_>>();
        let TypeSubject::MethodReturn(first_fqn) = first else {
            panic!("test method subject must be a method return")
        };
        let TypeSubject::MethodReturn(second_fqn) = second else {
            panic!("test method subject must be a method return")
        };
        assert_eq!(
            returns,
            vec![
                (&first_fqn, &RubyType::string()),
                (&second_fqn, &RubyType::integer()),
            ]
        );
    }

    #[test]
    fn replace_file_removes_stale_facts_for_same_file_only() {
        let subject = constant_subject("VALUE");
        let other_subject = constant_subject("OTHER");
        let mut store = TypeStore::new();
        store.add(TypeFact::new(
            subject.clone(),
            RubyType::integer(),
            TextRange::new(file(), 0, 8),
            TypeProvenance::Assignment,
        ));
        store.add(TypeFact::new(
            other_subject.clone(),
            RubyType::string(),
            TextRange::new(SourceFileId(2), 0, 8),
            TypeProvenance::Assignment,
        ));

        store.replace_file(
            file(),
            [TypeFact::new(
                subject.clone(),
                RubyType::string(),
                TextRange::new(file(), 10, 18),
                TypeProvenance::Assignment,
            )],
        );

        assert_eq!(
            store.type_at(&subject, file(), 4),
            TypeResolution::Unresolved
        );
        match store.type_at(&subject, file(), 14) {
            TypeResolution::Resolved(fact) => assert_eq!(fact.ruby_type, RubyType::string()),
            other => panic!("expected replacement fact, got {other:?}"),
        }
        match store.type_at(&other_subject, SourceFileId(2), 4) {
            TypeResolution::Resolved(fact) => assert_eq!(fact.ruby_type, RubyType::string()),
            other => panic!("expected other file fact to survive, got {other:?}"),
        }
    }

    #[test]
    fn replace_file_restores_order_after_append_only_additions() {
        let subject = constant_subject("VALUE");
        let mut store = TypeStore::new();
        store.add(TypeFact::new(
            subject.clone(),
            RubyType::string(),
            TextRange::new(SourceFileId(3), 0, 8),
            TypeProvenance::Assignment,
        ));
        store.add(TypeFact::new(
            subject.clone(),
            RubyType::integer(),
            TextRange::new(SourceFileId(1), 0, 8),
            TypeProvenance::Assignment,
        ));

        store.replace_file(
            SourceFileId(2),
            [TypeFact::new(
                subject.clone(),
                RubyType::boolean(),
                TextRange::new(SourceFileId(2), 0, 8),
                TypeProvenance::Assignment,
            )],
        );

        assert_eq!(
            store
                .facts_for(&subject)
                .into_iter()
                .map(|fact| fact.range.file_id)
                .collect::<Vec<_>>(),
            vec![SourceFileId(1), SourceFileId(2), SourceFileId(3)]
        );
    }

    #[test]
    fn ambiguous_when_same_position_has_multiple_types() {
        let subject = constant_subject("VALUE");
        let mut store = TypeStore::new();
        store.add(TypeFact::new(
            subject.clone(),
            RubyType::integer(),
            TextRange::new(file(), 0, 8),
            TypeProvenance::Literal,
        ));
        store.add(TypeFact::new(
            subject.clone(),
            RubyType::string(),
            TextRange::new(file(), 0, 8),
            TypeProvenance::Extension,
        ));

        match store.type_at(&subject, file(), 4) {
            TypeResolution::Ambiguous(facts) => assert_eq!(facts.len(), 2),
            other => panic!("expected ambiguous facts, got {other:?}"),
        }
    }

    #[test]
    #[should_panic(expected = "INVARIANT VIOLATED: TextRange start_byte must be <= end_byte")]
    fn invalid_range_panics() {
        let _ = TextRange::new(file(), 10, 9);
    }
}
