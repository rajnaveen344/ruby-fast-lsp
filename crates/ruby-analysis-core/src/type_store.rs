use std::collections::HashMap;

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

/// Deterministic type query result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeResolution {
    Resolved(TypeFact),
    Ambiguous(Vec<TypeFact>),
    Unresolved,
}

/// Append-only type fact store.
#[derive(Debug, Clone, Default)]
pub struct TypeStore {
    facts: HashMap<TypeSubject, Vec<TypeFact>>,
}

impl TypeStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, fact: TypeFact) {
        let facts = self.facts.entry(fact.subject.clone()).or_default();
        facts.push(fact);
        facts.sort_by_key(|fact| {
            (
                fact.range.file_id,
                fact.range.start_byte,
                fact.range.end_byte,
                provenance_rank(fact.provenance),
            )
        });
    }

    pub fn facts_for(&self, subject: &TypeSubject) -> &[TypeFact] {
        self.facts.get(subject).map(Vec::as_slice).unwrap_or(&[])
    }

    pub fn all_facts(&self) -> Vec<TypeFact> {
        self.facts
            .values()
            .flat_map(|facts| facts.iter().cloned())
            .collect()
    }

    pub fn facts_in_file(&self, file_id: SourceFileId) -> Vec<TypeFact> {
        self.facts
            .values()
            .flat_map(|facts| facts.iter())
            .filter(|fact| fact.range.file_id == file_id)
            .cloned()
            .collect()
    }

    pub fn remove_file(&mut self, file_id: SourceFileId) {
        self.facts.retain(|_, facts| {
            facts.retain(|fact| fact.range.file_id != file_id);
            !facts.is_empty()
        });
    }

    pub fn replace_file(
        &mut self,
        file_id: SourceFileId,
        facts: impl IntoIterator<Item = TypeFact>,
    ) {
        self.remove_file(file_id);
        for fact in facts {
            assert!(
                fact.range.file_id == file_id,
                "INVARIANT VIOLATED: replacement fact belongs to a different file id. \
                 This is a bug because TypeStore::replace_file must only receive facts for the target file. \
                 Fix: partition facts by SourceFileId before replacing."
            );
            self.add(fact);
        }
    }

    pub fn type_at(
        &self,
        subject: &TypeSubject,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> TypeResolution {
        let Some(facts) = self.facts.get(subject) else {
            return TypeResolution::Unresolved;
        };

        let Some(latest_start) = facts
            .iter()
            .filter(|fact| fact.range.starts_before_or_at(file_id, byte_offset))
            .map(|fact| fact.range.start_byte)
            .max()
        else {
            return TypeResolution::Unresolved;
        };

        let mut candidates: Vec<TypeFact> = facts
            .iter()
            .filter(|fact| fact.range.file_id == file_id && fact.range.start_byte == latest_start)
            .cloned()
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
}

fn provenance_rank(provenance: TypeProvenance) -> u8 {
    match provenance {
        TypeProvenance::Literal => 0,
        TypeProvenance::Assignment => 1,
        TypeProvenance::Flow => 2,
        TypeProvenance::Rbs => 3,
        TypeProvenance::Yard => 4,
        TypeProvenance::Extension => 5,
        TypeProvenance::Inferred => 6,
    }
}

#[cfg(test)]
mod tests {
    use crate::{FullyQualifiedName, RubyConstant};

    use super::*;

    fn file() -> SourceFileId {
        SourceFileId(1)
    }

    fn constant_subject(name: &str) -> TypeSubject {
        TypeSubject::Constant(FullyQualifiedName::Constant(vec![
            RubyConstant::new(name).unwrap()
        ]))
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
