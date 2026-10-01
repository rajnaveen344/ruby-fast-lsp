//! Core stubs, standard library, and dependency require-path indexing.

use super::resources::{run_cpu_indexing_task, IndexingWorkClass, MIB};
use super::runtime::runtime_stdlib_paths_for_project;
use super::IndexingCoordinator;
use crate::loader::context::LoadContext;
use crate::loader::file_processor::FileProcessor;
use crate::loader::require_paths::RequireFeatureIndex;
use crate::loader::scheduling::resources::{IndexingResourcePriority, IndexingWorkSpec};
use crate::loader::sources::stdlib::IndexerStdlib;
use crate::loader::version::ruby_version::RubyVersion;
use crate::server::RubyLanguageServer;
use anyhow::Result;
use log::info;
use ruby_analysis::engine::AnalysisEngine;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio_util::sync::CancellationToken;

async fn index_core_stubs_additively_off_reactor(
    ctx: &LoadContext,
    project_root: PathBuf,
    cancellation: Option<CancellationToken>,
    analysis_engine: Arc<parking_lot::RwLock<AnalysisEngine>>,
    ruby_version: Option<RubyVersion>,
    extension_path: Option<String>,
) -> Result<()> {
    run_cpu_indexing_task(
        &ctx.resources,
        Some(project_root),
        cancellation,
        IndexingWorkClass::ParallelIo,
        "additive core stub indexing",
        move || {
            let mut indexer = IndexerStdlib::new(FileProcessor::new(), ruby_version);
            if let Some(extension_path) = extension_path {
                indexer.set_extension_path(PathBuf::from(extension_path));
            }
            indexer.index_core_stubs_blocking(analysis_engine)
        },
    )
    .await?
}

impl IndexingCoordinator {
    /// Retain Bundler/RubyGems require roots for goto and unresolved-require diagnostics.
    pub(super) fn publish_dependency_require_paths(
        &mut self,
        ctx: &LoadContext,
        server: &RubyLanguageServer,
    ) -> Result<()> {
        self.indexing_checkpoint(ctx)?;
        let mut paths = Vec::new();
        if let Some(gem_indexer) = self.gem_indexer.as_ref() {
            paths.extend(gem_indexer.get_gem_lib_paths());
        }
        if let Some(stdlib_indexer) = self.stdlib_indexer.as_ref() {
            paths.extend(stdlib_indexer.get_stdlib_paths().iter().cloned());
        }
        let mut seen = std::collections::HashSet::new();
        paths.retain(|path| {
            !path.as_os_str().is_empty() && path.is_absolute() && seen.insert(path.clone())
        });
        let index_started = Instant::now();
        let index = {
            let analysis_engine = self.analysis_engine(ctx);
            let engine = analysis_engine.read();
            RequireFeatureIndex::build(&paths, Some(&engine))
        };
        let features = index.feature_count();
        let index = std::sync::Arc::new(index);
        info!(
            "[PERF][require feature index] project={} roots={} features={} elapsed={:?}",
            self.workspace_root.display(),
            paths.len(),
            features,
            index_started.elapsed()
        );
        self.indexing_checkpoint(ctx)?;
        if let Some(workspace) = server
            .list_workspaces()
            .into_iter()
            .find(|workspace| workspace.root_path == self.workspace_root)
        {
            if Arc::ptr_eq(&workspace.analysis_engine, &self.analysis_engine(ctx)) {
                workspace.set_dependency_require_resolution(paths.clone(), index.clone());
            }
        }
        if let Some(processor) = self.file_processor.as_mut() {
            processor.set_require_dependency_roots(paths);
            processor.set_require_feature_index(index);
        }
        Ok(())
    }

    /// Step 5: Index the Ruby standard library
    pub(super) async fn index_core_stubs(
        &self,
        ctx: &LoadContext,
        ruby_version: Option<RubyVersion>,
    ) -> Result<AnalysisEngine> {
        let analysis_engine = self.analysis_engine(ctx);
        let extension_path = self.config.extension_path.clone();
        let key = format!(
            "core-stubs:{}:{ruby_version:?}:{}",
            env!("CARGO_PKG_VERSION"),
            extension_path.as_deref().unwrap_or("<development>")
        );
        let indexing_resources = ctx.resources.clone();
        let resource_policy = indexing_resources.policy();
        let resource_spec = IndexingWorkSpec::new(
            Some(self.workspace_root.clone()),
            IndexingResourcePriority::Background,
            resource_policy.cpu_lanes(),
            256 * MIB,
            1,
        );
        let template = ctx
            .products
            .core_templates()
            .get_or_try_init(key, move || async move {
                indexing_resources
                    .run_parallel_with_resources(
                        "core stub template construction",
                        resource_spec,
                        None,
                        move || {
                            let template_engine =
                                Arc::new(parking_lot::RwLock::new(AnalysisEngine::new()));
                            let mut indexer = IndexerStdlib::new(
                                crate::loader::file_processor::FileProcessor::new(),
                                ruby_version,
                            );
                            if let Some(extension_path) = extension_path {
                                indexer.set_extension_path(PathBuf::from(extension_path));
                            }
                            indexer
                                .index_core_stubs_blocking(template_engine.clone())
                                .map_err(|error| error.to_string())?;
                            let engine = template_engine.read().clone();
                            Ok(engine)
                        },
                    )
                    .await
                    .map_err(|error| error.to_string())?
            })
            .await
            .map_err(anyhow::Error::msg)?;
        let dependency_seed = template.as_ref().clone();
        let installed_template = {
            let mut engine = analysis_engine.write();
            if engine.file_count() == 0 {
                *engine = template.as_ref().clone();
                true
            } else {
                false
            }
        };
        if installed_template {
            return Ok(dependency_seed);
        }

        // An active document may be opened while this coordinator waits for
        // the shared template producer. Replacing the engine at that point
        // would erase its current (possibly unsaved) facts. Keep the cached
        // clone as the empty-engine fast path, and use the ordinary per-file
        // lifecycle when live facts appeared during preparation.
        index_core_stubs_additively_off_reactor(
            ctx,
            self.workspace_root.clone(),
            self.resource_cancellation(),
            analysis_engine,
            ruby_version,
            self.config.extension_path.clone(),
        )
        .await?;
        Ok(dependency_seed)
    }

    /// Index runtime standard library modules after project declarations.
    pub(super) async fn index_standard_library(
        &mut self,
        ctx: &LoadContext,
        ruby_version: &Option<RubyVersion>,
    ) -> Result<()> {
        let required_stdlib = self.get_required_stdlib_modules();

        let mut stdlib_indexer =
            IndexerStdlib::new(self.file_processor.as_ref().unwrap().clone(), *ruby_version);

        // Pass extension path for loading zipped stubs
        if let Some(ref ext_path) = self.config.extension_path {
            stdlib_indexer.set_extension_path(PathBuf::from(ext_path));
        }
        if let Some(runtime) = self.effective_runtime.as_ref() {
            stdlib_indexer
                .set_selected_runtime(runtime.executable.clone(), runtime.java_home.clone());
            stdlib_indexer
                .set_runtime_stdlib_paths(runtime_stdlib_paths_for_project(ctx, runtime).await?);
        }

        stdlib_indexer.set_required_modules(required_stdlib);
        let analysis_engine = self.analysis_engine(ctx);
        let (stdlib_indexer, result) = run_cpu_indexing_task(
            &ctx.resources,
            Some(self.workspace_root.clone()),
            self.resource_cancellation(),
            IndexingWorkClass::ParallelIo,
            "runtime stdlib indexing",
            move || {
                let result = stdlib_indexer.index_runtime_stdlib_deferred_blocking(analysis_engine);
                (stdlib_indexer, result)
            },
        )
        .await?;
        result?;
        self.stdlib_indexer = Some(stdlib_indexer);
        Ok(())
    }

    /// Get the list of standard library modules that the project needs
    fn get_required_stdlib_modules(&self) -> Vec<String> {
        if let Some(ref project) = self.project_indexer {
            project.get_required_stdlib()
        } else {
            Vec::new()
        }
    }
}
