use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::core::{
    DiagnosticCandidate, DiagnosticCandidateKind, DiagnosticCandidateStore, DiagnosticFact,
    DiagnosticStore, FullyQualifiedName, GraphEdgeFact, GraphEdgeKind, GraphNodeFact, GraphStore,
    MethodCallSignatureCandidate, MethodFact, MethodParamFact, MethodParamKind, MethodStore,
    RaiseArgCandidate, ReferenceCandidate, ReferenceCandidateKind, ReferenceCandidateStore,
    ReferenceFact, ReferenceStore, RubyConstant, RubyMethod, RubyType, SourceFileId, SourceKind,
    SymbolFact, SymbolStore, TextRange, TypeFact, TypeResolution, TypeStore, TypeSubject,
    UnresolvedGraphEdgeFact,
};

use crate::{AnalysisQuery, FileIdMap};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    pub id: SourceFileId,
    pub path: PathBuf,
    pub source: String,
    pub kind: SourceKind,
}

#[derive(Debug, Clone, Default)]
pub struct FileAnalysisFacts {
    pub symbols: Vec<SymbolFact>,
    pub methods: Vec<MethodFact>,
    pub types: Vec<TypeFact>,
    pub graph_nodes: Vec<GraphNodeFact>,
    pub graph_edges: Vec<GraphEdgeFact>,
    pub unresolved_graph_edges: Vec<UnresolvedGraphEdgeFact>,
    pub reference_candidates: Vec<ReferenceCandidate>,
    pub diagnostic_candidates: Vec<DiagnosticCandidate>,
    pub diagnostics: Vec<DiagnosticFact>,
}

/// Shared analysis state for editor and agent consumers.
#[derive(Debug, Clone, Default)]
pub struct AnalysisEngine {
    file_ids: FileIdMap,
    files: HashMap<SourceFileId, SourceFile>,
    graph_store: GraphStore,
    unresolved_graph_edges: Vec<UnresolvedGraphEdgeFact>,
    method_store: MethodStore,
    reference_candidate_store: ReferenceCandidateStore,
    reference_store: ReferenceStore,
    diagnostic_candidate_store: DiagnosticCandidateStore,
    symbol_store: SymbolStore,
    type_store: TypeStore,
    diagnostic_store: DiagnosticStore,
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

    pub fn add_diagnostic_fact(&mut self, fact: DiagnosticFact) {
        self.assert_known_file_id(
            fact.range.file_id,
            "diagnostic fact references unknown source file id",
        );
        self.diagnostic_store.add(fact);
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
        self.resolve_reference_candidates();
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

    pub fn replace_reference_candidates_for_file(
        &mut self,
        file_id: SourceFileId,
        candidates: impl IntoIterator<Item = ReferenceCandidate>,
    ) {
        self.assert_known_file_id(
            file_id,
            "reference candidate replacement references unknown source file id",
        );
        self.reference_candidate_store
            .replace_file(file_id, candidates);
        self.resolve_reference_candidates();
    }

    pub fn replace_file_reference_analysis(
        &mut self,
        file_id: SourceFileId,
        candidates: impl IntoIterator<Item = ReferenceCandidate>,
        diagnostic_candidates: impl IntoIterator<Item = DiagnosticCandidate>,
        diagnostics: impl IntoIterator<Item = DiagnosticFact>,
    ) {
        self.assert_known_file_id(
            file_id,
            "file reference analysis references unknown source file id",
        );
        self.reference_candidate_store
            .replace_file(file_id, candidates);
        self.diagnostic_candidate_store
            .replace_file(file_id, diagnostic_candidates);
        self.diagnostic_store.replace_file(file_id, diagnostics);
        self.resolve_reference_candidates();
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
        self.resolve_reference_candidates();
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
        self.resolve_reference_candidates();
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
        self.resolve_reference_candidates();
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
        self.resolve_reference_candidates();
    }

    pub fn replace_diagnostic_facts_for_file(
        &mut self,
        file_id: SourceFileId,
        facts: impl IntoIterator<Item = DiagnosticFact>,
    ) {
        self.assert_known_file_id(
            file_id,
            "diagnostic fact replacement references unknown source file id",
        );
        self.diagnostic_store.replace_file(file_id, facts);
    }

    pub fn replace_file_analysis(&mut self, file_id: SourceFileId, facts: FileAnalysisFacts) {
        self.assert_known_file_id(file_id, "file analysis references unknown source file id");
        self.symbol_store.replace_file(file_id, facts.symbols);
        self.method_store.replace_file(file_id, facts.methods);
        self.type_store.replace_file(file_id, facts.types);
        self.graph_store.remove_file(file_id);
        self.unresolved_graph_edges
            .retain(|edge| edge.range.file_id != file_id);

        for node in facts.graph_nodes {
            assert!(
                node.range.file_id == file_id,
                "INVARIANT VIOLATED: file analysis graph node belongs to a different file id. \
                 This is a bug because replace_file_analysis must only receive facts for one file. \
                 Fix: partition collected file facts before ingest."
            );
            self.graph_store.add_node(node);
        }
        for edge in facts.graph_edges {
            assert!(
                edge.range.file_id == file_id,
                "INVARIANT VIOLATED: file analysis graph edge belongs to a different file id. \
                 This is a bug because replace_file_analysis must only receive facts for one file. \
                 Fix: partition collected file facts before ingest."
            );
            self.graph_store.add_edge(edge);
        }
        for edge in facts.unresolved_graph_edges {
            assert!(
                edge.range.file_id == file_id,
                "INVARIANT VIOLATED: file analysis unresolved graph edge belongs to a different file id. \
                 This is a bug because replace_file_analysis must only receive facts for one file. \
                 Fix: partition collected file facts before ingest."
            );
            self.unresolved_graph_edges.push(edge);
        }

        self.reference_candidate_store
            .replace_file(file_id, facts.reference_candidates);
        self.diagnostic_candidate_store
            .replace_file(file_id, facts.diagnostic_candidates);
        self.diagnostic_store
            .replace_file(file_id, facts.diagnostics);
        self.retry_unresolved_graph_edges();
        self.resolve_reference_candidates();
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

    pub fn diagnostic_facts_in_file(&self, file_id: SourceFileId) -> Vec<DiagnosticFact> {
        self.diagnostic_store.facts_in_file(file_id)
    }

    pub fn all_diagnostic_facts(&self) -> Vec<DiagnosticFact> {
        self.diagnostic_store.all_facts()
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

    pub fn diagnostic_store(&self) -> &DiagnosticStore {
        &self.diagnostic_store
    }

    pub fn reference_candidate_store(&self) -> &ReferenceCandidateStore {
        &self.reference_candidate_store
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

    fn resolve_reference_candidates(&mut self) {
        let candidates = self.reference_candidate_store.all_candidates();
        let mut candidate_file_ids = self.reference_candidate_store.file_ids();
        for file_id in self.diagnostic_candidate_store.file_ids() {
            if !candidate_file_ids.contains(&file_id) {
                candidate_file_ids.push(file_id);
            }
        }
        self.reference_store.clear();

        let mut unresolved_constants = self.resolve_diagnostic_candidates();
        for candidate in candidates {
            match candidate.kind {
                ReferenceCandidateKind::Resolved { target, caller } => {
                    self.reference_store
                        .add(ReferenceFact::new(target, candidate.range, caller));
                }
                ReferenceCandidateKind::Constant {
                    parts,
                    current_namespace,
                    name,
                } => {
                    if let Some(target) =
                        self.resolve_constant_reference(&parts, &current_namespace)
                    {
                        self.reference_store
                            .add(ReferenceFact::new(target, candidate.range, None));
                    } else {
                        unresolved_constants
                            .entry(candidate.range.file_id)
                            .or_default()
                            .push(DiagnosticFact::new(
                                candidate.range,
                                crate::core::DiagnosticSeverity::Error,
                                "unresolved-constant",
                                format!("Unresolved constant `{}`", name),
                            ));
                    }
                }
                ReferenceCandidateKind::Method {
                    owner,
                    owner_kind,
                    method,
                    caller,
                    diagnostic_range,
                    receiver_label,
                    diagnose_unresolved,
                    signature,
                } => {
                    let owner_fqn = FullyQualifiedName::namespace_with_kind(owner, owner_kind);
                    let fact =
                        AnalysisQuery::new(self).method_fact_for_receiver(&owner_fqn, &method);
                    if let Some(fact) = fact {
                        let target =
                            FullyQualifiedName::method(fact.owner.namespace_parts(), method);
                        self.reference_store.add(ReferenceFact::new(
                            target,
                            candidate.range,
                            caller,
                        ));
                        self.push_signature_diagnostics(
                            &fact,
                            &method,
                            &signature,
                            diagnostic_range,
                            &mut unresolved_constants,
                        );
                    } else if self.method_namespace_target_exists(&owner_fqn) {
                        let target =
                            FullyQualifiedName::method(owner_fqn.namespace_parts(), method);
                        self.reference_store.add(ReferenceFact::new(
                            target,
                            candidate.range,
                            caller,
                        ));

                        if diagnose_unresolved {
                            let suggestion =
                                self.find_method_suggestion(&owner_fqn, method.as_str());
                            let mut message = match receiver_label {
                                Some(label) => format!(
                                    "Unresolved method `{}` on `{}`",
                                    method.as_str(),
                                    label
                                ),
                                None => format!("Unresolved method `{}`", method.as_str()),
                            };
                            if let Some(suggestion) = suggestion {
                                message.push_str(&format!(". Did you mean `{}`?", suggestion));
                            }
                            unresolved_constants
                                .entry(diagnostic_range.file_id)
                                .or_default()
                                .push(DiagnosticFact::new(
                                    diagnostic_range,
                                    crate::core::DiagnosticSeverity::Warning,
                                    "unresolved-method",
                                    message,
                                ));
                        }
                    }
                }
            }
        }

        for file_id in candidate_file_ids {
            let mut diagnostics = self
                .diagnostic_store
                .facts_in_file(file_id)
                .into_iter()
                .filter(|fact| fact.code != "unresolved-constant")
                .filter(|fact| fact.code != "unresolved-method")
                .filter(|fact| fact.code != "wrong-arity")
                .filter(|fact| fact.code != "unknown-kwarg")
                .filter(|fact| fact.code != "missing-kwarg")
                .filter(|fact| fact.code != "raise-non-exception")
                .filter(|fact| fact.code != "bad-splat")
                .collect::<Vec<_>>();
            diagnostics.extend(unresolved_constants.remove(&file_id).unwrap_or_default());
            self.diagnostic_store.replace_file(file_id, diagnostics);
        }
    }

    fn push_signature_diagnostics(
        &self,
        fact: &MethodFact,
        method: &crate::core::RubyMethod,
        signature: &MethodCallSignatureCandidate,
        diagnostic_range: TextRange,
        diagnostics_by_file: &mut HashMap<SourceFileId, Vec<DiagnosticFact>>,
    ) {
        let arity = MethodArity::from_params(&fact.param_facts);
        if let Some((min, max, actual)) = arity_mismatch(signature, &arity) {
            let expected = match max {
                Some(max) if max == min => format!("{}", min),
                Some(max) => format!("{}..{}", min, max),
                None => format!("{}+", min),
            };
            diagnostics_by_file
                .entry(diagnostic_range.file_id)
                .or_default()
                .push(DiagnosticFact::new(
                    diagnostic_range,
                    crate::core::DiagnosticSeverity::Warning,
                    "wrong-arity",
                    format!(
                        "Wrong number of arguments for `{}` (expected {}, got {})",
                        method.as_str(),
                        expected,
                        actual
                    ),
                ));
        }

        if !arity.has_kwrest && !signature.has_keyword_splat {
            let declared = arity
                .required_keywords
                .iter()
                .chain(arity.optional_keywords.iter())
                .cloned()
                .collect::<Vec<_>>();
            for kwarg in &signature.keyword_args {
                if declared.contains(&kwarg.name) {
                    continue;
                }
                let suggestion = closest_keyword(&kwarg.name, &declared);
                let mut message = format!(
                    "Unknown keyword argument `{}:` for `{}`",
                    kwarg.name,
                    method.as_str()
                );
                if let Some(suggestion) = suggestion {
                    message.push_str(&format!(". Did you mean `{}:`?", suggestion));
                }
                diagnostics_by_file
                    .entry(kwarg.range.file_id)
                    .or_default()
                    .push(DiagnosticFact::new(
                        kwarg.range,
                        crate::core::DiagnosticSeverity::Warning,
                        "unknown-kwarg",
                        message,
                    ));
            }
        }

        if !arity.required_keywords.is_empty() && !signature.has_keyword_splat {
            let supplied = signature
                .keyword_args
                .iter()
                .map(|kwarg| kwarg.name.as_str())
                .collect::<Vec<_>>();
            let mut missing = arity
                .required_keywords
                .iter()
                .filter(|kwarg| !supplied.contains(&kwarg.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            missing.sort();
            if !missing.is_empty() {
                let kw_list = missing
                    .iter()
                    .map(|kwarg| format!("`{}:`", kwarg))
                    .collect::<Vec<_>>()
                    .join(", ");
                diagnostics_by_file
                    .entry(diagnostic_range.file_id)
                    .or_default()
                    .push(DiagnosticFact::new(
                        diagnostic_range,
                        crate::core::DiagnosticSeverity::Warning,
                        "missing-kwarg",
                        format!(
                            "Missing required keyword argument(s) for `{}`: {}",
                            method.as_str(),
                            kw_list
                        ),
                    ));
            }
        }
    }

    fn resolve_diagnostic_candidates(&self) -> HashMap<SourceFileId, Vec<DiagnosticFact>> {
        let mut diagnostics = HashMap::new();
        for candidate in self.diagnostic_candidate_store.all_candidates() {
            match candidate.kind {
                DiagnosticCandidateKind::BadSplat {
                    operator,
                    arg_repr,
                    expected,
                } => {
                    diagnostics
                        .entry(candidate.range.file_id)
                        .or_insert_with(Vec::new)
                        .push(DiagnosticFact::new(
                            candidate.range,
                            crate::core::DiagnosticSeverity::Warning,
                            "bad-splat",
                            format!(
                                "`{}{}` expected {} but got non-{} value",
                                operator, arg_repr, expected, expected
                            ),
                        ));
                }
                DiagnosticCandidateKind::RaiseNonException { arg_repr, arg } => {
                    if self.raise_arg_is_exception(arg) {
                        continue;
                    }
                    diagnostics
                        .entry(candidate.range.file_id)
                        .or_insert_with(Vec::new)
                        .push(DiagnosticFact::new(
                            candidate.range,
                            crate::core::DiagnosticSeverity::Warning,
                            "raise-non-exception",
                            format!(
                                "`raise` argument `{}` is not an Exception subclass",
                                arg_repr
                            ),
                        ));
                }
            }
        }
        diagnostics
    }

    fn raise_arg_is_exception(&self, arg: RaiseArgCandidate) -> bool {
        match arg {
            RaiseArgCandidate::StringLiteral | RaiseArgCandidate::Unknown => true,
            RaiseArgCandidate::NonExceptionLiteral => false,
            RaiseArgCandidate::Constant(name) => self.is_exception_class_name(&name),
            RaiseArgCandidate::Type(ruby_type) => self.ruby_type_is_exception(ruby_type),
            RaiseArgCandidate::BareMethodReturn {
                current_namespace,
                method,
            } => self
                .bare_method_return_type(&current_namespace, &method)
                .map(|ruby_type| self.ruby_type_is_exception(ruby_type))
                .unwrap_or(true),
        }
    }

    fn bare_method_return_type(
        &self,
        current_namespace: &[RubyConstant],
        method: &RubyMethod,
    ) -> Option<RubyType> {
        let query = AnalysisQuery::new(self);
        let mut namespace = current_namespace.to_vec();
        loop {
            let namespace_fqn = FullyQualifiedName::namespace_with_kind(
                namespace.clone(),
                crate::core::NamespaceKind::Instance,
            );
            if let Some(fact) = query.method_fact_for_receiver(&namespace_fqn, method) {
                return query.method_return_type(&fact).or(Some(RubyType::Unknown));
            }
            if namespace.is_empty() {
                break;
            }
            namespace.pop();
        }
        None
    }

    fn ruby_type_is_exception(&self, ruby_type: RubyType) -> bool {
        match ruby_type {
            RubyType::Class(fqn) | RubyType::ClassReference(fqn) => {
                let name = fqn
                    .namespace_parts()
                    .last()
                    .map(|constant| constant.to_string())
                    .unwrap_or_default();
                if name == "String" {
                    return true;
                }
                if NON_EXCEPTION_TYPES.contains(&name.as_str()) {
                    return false;
                }
                self.is_exception_class_name(&name)
            }
            RubyType::Module(_) | RubyType::ModuleReference(_) => false,
            RubyType::Union(_) | RubyType::Unknown => true,
            RubyType::Array(_) | RubyType::Hash(_, _) => false,
        }
    }

    fn is_exception_class_name(&self, name: &str) -> bool {
        if EXCEPTION_WHITELIST.contains(&name) {
            return true;
        }
        if name.ends_with("Error") || name.ends_with("Exception") {
            return true;
        }
        let Ok(ruby_const) = RubyConstant::new(name) else {
            return true;
        };
        let ns_fqn = FullyQualifiedName::namespace_with_kind(
            vec![ruby_const],
            crate::core::NamespaceKind::Instance,
        );
        if self.graph_store.nodes_for(&ns_fqn).is_empty()
            && self.symbol_store.facts_for(&ns_fqn).is_empty()
        {
            return true;
        }

        let mut current = ns_fqn;
        let mut visited = std::collections::HashSet::new();
        while visited.insert(current.clone()) {
            let mut advanced = false;
            for edge in self.graph_store.all_edges() {
                if edge.kind != GraphEdgeKind::Superclass || edge.source != current {
                    continue;
                }
                let last = edge.target.namespace_parts().last().map(|c| c.to_string());
                if let Some(target_name) = last {
                    if EXCEPTION_WHITELIST.contains(&target_name.as_str()) {
                        return true;
                    }
                }
                current = edge.target;
                advanced = true;
                break;
            }
            if !advanced {
                break;
            }
        }

        false
    }

    fn find_method_suggestion(
        &self,
        owner_fqn: &FullyQualifiedName,
        target: &str,
    ) -> Option<String> {
        let threshold = suggestion_threshold(target.len());
        if threshold == 0 {
            return None;
        }

        let target_len = target.len();
        let mut best: Option<(String, usize)> = None;
        for fact in AnalysisQuery::new(self).method_completion_facts(owner_fqn, "") {
            let FullyQualifiedName::Method(_, method) = &fact.fqn else {
                continue;
            };
            let candidate = method.as_str();
            if candidate == target {
                continue;
            }
            if candidate.len().abs_diff(target_len) > threshold {
                continue;
            }
            let dist = levenshtein(candidate, target);
            if dist > threshold {
                continue;
            }
            match &best {
                Some((_, d)) if *d <= dist => {}
                Some(_) | None => best = Some((candidate.to_string(), dist)),
            }
        }
        best.map(|(name, _)| name)
    }

    fn method_namespace_target_exists(&self, fqn: &FullyQualifiedName) -> bool {
        let parts = fqn.namespace_parts();
        if parts.is_empty() {
            return true;
        }
        let instance_fqn = FullyQualifiedName::namespace_with_kind(
            parts.clone(),
            crate::core::NamespaceKind::Instance,
        );
        let singleton_fqn = FullyQualifiedName::namespace_with_kind(
            parts.clone(),
            crate::core::NamespaceKind::Singleton,
        );
        !self.graph_store.nodes_for(&instance_fqn).is_empty()
            || !self.graph_store.nodes_for(&singleton_fqn).is_empty()
            || !self
                .symbol_store
                .facts_for(&FullyQualifiedName::constant(parts))
                .is_empty()
    }

    fn resolve_constant_reference(
        &self,
        parts: &[crate::core::RubyConstant],
        current_namespace: &[crate::core::RubyConstant],
    ) -> Option<FullyQualifiedName> {
        let mut search = current_namespace.to_vec();

        loop {
            let mut probe = search.clone();
            probe.extend(parts.iter().cloned());

            let namespace_fqn = FullyQualifiedName::namespace(probe.clone());
            if !self.graph_store.nodes_for(&namespace_fqn).is_empty()
                || !self.symbol_store.facts_for(&namespace_fqn).is_empty()
            {
                return Some(namespace_fqn);
            }

            let constant_fqn = FullyQualifiedName::constant(probe);
            if !self.symbol_store.facts_for(&constant_fqn).is_empty() {
                return Some(constant_fqn);
            }

            if search.is_empty() {
                break;
            }
            search.pop();
        }

        None
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

const EXCEPTION_WHITELIST: &[&str] = &[
    "Exception",
    "StandardError",
    "RuntimeError",
    "ArgumentError",
    "TypeError",
    "NameError",
    "NoMethodError",
    "IOError",
    "RangeError",
    "NotImplementedError",
    "ZeroDivisionError",
    "IndexError",
    "KeyError",
    "StopIteration",
    "SystemExit",
    "Interrupt",
    "ScriptError",
    "SyntaxError",
    "LoadError",
    "LocalJumpError",
    "FrozenError",
    "EncodingError",
    "RegexpError",
    "SystemCallError",
    "ThreadError",
    "FiberError",
    "SecurityError",
    "SignalException",
];

const NON_EXCEPTION_TYPES: &[&str] = &[
    "Integer",
    "Float",
    "Rational",
    "Complex",
    "Numeric",
    "Array",
    "Hash",
    "Symbol",
    "Regexp",
    "Range",
    "Proc",
    "Method",
    "UnboundMethod",
    "IO",
    "File",
    "Dir",
    "Time",
    "Struct",
    "Encoding",
    "Fiber",
    "Thread",
    "Mutex",
    "Queue",
    "TrueClass",
    "FalseClass",
    "NilClass",
    "Binding",
    "BasicObject",
    "Object",
];

fn suggestion_threshold(name_len: usize) -> usize {
    match name_len {
        0..=2 => 0,
        3..=8 => 2,
        _ => 3,
    }
}

struct MethodArity {
    required: usize,
    optional: usize,
    has_rest: bool,
    required_keywords: Vec<String>,
    optional_keywords: Vec<String>,
    has_kwrest: bool,
}

impl MethodArity {
    fn from_params(params: &[MethodParamFact]) -> Self {
        let mut arity = Self {
            required: 0,
            optional: 0,
            has_rest: false,
            required_keywords: Vec::new(),
            optional_keywords: Vec::new(),
            has_kwrest: false,
        };
        for param in params {
            match param.kind {
                MethodParamKind::Required => arity.required += 1,
                MethodParamKind::Optional => arity.optional += 1,
                MethodParamKind::Rest => arity.has_rest = true,
                MethodParamKind::RequiredKeyword => {
                    arity.required_keywords.push(param.name.clone())
                }
                MethodParamKind::OptionalKeyword => {
                    arity.optional_keywords.push(param.name.clone())
                }
                MethodParamKind::KeywordRest => arity.has_kwrest = true,
                MethodParamKind::Block => {}
            }
        }
        arity
    }
}

fn arity_mismatch(
    signature: &MethodCallSignatureCandidate,
    arity: &MethodArity,
) -> Option<(usize, Option<usize>, usize)> {
    let min = arity.required;
    let max = if arity.has_rest {
        None
    } else {
        Some(arity.required + arity.optional)
    };

    if signature.has_positional_splat {
        let too_many = max
            .map(|max| signature.positional_count > max)
            .unwrap_or(false);
        if too_many {
            return Some((min, max, signature.positional_count));
        }
        return None;
    }

    let too_few = signature.positional_count < min;
    let too_many = max
        .map(|max| signature.positional_count > max)
        .unwrap_or(false);
    if too_few || too_many {
        Some((min, max, signature.positional_count))
    } else {
        None
    }
}

fn closest_keyword(target: &str, declared: &[String]) -> Option<String> {
    let threshold = suggestion_threshold(target.len());
    if threshold == 0 {
        return None;
    }
    let mut best: Option<(String, usize)> = None;
    for candidate in declared {
        let dist = levenshtein(candidate, target);
        if dist > threshold {
            continue;
        }
        match &best {
            Some((_, current_dist)) if *current_dist <= dist => {}
            Some(_) | None => best = Some((candidate.clone(), dist)),
        }
    }
    best.map(|(name, _)| name)
}

fn levenshtein(a: &str, b: &str) -> usize {
    if a == b {
        return 0;
    }
    if a.is_empty() {
        return b.chars().count();
    }
    if b.is_empty() {
        return a.chars().count();
    }

    let b_chars = b.chars().collect::<Vec<_>>();
    let mut previous = (0..=b_chars.len()).collect::<Vec<_>>();
    let mut current = vec![0; b_chars.len() + 1];

    for (i, ca) in a.chars().enumerate() {
        current[0] = i + 1;
        for (j, cb) in b_chars.iter().enumerate() {
            let cost = usize::from(ca != *cb);
            current[j + 1] = (previous[j + 1] + 1)
                .min(current[j] + 1)
                .min(previous[j] + cost);
        }
        std::mem::swap(&mut previous, &mut current);
    }

    previous[b_chars.len()]
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;
