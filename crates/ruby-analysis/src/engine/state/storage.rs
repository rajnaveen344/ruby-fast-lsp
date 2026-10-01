//! The fact arena of type facts, their reads, and storage compaction.

use crate::core::storage::type_store::TypeStore;
use crate::core::{SourceFileId, TypeFact, TypeResolution, TypeSubject};

use super::AnalysisEngine;

#[derive(Debug, Clone, Default)]
pub(in crate::engine) struct FactArena {
    pub(in crate::engine) types: TypeStore,
}

impl AnalysisEngine {
    pub fn type_at(
        &self,
        subject: &TypeSubject,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> TypeResolution {
        self.facts.types.type_at(subject, file_id, byte_offset)
    }

    pub fn type_facts_for(&self, subject: &TypeSubject) -> Vec<TypeFact> {
        self.facts.types.facts_for(subject)
    }

    pub(crate) fn type_store(&self) -> &TypeStore {
        &self.facts.types
    }
}

impl AnalysisEngine {
    pub fn shrink_to_fit(&mut self) {
        self.files.shrink_to_fit();
        self.names.shrink_to_fit();

        self.decls.shrink_to_fit();
        self.facts.types.shrink_to_fit();
        self.hierarchy.shrink_to_fit();
        self.uses.shrink_to_fit();
        self.diagnostics.shrink_to_fit();
        self.inference_by_file.shrink_to_fit();
        self.call_expression_outcomes_by_file.shrink_to_fit();
        self.local_read_types_by_file.shrink_to_fit();
    }
}
