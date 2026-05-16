use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ruby_analysis_core::{
    FullyQualifiedName, GraphEdgeFact, GraphNodeFact, GraphStore, MethodFact, MethodStore,
    ReferenceFact, ReferenceStore, SourceFileId, SourceKind, SymbolFact, SymbolStore, TextRange,
    TypeFact, TypeResolution, TypeStore, TypeSubject, UnresolvedGraphEdgeFact,
};

use crate::FileIdMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub id: SourceFileId,
    pub path: PathBuf,
    pub source: String,
    pub kind: SourceKind,
}

/// Shared analysis state for editor and agent consumers.
#[derive(Debug, Clone, Default)]
pub struct AnalysisEngine {
    file_ids: FileIdMap,
    files: HashMap<SourceFileId, SourceFile>,
    graph_store: GraphStore,
    unresolved_graph_edges: Vec<UnresolvedGraphEdgeFact>,
    method_store: MethodStore,
    reference_store: ReferenceStore,
    symbol_store: SymbolStore,
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
        self.open_or_update_file_with_kind(path, source, SourceKind::Project)
    }

    pub fn open_or_update_file_with_kind(
        &mut self,
        path: impl AsRef<Path>,
        source: impl Into<String>,
        kind: SourceKind,
    ) -> SourceFileId {
        let path = path.as_ref();
        let id = self.file_ids.get_or_insert(path);
        self.files.insert(
            id,
            SourceFile {
                id,
                path: path.components().collect(),
                source: source.into(),
                kind,
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

    pub fn add_symbol_fact(&mut self, fact: SymbolFact) {
        self.assert_known_file_id(
            fact.range.file_id,
            "symbol fact references unknown source file id",
        );
        self.symbol_store.add(fact);
    }

    pub fn add_reference_fact(&mut self, fact: ReferenceFact) {
        self.assert_known_file_id(
            fact.range.file_id,
            "reference fact references unknown source file id",
        );
        self.reference_store.add(fact);
    }

    pub fn add_method_fact(&mut self, fact: MethodFact) {
        self.assert_known_file_id(
            fact.range.file_id,
            "method fact references unknown source file id",
        );
        self.method_store.add(fact);
    }

    pub fn add_graph_node_fact(&mut self, fact: GraphNodeFact) {
        self.assert_known_file_id(
            fact.range.file_id,
            "graph node fact references unknown source file id",
        );
        self.graph_store.add_node(fact);
    }

    pub fn add_graph_edge_fact(&mut self, fact: GraphEdgeFact) {
        self.assert_known_file_id(
            fact.range.file_id,
            "graph edge fact references unknown source file id",
        );
        self.graph_store.add_edge(fact);
    }

    pub fn replace_symbol_facts_for_file(
        &mut self,
        file_id: SourceFileId,
        facts: impl IntoIterator<Item = SymbolFact>,
    ) {
        self.assert_known_file_id(
            file_id,
            "symbol fact replacement references unknown source file id",
        );
        self.symbol_store.replace_file(file_id, facts);
    }

    pub fn replace_reference_facts_for_file(
        &mut self,
        file_id: SourceFileId,
        facts: impl IntoIterator<Item = ReferenceFact>,
    ) {
        self.assert_known_file_id(
            file_id,
            "reference fact replacement references unknown source file id",
        );
        self.reference_store.replace_file(file_id, facts);
    }

    pub fn replace_method_facts_for_file(
        &mut self,
        file_id: SourceFileId,
        facts: impl IntoIterator<Item = MethodFact>,
    ) {
        self.assert_known_file_id(
            file_id,
            "method fact replacement references unknown source file id",
        );
        self.method_store.replace_file(file_id, facts);
    }

    pub fn replace_graph_facts_for_file(
        &mut self,
        file_id: SourceFileId,
        nodes: impl IntoIterator<Item = GraphNodeFact>,
        edges: impl IntoIterator<Item = GraphEdgeFact>,
    ) {
        self.assert_known_file_id(
            file_id,
            "graph fact replacement references unknown source file id",
        );
        self.graph_store.replace_file(file_id, nodes, edges);
    }

    pub fn replace_graph_update_for_file(
        &mut self,
        file_id: SourceFileId,
        nodes: impl IntoIterator<Item = GraphNodeFact>,
        edges: impl IntoIterator<Item = GraphEdgeFact>,
        unresolved_edges: impl IntoIterator<Item = UnresolvedGraphEdgeFact>,
    ) {
        self.assert_known_file_id(
            file_id,
            "graph update replacement references unknown source file id",
        );
        self.graph_store.remove_file(file_id);
        self.unresolved_graph_edges
            .retain(|edge| edge.range.file_id != file_id);

        for node in nodes {
            assert!(
                node.range.file_id == file_id,
                "INVARIANT VIOLATED: replacement graph node belongs to a different file id. \
                 This is a bug because graph updates must only receive facts for the target file. \
                 Fix: partition graph facts by SourceFileId before replacing."
            );
            self.graph_store.add_node(node);
        }
        for edge in edges {
            assert!(
                edge.range.file_id == file_id,
                "INVARIANT VIOLATED: replacement graph edge belongs to a different file id. \
                 This is a bug because graph updates must only receive facts for the target file. \
                 Fix: partition graph facts by SourceFileId before replacing."
            );
            self.graph_store.add_edge(edge);
        }
        for edge in unresolved_edges {
            assert!(
                edge.range.file_id == file_id,
                "INVARIANT VIOLATED: unresolved graph edge belongs to a different file id. \
                 This is a bug because graph updates must only receive facts for the target file. \
                 Fix: partition graph facts by SourceFileId before replacing."
            );
            self.unresolved_graph_edges.push(edge);
        }
        self.retry_unresolved_graph_edges();
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

    pub fn symbol_facts_for(&self, fqn: &FullyQualifiedName) -> &[SymbolFact] {
        self.symbol_store.facts_for(fqn)
    }

    pub fn all_symbol_facts(&self) -> Vec<SymbolFact> {
        self.symbol_store.all_facts()
    }

    pub fn reference_facts_for(&self, target: &FullyQualifiedName) -> &[ReferenceFact] {
        self.reference_store.facts_for(target)
    }

    pub fn method_facts_for(&self, fqn: &FullyQualifiedName) -> &[MethodFact] {
        self.method_store.facts_for(fqn)
    }

    pub fn all_method_facts(&self) -> Vec<MethodFact> {
        self.method_store.all_facts()
    }

    pub fn graph_nodes_for(&self, fqn: &FullyQualifiedName) -> &[GraphNodeFact] {
        self.graph_store.nodes_for(fqn)
    }

    pub fn graph_edges_from(&self, source: &FullyQualifiedName) -> &[GraphEdgeFact] {
        self.graph_store.edges_from(source)
    }

    pub fn all_graph_edges(&self) -> Vec<GraphEdgeFact> {
        self.graph_store.all_edges()
    }

    pub fn graph_store(&self) -> &GraphStore {
        &self.graph_store
    }

    pub fn unresolved_graph_edges(&self) -> &[UnresolvedGraphEdgeFact] {
        &self.unresolved_graph_edges
    }

    pub fn reference_store(&self) -> &ReferenceStore {
        &self.reference_store
    }

    pub fn method_store(&self) -> &MethodStore {
        &self.method_store
    }

    pub fn symbol_store(&self) -> &SymbolStore {
        &self.symbol_store
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

    fn retry_unresolved_graph_edges(&mut self) {
        if self.unresolved_graph_edges.is_empty() {
            return;
        }

        let pending = std::mem::take(&mut self.unresolved_graph_edges);
        for unresolved in pending {
            if let Some(target) = self.resolve_unresolved_graph_target(&unresolved) {
                self.graph_store.add_edge(GraphEdgeFact::new(
                    unresolved.source,
                    target,
                    unresolved.kind,
                    unresolved.range,
                ));
            } else {
                self.unresolved_graph_edges.push(unresolved);
            }
        }
    }

    fn resolve_unresolved_graph_target(
        &self,
        unresolved: &UnresolvedGraphEdgeFact,
    ) -> Option<FullyQualifiedName> {
        let mut search_namespaces = if unresolved.absolute {
            Vec::new()
        } else {
            unresolved.context.namespace_parts()
        };

        loop {
            let mut probe = search_namespaces.clone();
            probe.extend(unresolved.target_parts.iter().cloned());
            let namespace_fqn = FullyQualifiedName::namespace(probe);
            if !self.graph_store.nodes_for(&namespace_fqn).is_empty() {
                return Some(namespace_fqn);
            }

            if unresolved.absolute || search_namespaces.is_empty() {
                break;
            }
            search_namespaces.pop();
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use ruby_analysis_core::{
        FullyQualifiedName, GraphEdgeKind, GraphNodeFact, GraphNodeKind, RubyConstant, RubyType,
        SymbolFact, SymbolKind, TypeProvenance, TypeSubject, UnresolvedGraphEdgeFact,
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
    fn source_kind_updates_with_file() {
        let mut engine = AnalysisEngine::new();

        let file_id =
            engine.open_or_update_file_with_kind("gems/foo.rb", "module Foo; end", SourceKind::Gem);

        assert_eq!(engine.file(file_id).unwrap().kind, SourceKind::Gem);
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
    fn replace_symbol_facts_for_file_removes_stale_engine_facts() {
        let mut engine = AnalysisEngine::new();
        let file_id = engine.open_or_update_file("app/user.rb", "class User; end");
        let fqn = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);

        engine.add_symbol_fact(SymbolFact::new(
            fqn.clone(),
            SymbolKind::Class,
            engine.text_range(file_id, 0, 10),
        ));
        engine.replace_symbol_facts_for_file(
            file_id,
            [SymbolFact::new(
                fqn.clone(),
                SymbolKind::Class,
                engine.text_range(file_id, 20, 30),
            )],
        );

        let facts = engine.symbol_facts_for(&fqn);
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].range.start_byte, 20);
    }

    #[test]
    fn graph_update_retries_unresolved_edges_when_target_arrives() {
        let mut engine = AnalysisEngine::new();
        let user_file = engine.open_or_update_file("user.rb", "class User; include Auth; end");
        let auth_file = engine.open_or_update_file("auth.rb", "module Auth; end");

        let user = FullyQualifiedName::namespace(vec![RubyConstant::new("User").unwrap()]);
        let auth = FullyQualifiedName::namespace(vec![RubyConstant::new("Auth").unwrap()]);
        engine.replace_graph_update_for_file(
            user_file,
            [GraphNodeFact::new(
                user.clone(),
                GraphNodeKind::Class,
                TextRange::new(user_file, 0, 10),
            )],
            [],
            [UnresolvedGraphEdgeFact::new(
                user.clone(),
                vec![RubyConstant::new("Auth").unwrap()],
                false,
                user.clone(),
                GraphEdgeKind::Include,
                TextRange::new(user_file, 12, 24),
            )],
        );
        assert_eq!(engine.unresolved_graph_edges().len(), 1);

        engine.replace_graph_update_for_file(
            auth_file,
            [GraphNodeFact::new(
                auth.clone(),
                GraphNodeKind::Module,
                TextRange::new(auth_file, 0, 11),
            )],
            [],
            [],
        );

        assert!(engine.unresolved_graph_edges().is_empty());
        assert!(engine
            .graph_edges_from(&user)
            .iter()
            .any(|edge| edge.target == auth && edge.kind == GraphEdgeKind::Include));
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
