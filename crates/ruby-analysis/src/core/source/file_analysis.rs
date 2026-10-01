use crate::core::{
    DiagnosticCandidate, DiagnosticFact, ExecutionContextFact, GraphEdgeFact, GraphNodeFact,
    InferenceEvidence, MethodFact, MethodVisibilityOverrideFact, ReferenceCandidate, RubyType,
    SymbolFact, TextRange, TypeFact, UnresolvedGraphEdgeFact,
};

/// Complete file-owned analysis output for one source file.
///
/// Replacing a file with a `FileAnalysis` removes every fact previously owned
/// by that file; nothing from an earlier analysis survives the replacement.
#[derive(Debug, Clone, Default)]
pub struct FileAnalysis {
    pub symbols: Vec<SymbolFact>,
    pub methods: Vec<MethodFact>,
    pub method_visibility_overrides: Vec<MethodVisibilityOverrideFact>,
    pub types: Vec<TypeFact>,
    pub graph_nodes: Vec<GraphNodeFact>,
    pub graph_edges: Vec<GraphEdgeFact>,
    pub unresolved_graph_edges: Vec<UnresolvedGraphEdgeFact>,
    pub reference_candidates: Vec<ReferenceCandidate>,
    pub diagnostic_candidates: Vec<DiagnosticCandidate>,
    pub diagnostics: Vec<DiagnosticFact>,
    pub execution_contexts: Vec<ExecutionContextFact>,
    pub inference: InferenceEvidence,
    pub local_read_types: Box<[(TextRange, RubyType)]>,
}
