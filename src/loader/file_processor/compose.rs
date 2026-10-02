//! Composition of one file's final `FileAnalysis` from collector output.
//!
//! Interactive processing and batch collection share this assembly. Their
//! genuine policy differences are explicit inputs: where require diagnostics
//! find their project root and load paths, whether a direct declaration seed
//! replaces collected declarations, and which document extension patches are
//! mapped against.

use super::extension_facts::add_extension_analysis_facts;
use super::merge::{
    merge_collected_type_facts, merge_execution_context_direct_facts, merge_runtime_direct_facts,
};
use super::FileProcessor;
use crate::loader::context::{LoadContext, LoadTarget};
use crate::loader::require_paths::unresolved_require_diagnostics;
use ruby_analysis::core::{FileAnalysis, SourceFileId, SourceKind};
use ruby_analysis::indexer::fact_collector::FactCollectorOutput;
use ruby_analysis::indexer::RubyDocument;
use ruby_fast_lsp_extension_api::ProjectContext;
use std::path::PathBuf;
use tower_lsp::lsp_types::Url;

/// Where unresolved-require diagnostics find their project root and load paths.
pub(super) enum RequireDiagnosticRoots<'a> {
    /// The load context's owning project and its configured load paths,
    /// falling back to the processor's batch root when the file has no project.
    Context(&'a LoadContext),
    /// The processor's own batch resolve context.
    Processor,
}

/// Which document extension patches are mapped against.
pub(super) enum ExtensionDocument<'a> {
    /// The document the collector returned after traversal.
    Collected,
    /// The document the caller handed to the collector.
    Original(&'a RubyDocument),
}

/// Inputs that select one file's composition policy.
pub(super) struct FileComposition<'a> {
    pub uri: &'a Url,
    pub content: &'a str,
    pub file_id: SourceFileId,
    pub source_kind: SourceKind,
    pub analysis_engine: &'a dyn LoadTarget,
    pub extension_project_context: Option<&'a ProjectContext>,
    /// Direct declarations that replace the collector's declarations, if any.
    pub declarations: Option<FileAnalysis>,
    pub extension_document: ExtensionDocument<'a>,
    pub require_roots: RequireDiagnosticRoots<'a>,
}

impl FileProcessor {
    /// Merge declarations, then extension facts, then flow types, and apply
    /// the source-kind policy. Returns the final analysis and the collector's
    /// document.
    pub(super) fn compose_file_analysis(
        &self,
        composition: FileComposition<'_>,
        output: FactCollectorOutput,
    ) -> (FileAnalysis, RubyDocument) {
        let FileComposition {
            uri,
            content,
            file_id,
            source_kind,
            analysis_engine,
            extension_project_context,
            declarations,
            extension_document,
            require_roots,
        } = composition;
        let FactCollectorOutput {
            mut analysis,
            flow_types,
            extension_patches,
            document,
        } = output;
        if !source_kind.contributes_project_diagnostics() {
            analysis.inference.method_return_outcomes.clear();
            analysis.inference.method_return_equations.clear();
        }
        if let Some(declarations) = declarations {
            let collected_declarations = analysis.replace_declarations(declarations);
            merge_execution_context_direct_facts(&collected_declarations, &mut analysis);
            merge_runtime_direct_facts(&collected_declarations, &mut analysis);
        }
        let extension_document = match extension_document {
            ExtensionDocument::Collected => &document,
            ExtensionDocument::Original(original) => original,
        };
        add_extension_analysis_facts(
            analysis_engine,
            extension_document,
            &extension_patches,
            extension_project_context,
            &mut analysis,
        );
        merge_collected_type_facts(flow_types, &mut analysis.types);
        if !source_kind.contributes_references() {
            analysis.reference_candidates = Vec::new();
        }
        if source_kind.contributes_project_diagnostics() {
            self.add_require_diagnostics(
                uri,
                content,
                file_id,
                analysis_engine,
                require_roots,
                &mut analysis,
            );
        } else {
            analysis.diagnostic_candidates = Vec::new();
            analysis.diagnostics = Vec::new();
        }
        (analysis, document)
    }

    fn add_require_diagnostics(
        &self,
        uri: &Url,
        content: &str,
        file_id: SourceFileId,
        analysis_engine: &dyn LoadTarget,
        require_roots: RequireDiagnosticRoots<'_>,
        analysis: &mut FileAnalysis,
    ) {
        let current_path = uri
            .to_file_path()
            .unwrap_or_else(|_| PathBuf::from(uri.to_string()));
        match require_roots {
            RequireDiagnosticRoots::Context(ctx) => {
                let Some(project_root) = ctx
                    .requires
                    .project_root
                    .clone()
                    .or_else(|| self.require_project_root.clone())
                else {
                    return;
                };
                let load_paths = ctx.config.load_paths_for_project(&project_root);
                let feature_index = ctx.requires.feature_index();
                let diagnostics = analysis_engine.view(|view| {
                    unresolved_require_diagnostics(
                        content,
                        file_id,
                        &current_path,
                        &project_root,
                        &load_paths,
                        &feature_index,
                        Some(view),
                    )
                });
                analysis.diagnostics.extend(diagnostics);
            }
            RequireDiagnosticRoots::Processor => {
                let Some(project_root) = self.require_project_root.as_ref() else {
                    return;
                };
                let diagnostics = analysis_engine.view(|view| {
                    unresolved_require_diagnostics(
                        content,
                        file_id,
                        &current_path,
                        project_root,
                        &self.require_load_paths,
                        &self.require_feature_index,
                        Some(view),
                    )
                });
                analysis.diagnostics.extend(diagnostics);
            }
        }
    }
}
