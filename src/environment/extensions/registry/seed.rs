//! Semantic seed facts that loaded extensions contribute to a project engine.
//!
//! The registry only produces the seed. The loader registers its synthetic
//! source and commits the facts through the engine lifecycle.
use std::path::PathBuf;

use ruby_analysis::core::{
    FileAnalysis, FullyQualifiedName, GraphNodeFact, GraphNodeKind, MethodFact, NamespaceKind,
    SourceFileId, SourceKind, SymbolFact, SymbolKind as AnalysisSymbolKind, TextRange,
};
use ruby_analysis::engine::SourceFileInput;

use crate::environment::extensions::registry::loaded::LoadedWasmExtension;
use crate::invariant::ExpectInvariant;

const SEMANTIC_SEED_PATH: &str = "/__ruby_fast_lsp_extension__/semantic_targets.rb";

/// Namespaces and method targets of every extension that applies to one
/// project, addressed to one synthetic stub source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ExtensionSemanticSeed {
    namespaces: Vec<(FullyQualifiedName, GraphNodeKind)>,
    methods: Vec<(FullyQualifiedName, FullyQualifiedName)>,
}

impl ExtensionSemanticSeed {
    pub(super) fn from_extensions<'a>(
        extensions: impl IntoIterator<Item = &'a LoadedWasmExtension>,
    ) -> Self {
        let mut namespaces = Vec::new();
        let mut methods = Vec::new();
        for extension in extensions {
            for namespace in &extension.semantic_namespaces {
                for kind in [NamespaceKind::Instance, NamespaceKind::Singleton] {
                    let owner =
                        FullyQualifiedName::namespace_with_kind(namespace.owner.clone(), kind);
                    let entry = (owner, namespace.declaration_kind);
                    if !namespaces.contains(&entry) {
                        namespaces.push(entry);
                    }
                }
            }
            for target in &extension.semantic_targets {
                let owner = FullyQualifiedName::namespace_with_kind(
                    target.owner.clone(),
                    target.owner_kind,
                );
                let fqn = FullyQualifiedName::method(target.owner.clone(), target.method);
                methods.push((fqn, owner));
            }
        }
        Self {
            namespaces,
            methods,
        }
    }

    /// The synthetic stub source that owns the seed facts. Registering it
    /// again returns the same file, so a new seed replaces the previous one.
    pub(crate) fn source(&self) -> SourceFileInput {
        SourceFileInput {
            path: semantic_seed_path(),
            content: String::new(),
            kind: SourceKind::Stub,
        }
    }

    /// The seed facts addressed to the registered seed source `file_id`.
    pub(crate) fn analysis(&self, file_id: SourceFileId) -> FileAnalysis {
        let range = TextRange::new(file_id, 0, 0);
        let mut facts = FileAnalysis::default();
        for (owner, kind) in &self.namespaces {
            facts
                .graph_nodes
                .push(GraphNodeFact::new(owner.clone(), *kind, range));
        }
        for (fqn, owner) in &self.methods {
            facts.symbols.push(SymbolFact::new(
                fqn.clone(),
                AnalysisSymbolKind::Method,
                range,
            ));
            facts
                .methods
                .push(MethodFact::new(fqn.clone(), owner.clone(), range));
        }
        facts
    }
}

fn semantic_seed_path() -> PathBuf {
    std::path::absolute(SEMANTIC_SEED_PATH).expect_invariant(
        "extension semantic source has no absolute native path",
        "registered definitions need a valid file URI for navigation",
        "retain the filesystem root when constructing the synthetic source path",
    )
}
