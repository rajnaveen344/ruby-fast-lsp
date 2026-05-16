use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ruby_analysis_core::{
    SourceFileId, TextRange, TypeFact, TypeResolution, TypeStore, TypeSubject,
};

use crate::FileIdMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub id: SourceFileId,
    pub path: PathBuf,
    pub source: String,
}

/// Shared analysis state for editor and agent consumers.
#[derive(Debug, Clone, Default)]
pub struct AnalysisEngine {
    file_ids: FileIdMap,
    files: HashMap<SourceFileId, SourceFile>,
    type_store: TypeStore,
}

impl AnalysisEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open_or_update_file(
        &mut self,
        path: impl AsRef<Path>,
        source: impl Into<String>,
    ) -> SourceFileId {
        let path = path.as_ref();
        let id = self.file_ids.get_or_insert(path);
        self.files.insert(
            id,
            SourceFile {
                id,
                path: path.components().collect(),
                source: source.into(),
            },
        );
        id
    }

    pub fn file_id(&self, path: impl AsRef<Path>) -> Option<SourceFileId> {
        self.file_ids.get(path)
    }

    pub fn file(&self, id: SourceFileId) -> Option<&SourceFile> {
        self.files.get(&id)
    }

    pub fn add_type_fact(&mut self, fact: TypeFact) {
        self.assert_known_file_id(
            fact.range.file_id,
            "type fact references unknown source file id",
        );
        self.type_store.add(fact);
    }

    pub fn replace_type_facts_for_file(
        &mut self,
        file_id: SourceFileId,
        facts: impl IntoIterator<Item = TypeFact>,
    ) {
        self.assert_known_file_id(
            file_id,
            "type fact replacement references unknown source file id",
        );
        self.type_store.replace_file(file_id, facts);
    }

    pub fn type_at(
        &self,
        subject: &TypeSubject,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> TypeResolution {
        self.type_store.type_at(subject, file_id, byte_offset)
    }

    pub fn type_facts_for(&self, subject: &TypeSubject) -> &[TypeFact] {
        self.type_store.facts_for(subject)
    }

    pub fn type_store(&self) -> &TypeStore {
        &self.type_store
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn text_range(&self, file_id: SourceFileId, start_byte: u32, end_byte: u32) -> TextRange {
        self.assert_known_file_id(file_id, "TextRange requested for unknown source file id");
        TextRange::new(file_id, start_byte, end_byte)
    }

    fn assert_known_file_id(&self, file_id: SourceFileId, message: &str) {
        assert!(
            self.files.contains_key(&file_id),
            "INVARIANT VIOLATED: {message}. \
             This is a bug because analysis facts and ranges must only reference registered files. \
             Fix: call AnalysisEngine::open_or_update_file before adding file facts."
        );
    }
}

#[cfg(test)]
mod tests {
    use ruby_analysis_core::{
        FullyQualifiedName, RubyConstant, RubyType, TypeProvenance, TypeSubject,
    };

    use super::*;

    fn constant_subject(name: &str) -> TypeSubject {
        TypeSubject::Constant(FullyQualifiedName::Constant(vec![
            RubyConstant::new(name).unwrap()
        ]))
    }

    #[test]
    fn file_ids_are_stable_across_updates() {
        let mut engine = AnalysisEngine::new();

        let first = engine.open_or_update_file("app/user.rb", "A = 1");
        let second = engine.open_or_update_file("app/user.rb", "A = 2");

        assert_eq!(first, second);
        assert_eq!(engine.file_count(), 1);
        assert_eq!(engine.file(first).unwrap().source, "A = 2");
    }

    #[test]
    fn type_at_reads_engine_owned_store() {
        let mut engine = AnalysisEngine::new();
        let file_id = engine.open_or_update_file("app/user.rb", "A = 1");
        let subject = constant_subject("A");

        engine.add_type_fact(TypeFact::new(
            subject.clone(),
            RubyType::integer(),
            engine.text_range(file_id, 0, 5),
            TypeProvenance::Assignment,
        ));

        match engine.type_at(&subject, file_id, 4) {
            TypeResolution::Resolved(fact) => assert_eq!(fact.ruby_type, RubyType::integer()),
            other => panic!("expected resolved type fact, got {other:?}"),
        }
    }

    #[test]
    fn replace_type_facts_for_file_removes_stale_engine_facts() {
        let mut engine = AnalysisEngine::new();
        let file_id = engine.open_or_update_file("app/user.rb", "A = 1");
        let subject = constant_subject("A");

        engine.add_type_fact(TypeFact::new(
            subject.clone(),
            RubyType::integer(),
            engine.text_range(file_id, 0, 5),
            TypeProvenance::Assignment,
        ));
        engine.replace_type_facts_for_file(
            file_id,
            [TypeFact::new(
                subject.clone(),
                RubyType::string(),
                engine.text_range(file_id, 10, 15),
                TypeProvenance::Assignment,
            )],
        );

        assert_eq!(
            engine.type_at(&subject, file_id, 4),
            TypeResolution::Unresolved
        );
        match engine.type_at(&subject, file_id, 12) {
            TypeResolution::Resolved(fact) => assert_eq!(fact.ruby_type, RubyType::string()),
            other => panic!("expected replacement fact, got {other:?}"),
        }
    }

    #[test]
    #[should_panic(expected = "type fact references unknown source file id")]
    fn rejects_type_fact_for_unknown_file() {
        let mut engine = AnalysisEngine::new();
        let subject = constant_subject("A");

        engine.add_type_fact(TypeFact::new(
            subject,
            RubyType::integer(),
            TextRange::new(SourceFileId(99), 0, 5),
            TypeProvenance::Assignment,
        ));
    }
}
