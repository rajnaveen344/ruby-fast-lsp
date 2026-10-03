//! Exhaustive project semantic context and open-document resolution.

use super::IndexerProject;
use crate::invariant::ExpectInvariant;
use crate::loader::context::{LoadContext, LoadTarget};
use anyhow::{anyhow, Context, Result};
use log::info;
use rayon::prelude::*;
use ruby_analysis::core::FileAnalysis;
use ruby_analysis::core::SourceKind;
use ruby_analysis::engine::ResolveMode;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tower_lsp::lsp_types::Url;

const MAX_EXHAUSTIVE_SEMANTIC_CONTEXT_BYTES: usize = 128 * 1024 * 1024;

impl IndexerProject {
    pub(crate) fn refresh_exhaustive_semantic_context(&mut self) -> Result<()> {
        invariant!(
            self.pending_project_navigation_files.is_none()
                && self.pending_project_files.is_some()
                && !self.exhaustive_collection_started,
            what = "project collection baseline validated outside the idle post-frontier state",
            why = "every project file reads one immutable generation engine",
            fix = "initialize the baseline first; validate it after the frontier, before the first tail batch",
        );
        let snapshot = self.exhaustive_semantic_context.as_ref().expect_invariant(
            "project completion lost the pre-collection semantic baseline",
            "rebuilding it later makes results depend on navigation demand",
            "initialize one baseline before collecting any project file",
        );
        let known_namespaces = self.exhaustive_known_namespaces.as_ref().expect_invariant(
            "project completion lost the pre-collection namespace baseline",
            "rebuilding it later makes results depend on navigation demand",
            "initialize one baseline before collecting any project file",
        );
        let estimated_bytes = snapshot.read().view().estimated_memory_stats().total();
        invariant!(
            estimated_bytes <= MAX_EXHAUSTIVE_SEMANTIC_CONTEXT_BYTES,
            what = "collection baseline for {} needs about {} bytes, over the {}-byte clone budget",
            why = "parallel collection must not clone an unbounded engine",
            fix = "shrink the seed or replace the clone with a compact query projection",
            self.workspace_root.display(),
            estimated_bytes,
            MAX_EXHAUSTIVE_SEMANTIC_CONTEXT_BYTES,
        );
        info!(
            "Retained immutable project collection baseline for {}: estimated_bytes={}, namespaces={}",
            self.workspace_root.display(),
            estimated_bytes,
            known_namespaces.len()
        );
        Ok(())
    }

    pub(super) fn initialize_project_collection_semantic_context(
        &mut self,
        ctx: &LoadContext,
        project_files: &[PathBuf],
    ) -> Result<()> {
        invariant!(
            self.exhaustive_known_namespaces.is_none()
                && self.exhaustive_semantic_context.is_none(),
            what = "one project generation initialized its semantic collection baseline twice",
            why = "every Ruby file must read exactly one immutable pre-collection universe",
            fix = "clear the prior generation before starting project collection",
        );
        let project_uri = Url::from_directory_path(&self.workspace_root).map_err(|_| {
            anyhow::anyhow!(
                "Project root is not a valid file URI: {}",
                self.workspace_root.display()
            )
        })?;
        let analysis_engine = ctx.sink.target_for_uri(&project_uri);
        if let Some(path) = project_files.first() {
            let uri = Url::from_file_path(path).map_err(|_| {
                anyhow!(
                    "project source path is not a valid file URI: {}",
                    path.display()
                )
            })?;
            self.file_processor
                .ensure_project_semantic_seed(&uri, &analysis_engine);
        }

        let mut snapshot = analysis_engine.register_project_paths_and_snapshot(project_files);
        let stale_project_file_ids = snapshot
            .view()
            .files()
            .filter(|file| {
                matches!(file.kind, SourceKind::Project | SourceKind::Excluded)
                    && snapshot
                        .view()
                        .semantic_export_fingerprint(file.id)
                        .is_some()
            })
            .map(|file| file.id)
            .collect::<Vec<_>>();
        // Withhold prior project facts from the pre-collection baseline. The
        // files stay registered in this private snapshot because the seed
        // below addresses them by path; the live engine is untouched, so this
        // is an empty update rather than a removal.
        for file_id in stale_project_file_ids {
            snapshot.update(file_id, FileAnalysis::default(), ResolveMode::Deferred);
        }
        let semantic_context = Arc::new(parking_lot::RwLock::new(snapshot));
        let semantic_target: Arc<dyn LoadTarget> = semantic_context.clone();
        let baseline_known_namespaces = Arc::new({
            let engine = semantic_context.read();
            engine.view().known_namespace_fqns()
        });
        let requires_direct_semantic_seed = !project_files.is_empty();
        if requires_direct_semantic_seed {
            let semantic_seed_started = Instant::now();
            let outcomes = project_files
                .par_iter()
                .map(|path| -> Result<(PathBuf, Option<FileAnalysis>)> {
                    let (content, _) = Self::read_authoritative_project_source(ctx, path)
                        .with_context(|| {
                            format!(
                                "failed to read project semantic seed source {}",
                                path.display()
                            )
                        })?;
                    let uri = Url::from_file_path(path).map_err(|_| {
                        anyhow!(
                            "project semantic seed path is not a valid file URI: {}",
                            path.display()
                        )
                    })?;
                    Ok((
                        path.clone(),
                        self.file_processor.collect_project_direct_semantic_seed(
                            &uri,
                            &content,
                            &semantic_target,
                            baseline_known_namespaces.as_ref(),
                        ),
                    ))
                })
                .collect::<Vec<_>>();
            let mut engine = semantic_context.write();
            let mut seeded = false;
            for outcome in outcomes {
                let (path, facts) = outcome?;
                let Some(facts) = facts else { continue };
                let file_id = engine.view().file_id(&path).unwrap_or_else(|| {
                    unreachable_invariant!(
                        what = "project semantic seed lost the registered identity for {}",
                        why = "the skeleton addresses the same pre-registered file as collection",
                        fix = "keep registration while installing the declaration barrier",
                        path.display(),
                    )
                });
                engine.update(file_id, facts, ResolveMode::Deferred);
                seeded = true;
            }
            if seeded {
                engine.resolve();
            }
            info!(
                "Project-wide declaration skeleton completed for {} files in {:?}",
                project_files.len(),
                semantic_seed_started.elapsed()
            );
        }
        let known_namespaces = Arc::new({
            let engine = semantic_context.read();
            engine.view().known_namespace_fqns()
        });
        let estimated_bytes = semantic_context
            .read()
            .view()
            .estimated_memory_stats()
            .total();
        invariant!(
            estimated_bytes <= MAX_EXHAUSTIVE_SEMANTIC_CONTEXT_BYTES,
            what = "semantic baseline for {} needs about {} bytes, over the {}-byte clone budget",
            why = "collection must not retain an unbounded snapshot",
            fix = "shrink the seed or replace the clone with a compact query projection",
            self.workspace_root.display(),
            estimated_bytes,
            MAX_EXHAUSTIVE_SEMANTIC_CONTEXT_BYTES,
        );
        info!(
            "Captured immutable pre-collection semantic baseline for {}: estimated_bytes={}, namespaces={}",
            self.workspace_root.display(),
            estimated_bytes,
            known_namespaces.len()
        );
        self.exhaustive_known_namespaces = Some(known_namespaces);
        self.exhaustive_semantic_context = Some(semantic_context);
        Ok(())
    }

    pub(super) fn resolve_open_project_files(
        &self,
        ctx: &LoadContext,
        analysis_engine: &Arc<dyn LoadTarget>,
    ) {
        let resolve_start = Instant::now();
        let mut open_project_paths = ctx
            .sources
            .open_uris()
            .iter()
            .filter_map(|uri| {
                let project_root = ctx.sink.project_root_for_uri(uri)?;
                (project_root == self.workspace_root)
                    .then(|| uri.to_file_path().ok())
                    .flatten()
            })
            .collect::<Vec<_>>();
        open_project_paths.sort();
        open_project_paths.dedup();
        let open_project_file_ids = analysis_engine.view(|view| {
            open_project_paths
                .iter()
                .map(|path| {
                    view.file_id(path).unwrap_or_else(|| {
                        unreachable_invariant!(
                            what = "open document {} has no registered analysis file after collection",
                            why = "didOpen and the project pass share one engine",
                            fix = "route open documents and the project pass to the same workspace owner",
                            path.display(),
                        )
                    })
                })
                .collect::<Vec<_>>()
        });
        analysis_engine.resolve_files(&open_project_file_ids);
        let resolve_elapsed = resolve_start.elapsed();
        info!(
            "Open project reference/diagnostic resolution completed in {:?} for {} document(s); \
             closed-file candidates remain deferred",
            resolve_elapsed,
            open_project_file_ids.len()
        );
        info!(
            "Project navigation stage completed in {:?}",
            resolve_elapsed
        );
    }
}
