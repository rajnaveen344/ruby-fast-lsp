use std::collections::HashMap;
use std::mem::size_of;

use indexmap::IndexSet;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
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
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
    subject: StoredTypeSubject,
    ruby_type: RubyTypeId,
    range: TextRange,
    provenance: TypeProvenance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct StoredTypeSubject(u32);

impl StoredTypeSubject {
    const EXPRESSION_TAG: u32 = 1 << 31;

    fn interned(id: TypeSubjectId) -> Self {
        assert!(
            id.0 < Self::EXPRESSION_TAG,
            "INVARIANT VIOLATED: the non-expression type subject interner exceeded the compact 31-bit id space. This is a bug because the high bit distinguishes range-owned expression facts. Fix: widen StoredTypeSubject and every stored subject reference together before interning 2^31 subjects."
        );
        Self(id.0)
    }

    fn expression() -> Self {
        Self(Self::EXPRESSION_TAG)
    }

    fn interned_id(self) -> Option<TypeSubjectId> {
        if self.0 == Self::EXPRESSION_TAG {
            None
        } else {
            assert!(
                self.0 < Self::EXPRESSION_TAG,
                "INVARIANT VIOLATED: stored type subject has an unknown compact tag. This is a bug because only interned ids and the expression tag are valid. Fix: construct stored subjects through StoredTypeSubject::interned or StoredTypeSubject::expression."
            );
            Some(TypeSubjectId(self.0))
        }
    }

    fn is_expression(self) -> bool {
        self.interned_id().is_none()
    }
}

/// Deterministic type query result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeResolution {
    Resolved(TypeFact),
    Ambiguous(Vec<TypeFact>),
    Unresolved,
}

/// Borrowed result for one internal named-fact selection.
///
/// This keeps hot indexing queries allocation-free without exposing compact
/// store ids or indexes as semantic API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NamedTypeResolution<'a> {
    Resolved(&'a RubyType),
    Ambiguous,
    Unresolved,
}

/// Append-only type fact store.
#[derive(Debug, Clone)]
pub struct TypeStore {
    facts: Vec<Option<StoredTypeFact>>,
    free_facts: Vec<TypeFactId>,
    subjects: IndexSet<TypeSubject>,
    ruby_types: IndexSet<RubyType>,
    facts_by_subject: HashMap<TypeSubjectId, Vec<TypeFactId>>,
    facts_by_file: HashMap<SourceFileId, Vec<TypeFactId>>,
    file_owned_indexes_ordered: bool,
}

impl Default for TypeStore {
    fn default() -> Self {
        Self {
            facts: Vec::new(),
            free_facts: Vec::new(),
            subjects: IndexSet::new(),
            ruby_types: IndexSet::new(),
            facts_by_subject: HashMap::new(),
            facts_by_file: HashMap::new(),
            file_owned_indexes_ordered: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct TypeFactId(u32);

impl TypeFactId {
    fn from_index(index: usize) -> Self {
        Self(u32::try_from(index).expect(
            "INVARIANT VIOLATED: type fact arena exceeded u32 ids. This is a bug because the \
             retained type indexes use bounded compact ids. Fix: widen TypeFactId and every \
             stored type-fact index together before retaining more than u32::MAX facts.",
        ))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct TypeSubjectId(u32);

impl TypeSubjectId {
    fn from_index(index: usize) -> Self {
        Self(u32::try_from(index).expect(
            "INVARIANT VIOLATED: type subject interner exceeded u32 ids. This is a bug because \
             every stored type fact refers to a compact subject id. Fix: widen TypeSubjectId \
             and every stored subject reference together before interning more than u32::MAX \
             subjects.",
        ))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct RubyTypeId(u32);

impl RubyTypeId {
    fn from_index(index: usize) -> Self {
        Self(u32::try_from(index).expect(
            "INVARIANT VIOLATED: Ruby type interner exceeded u32 ids. This is a bug because \
             every stored type fact refers to a compact Ruby type id. Fix: widen RubyTypeId \
             and every stored Ruby type reference together before interning more than \
             u32::MAX distinct types.",
        ))
    }

    fn index(self) -> usize {
        self.0 as usize
    }
}

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
                    TypeSubject::Expression(_) => panic!(
                        "INVARIANT VIOLATED: an expression subject was inserted into the general type-subject interner. This is a bug because expressions must use their compact file-local range identity. Fix: route every inserted TypeSubject through TypeStore::store_subject."
                    ),
                },
                None => None,
            }
        })
    }

    pub fn known_method_return_types(
        &self,
    ) -> impl Iterator<Item = (&FullyQualifiedName, &RubyType)> {
        self.method_return_types()
            .filter(|(_, ruby_type)| **ruby_type != RubyType::Unknown)
    }

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

    pub fn remove_file(&mut self, file_id: SourceFileId) {
        let Some(stale_ids) = self.facts_by_file.remove(&file_id) else {
            return;
        };
        for stale_id in stale_ids {
            let Some(stale) = self.take_fact(stale_id) else {
                continue;
            };
            self.free_facts.push(stale_id);
            if let Some(subject_id) = stale.subject.interned_id() {
                if let Some(ids) = self.facts_by_subject.get_mut(&subject_id) {
                    ids.retain(|id| *id != stale_id);
                    if ids.is_empty() {
                        self.facts_by_subject.remove(&subject_id);
                    }
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
            let subject = self.store_subject(fact.subject, fact.range);
            if let Some(subject_id) = subject.interned_id() {
                if let Some((_, appended_count)) = touched_subjects
                    .iter_mut()
                    .find(|(touched, _)| *touched == subject_id)
                {
                    *appended_count += 1;
                } else {
                    touched_subjects.push((subject_id, 1));
                }
            }
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
        for (subject, appended_count) in touched_subjects {
            if let Some(ids) = self.facts_by_subject.get_mut(&subject) {
                if self.file_owned_indexes_ordered {
                    place_appended_file_facts(
                        ids,
                        appended_count,
                        file_id,
                        |id| {
                            self.facts[id.index()]
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
            + self.subjects.capacity() * (size_of::<TypeSubject>() + size_of::<usize>() + 1)
            + self
                .subjects
                .iter()
                .map(type_subject_heap_bytes)
                .sum::<usize>()
            + self.ruby_types.capacity() * (size_of::<RubyType>() + size_of::<usize>() + 1)
            + self
                .ruby_types
                .iter()
                .map(ruby_type_heap_bytes)
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

        let mut candidates: Vec<TypeFact> = ids
            .iter()
            .filter_map(|id| self.fact(*id))
            .filter(|fact| self.stored_subject_matches(fact, subject))
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
            let slot = self.facts.get_mut(id.index()).expect(
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
        ids.iter()
            .filter_map(|id| self.fact(*id))
            .filter(|fact| fact.subject.is_expression() && fact.range == range)
            .map(|fact| self.expand_fact(fact))
            .collect()
    }

    fn store_subject(&mut self, subject: TypeSubject, fact_range: TextRange) -> StoredTypeSubject {
        match subject {
            TypeSubject::Expression(range) => {
                assert!(
                    range == fact_range,
                    "INVARIANT VIOLATED: expression subject range differs from its type fact range. This is a bug because compact expression identity reuses the fact's existing range. Fix: construct the expression subject and fact from the same AST location."
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
                let (index, _) = self.subjects.insert_full(subject);
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
        self.subjects.get_index(id.index()).expect(
            "INVARIANT VIOLATED: type fact points to missing subject id. \
             This is a bug because type facts must only store interned subject ids. \
             Fix: intern type subjects before inserting facts.",
        )
    }

    pub(crate) fn intern_ruby_type(&mut self, ruby_type: RubyType) -> RubyTypeId {
        let (index, _) = self.ruby_types.insert_full(ruby_type);
        RubyTypeId::from_index(index)
    }

    pub(crate) fn ruby_type(&self, id: RubyTypeId) -> &RubyType {
        self.ruby_types.get_index(id.index()).expect(
            "INVARIANT VIOLATED: type fact points to missing Ruby type id. This is a bug because \
             stored facts must only reference interned Ruby types. Fix: intern Ruby types before \
             inserting facts and keep the interner append-only while facts exist.",
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

fn sort_type_ids(facts: &[Option<StoredTypeFact>], ids: &mut [TypeFactId]) {
    ids.sort_by_key(|id| {
        let fact = facts[id.index()].as_ref().expect(
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
        let fact = facts[id.index()].as_ref().expect(
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
    fn method_return_type_views_retain_unknown_locally_and_filter_known_returns() {
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
            unknown.clone(),
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

        let all_returns = store.method_return_types().collect::<Vec<_>>();
        let returns = store.known_method_return_types().collect::<Vec<_>>();
        let TypeSubject::MethodReturn(first_fqn) = first else {
            panic!("test method subject must be a method return")
        };
        let TypeSubject::MethodReturn(second_fqn) = second else {
            panic!("test method subject must be a method return")
        };
        let TypeSubject::MethodReturn(unknown_fqn) = unknown else {
            panic!("test unknown method subject must be a method return")
        };
        assert_eq!(
            all_returns,
            vec![
                (&first_fqn, &RubyType::string()),
                (&unknown_fqn, &RubyType::Unknown),
                (&second_fqn, &RubyType::integer()),
            ],
            "the local collector view must retain an Unknown proof kill in arena order"
        );
        assert_eq!(
            returns,
            vec![
                (&first_fqn, &RubyType::string()),
                (&second_fqn, &RubyType::integer()),
            ]
        );
    }

    #[test]
    fn identical_ruby_types_share_one_internal_value() {
        let mut store = TypeStore::new();
        store.add(TypeFact::new(
            constant_subject("FIRST"),
            RubyType::string(),
            TextRange::new(file(), 0, 5),
            TypeProvenance::Assignment,
        ));
        store.add(TypeFact::new(
            constant_subject("SECOND"),
            RubyType::string(),
            TextRange::new(file(), 10, 16),
            TypeProvenance::Assignment,
        ));

        assert_eq!(store.fact_count(), 2);
        assert_eq!(store.ruby_types.len(), 1);
    }

    #[test]
    fn stored_type_fact_retains_the_compact_arena_layout() {
        assert_eq!(
            size_of::<StoredTypeFact>(),
            24,
            "adding retained fields to every type fact requires real-project memory evidence"
        );
    }

    #[test]
    fn expression_facts_use_file_local_range_identity_without_subject_buckets() {
        let old_range = TextRange::new(file(), 0, 5);
        let new_range = TextRange::new(file(), 10, 15);
        let mut store = TypeStore::new();
        store.add(TypeFact::new(
            TypeSubject::Expression(old_range),
            RubyType::string(),
            old_range,
            TypeProvenance::Literal,
        ));

        assert_eq!(store.subjects.len(), 0);
        assert_eq!(store.facts_by_subject.len(), 0);
        assert_eq!(
            store.facts_for(&TypeSubject::Expression(old_range))[0].ruby_type,
            RubyType::string()
        );
        assert!(matches!(
            store.type_at(&TypeSubject::Expression(old_range), file(), 4),
            TypeResolution::Resolved(TypeFact {
                ruby_type: RubyType::Class(_),
                ..
            })
        ));

        store.replace_file(
            file(),
            [TypeFact::new(
                TypeSubject::Expression(new_range),
                RubyType::integer(),
                new_range,
                TypeProvenance::Literal,
            )],
        );

        assert_eq!(store.subjects.len(), 0);
        assert_eq!(store.facts_by_subject.len(), 0);
        assert!(store
            .facts_for(&TypeSubject::Expression(old_range))
            .is_empty());
        assert_eq!(
            store.facts_for(&TypeSubject::Expression(new_range))[0].ruby_type,
            RubyType::integer()
        );
    }

    #[test]
    fn updates_only_matching_inferred_method_returns_in_place() {
        let inferred = method_return_subject("Target", "call");
        let contracted = method_return_subject("Contracted", "call");
        let other_file = method_return_subject("Other", "call");
        let mut store = TypeStore::new();
        store.add(TypeFact::new(
            inferred.clone(),
            RubyType::Unknown,
            TextRange::new(file(), 0, 8),
            TypeProvenance::Inferred,
        ));
        store.add(TypeFact::new(
            contracted.clone(),
            RubyType::string(),
            TextRange::new(file(), 10, 18),
            TypeProvenance::Rbs,
        ));
        store.add(TypeFact::new(
            other_file.clone(),
            RubyType::Unknown,
            TextRange::new(SourceFileId(2), 0, 8),
            TypeProvenance::Inferred,
        ));

        let TypeSubject::MethodReturn(inferred_fqn) = &inferred else {
            panic!("test subject must be a method return")
        };
        let TypeSubject::MethodReturn(contracted_fqn) = &contracted else {
            panic!("test subject must be a method return")
        };
        let TypeSubject::MethodReturn(other_fqn) = &other_file else {
            panic!("test subject must be a method return")
        };
        let updated = store.update_inferred_method_return_types_in_file(
            file(),
            [
                (inferred_fqn, RubyType::integer()),
                (contracted_fqn, RubyType::boolean()),
                (other_fqn, RubyType::boolean()),
            ],
        );

        assert_eq!(updated, 1);
        assert_eq!(store.facts_for(&inferred)[0].ruby_type, RubyType::integer());
        assert_eq!(
            store.facts_for(&contracted)[0].ruby_type,
            RubyType::string()
        );
        assert_eq!(store.facts_for(&other_file)[0].ruby_type, RubyType::Unknown);
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
    fn named_file_query_selects_without_materializing_unrelated_expression_facts() {
        let wanted = constant_subject("WANTED");
        let excluded = TextRange::new(file(), 20, 26);
        let mut store = TypeStore::new();
        store.add(TypeFact::new(
            wanted.clone(),
            RubyType::integer(),
            TextRange::new(file(), 0, 6),
            TypeProvenance::Assignment,
        ));
        for offset in 1..20 {
            let range = TextRange::new(file(), offset, offset + 1);
            store.add(TypeFact::new(
                TypeSubject::Expression(range),
                RubyType::string(),
                range,
                TypeProvenance::Literal,
            ));
        }
        store.add(TypeFact::new(
            wanted.clone(),
            RubyType::string(),
            excluded,
            TypeProvenance::Assignment,
        ));

        let resolution = store.named_type_in_file_before_matching(file(), 30, |subject, range| {
            subject == &wanted && range != excluded
        });
        match resolution {
            NamedTypeResolution::Resolved(ruby_type) => {
                assert_eq!(*ruby_type, RubyType::integer())
            }
            other => panic!("expected the latest non-excluded named fact, got {other:?}"),
        }
    }

    #[test]
    fn named_file_query_preserves_same_position_type_ambiguity() {
        let wanted = constant_subject("WANTED");
        let range = TextRange::new(file(), 10, 16);
        let mut store = TypeStore::new();
        store.add(TypeFact::new(
            wanted.clone(),
            RubyType::integer(),
            range,
            TypeProvenance::Assignment,
        ));
        store.add(TypeFact::new(
            wanted.clone(),
            RubyType::string(),
            range,
            TypeProvenance::Flow,
        ));

        match store.named_type_in_file_before_matching(file(), 20, |subject, _| subject == &wanted)
        {
            NamedTypeResolution::Ambiguous => {}
            other => panic!("expected conflicting latest named facts, got {other:?}"),
        }
    }

    #[test]
    #[should_panic(expected = "INVARIANT VIOLATED: TextRange start_byte must be <= end_byte")]
    fn invalid_range_panics() {
        let _ = TextRange::new(file(), 10, 9);
    }
}
