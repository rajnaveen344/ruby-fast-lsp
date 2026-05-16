use std::path::Path;

use ruby_analysis_core::{
    FullyQualifiedName, GraphEdgeFact, GraphNodeFact, ReferenceFact, SourceFileId, SymbolFact,
    TypeFact, TypeResolution, TypeSubject,
};

use crate::{AnalysisEngine, SourceFile};

pub struct AnalysisQuery<'a> {
    engine: &'a AnalysisEngine,
}

impl<'a> AnalysisQuery<'a> {
    pub fn new(engine: &'a AnalysisEngine) -> Self {
        Self { engine }
    }

    pub fn file_id(&self, path: impl AsRef<Path>) -> Option<SourceFileId> {
        self.engine.file_id(path)
    }

    pub fn file(&self, file_id: SourceFileId) -> Option<&'a SourceFile> {
        self.engine.file(file_id)
    }

    pub fn type_at(
        &self,
        subject: &TypeSubject,
        file_id: SourceFileId,
        byte_offset: u32,
    ) -> TypeResolution {
        self.engine.type_at(subject, file_id, byte_offset)
    }

    pub fn type_facts_in_file(&self, file_id: SourceFileId) -> Vec<TypeFact> {
        self.engine.type_store().facts_in_file(file_id)
    }

    pub fn symbol_facts_in_file(&self, file_id: SourceFileId) -> Vec<SymbolFact> {
        self.engine.symbol_store().facts_in_file(file_id)
    }

    pub fn all_symbol_facts(&self) -> Vec<SymbolFact> {
        self.engine.all_symbol_facts()
    }

    pub fn symbols_for_fqn(&self, fqn: &FullyQualifiedName) -> &'a [SymbolFact] {
        self.engine.symbol_facts_for(fqn)
    }

    pub fn references_for_fqn(&self, fqn: &FullyQualifiedName) -> &'a [ReferenceFact] {
        self.engine.reference_facts_for(fqn)
    }

    pub fn references_in_file(&self, file_id: SourceFileId) -> Vec<ReferenceFact> {
        self.engine.reference_store().facts_in_file(file_id)
    }

    pub fn graph_nodes_for(&self, fqn: &FullyQualifiedName) -> &'a [GraphNodeFact] {
        self.engine.graph_nodes_for(fqn)
    }

    pub fn graph_edges_from(&self, fqn: &FullyQualifiedName) -> &'a [GraphEdgeFact] {
        self.engine.graph_edges_from(fqn)
    }

    pub fn all_graph_edges(&self) -> Vec<GraphEdgeFact> {
        self.engine.all_graph_edges()
    }

    pub fn graph_nodes_in_file(&self, file_id: SourceFileId) -> Vec<GraphNodeFact> {
        self.engine.graph_store().nodes_in_file(file_id)
    }

    pub fn graph_edges_in_file(&self, file_id: SourceFileId) -> Vec<GraphEdgeFact> {
        self.engine.graph_store().edges_in_file(file_id)
    }
}
