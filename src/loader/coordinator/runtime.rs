//! Effective Ruby runtime, version, and library-path discovery for a project.

use super::resources::{run_cpu_indexing_task, IndexingWorkClass};
use super::IndexingCoordinator;
use crate::environment::config::runtime::{EffectiveRuntimeSelection, SelectedRuntimeDescriptor};
use crate::environment::runtime::catalog::RuntimeImplementation;
use crate::loader::context::LoadContext;
use crate::loader::sources::stdlib::{RuntimeStdlibPathKey, RuntimeStdlibPaths};
use crate::loader::version::ruby_version::{RubyImplementation, RubyVersion};
use crate::server::RubyLanguageServer;
use anyhow::Result;
use log::info;
use std::time::Instant;

pub(super) async fn runtime_stdlib_paths_for_project(
    ctx: &LoadContext,
    runtime: &SelectedRuntimeDescriptor,
) -> Result<RuntimeStdlibPaths> {
    let key = RuntimeStdlibPathKey::new(&runtime.executable, runtime.java_home.as_deref())?;
    let producer_key = key.clone();
    let producer_resources = ctx.resources.clone();
    let started = Instant::now();
    let product = ctx
        .products
        .stdlib_paths()
        .get_or_try_init(key, move || async move {
            run_cpu_indexing_task(
                &producer_resources,
                None,
                None,
                IndexingWorkClass::Io,
                "exact runtime stdlib path discovery",
                move || producer_key.discover(),
            )
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())
        })
        .await
        .map_err(anyhow::Error::msg)?;
    info!(
        "[PERF][runtime stdlib path product] executable={} paths={} wait={:?}",
        runtime.executable.display(),
        product.paths().len(),
        started.elapsed()
    );
    Ok(product.as_ref().clone())
}

fn ruby_version_for_runtime(runtime: &SelectedRuntimeDescriptor) -> Option<RubyVersion> {
    let mut components = runtime.compatibility_version.split('.');
    let major = components.next()?.parse::<u8>().ok()?;
    let minor = components.next()?.parse::<u8>().ok()?;
    let implementation = match runtime.implementation {
        RuntimeImplementation::Mri => RubyImplementation::Mri,
        RuntimeImplementation::Jruby => RubyImplementation::JRuby,
        RuntimeImplementation::Truffleruby => RubyImplementation::TruffleRuby,
    };
    Some(RubyVersion::new_with_implementation(
        major,
        minor,
        implementation,
    ))
}

impl IndexingCoordinator {
    /// Step 1: Select the Ruby version for the already resolved runtime.
    pub(super) async fn detect_ruby_version_off_reactor(
        &mut self,
        _server: &RubyLanguageServer,
    ) -> Result<Option<RubyVersion>> {
        if let Some(runtime) = &self.effective_runtime {
            let version = ruby_version_for_runtime(runtime);
            self.detected_ruby_version = version;
            return Ok(version);
        }
        if let Some(version) = self.config.get_ruby_version().map(RubyVersion::from_tuple) {
            self.detected_ruby_version = Some(version);
            return Ok(Some(version));
        }
        // Auto was already resolved against the exact runtime catalog. A
        // marker with no installed match is an unfulfilled runtime request,
        // not permission to reconstruct compatibility through another probe.
        // None selects the bundled Ruby 3.0 core fallback; explicit legacy
        // compatibility above remains authoritative without an executable.
        self.detected_ruby_version = None;
        Ok(None)
    }

    pub(super) async fn resolve_effective_runtime(&mut self, ctx: &LoadContext) -> Result<()> {
        let root = self.workspace_root.to_string_lossy();
        self.effective_runtime = match self
            .config
            .runtime
            .selection_for_project(&root, &self.config.ruby_version)
        {
            EffectiveRuntimeSelection::Explicit(runtime) => Some(runtime),
            EffectiveRuntimeSelection::Auto => {
                ctx.discovery
                    .resolve_auto_runtime(&self.workspace_root)
                    .await?
            }
            EffectiveRuntimeSelection::LegacyMriCompatibility { .. } => None,
        };
        ctx.sink
            .select_runtime(&self.workspace_root, self.effective_runtime.clone());
        Ok(())
    }
}
