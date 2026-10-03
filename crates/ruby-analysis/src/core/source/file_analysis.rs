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

impl FileAnalysis {
    /// Swap in the declaration facts of `seed` and return the displaced ones.
    ///
    /// Declarations are symbols, methods, visibility overrides, declaration
    /// types, and graph nodes and edges. Every other field of `self` is kept,
    /// and the returned value holds only the displaced declarations. The seed
    /// must hold declarations only, as the declaration indexers produce.
    pub fn replace_declarations(&mut self, seed: FileAnalysis) -> FileAnalysis {
        let FileAnalysis {
            symbols,
            methods,
            method_visibility_overrides,
            types,
            graph_nodes,
            graph_edges,
            unresolved_graph_edges,
            reference_candidates,
            diagnostic_candidates,
            diagnostics,
            execution_contexts,
            inference,
            local_read_types,
        } = seed;
        invariant!(
            reference_candidates.is_empty()
                && diagnostic_candidates.is_empty()
                && diagnostics.is_empty()
                && execution_contexts.is_empty()
                && inference == InferenceEvidence::default()
                && local_read_types.is_empty(),
            what = "a declaration seed carried non-declaration facts",
            why = "replace_declarations swaps declarations only and would drop the rest",
            fix = "build the seed with a declaration indexer or merge the other facts separately",
        );
        FileAnalysis {
            symbols: std::mem::replace(&mut self.symbols, symbols),
            methods: std::mem::replace(&mut self.methods, methods),
            method_visibility_overrides: std::mem::replace(
                &mut self.method_visibility_overrides,
                method_visibility_overrides,
            ),
            types: std::mem::replace(&mut self.types, types),
            graph_nodes: std::mem::replace(&mut self.graph_nodes, graph_nodes),
            graph_edges: std::mem::replace(&mut self.graph_edges, graph_edges),
            unresolved_graph_edges: std::mem::replace(
                &mut self.unresolved_graph_edges,
                unresolved_graph_edges,
            ),
            ..FileAnalysis::default()
        }
    }
}
