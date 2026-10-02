//! Required-gem manifests, shared dependency products, and gem fact indexing.

use super::GemSource;
use super::IndexerGem;
use super::LoadedGemDependencyProduct;
use crate::invariant::ExpectInvariant;
use crate::loader::cache::dependency_product::{
    GemDependencyFileTemplate, GemDependencyManifest, GemDependencyProduct, GemDependencySource,
};
use crate::loader::context::LoadContext;
use crate::loader::file_processor::FileProcessor;
use crate::utils;
use crate::utils::admission::{IndexingResourcePriority, IndexingWorkSpec};
use anyhow::{anyhow, Context, Result};
use log::{debug, info};
use rayon::prelude::*;
use std::path::{Component, Path};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tower_lsp::lsp_types::Url;

const GEM_PRODUCT_COLLECTION_LANES: usize = 12;

pub(super) const GEM_PRODUCT_TRANSIENT_MEMORY_BYTES: usize = 256 * 1024 * 1024;

fn build_gem_dependency_product(
    manifest: &GemDependencyManifest,
    dependency_seed: ruby_analysis::engine::AnalysisEngine,
    processor: FileProcessor,
) -> Result<GemDependencyProduct> {
    let producer_engine = std::sync::Arc::new(parking_lot::RwLock::new(dependency_seed));
    let known_namespaces = std::sync::Arc::new({
        let engine = producer_engine.read();
        ruby_analysis::engine::AnalysisQuery::new(&engine).known_namespace_fqns()
    });
    let chunk_size = manifest
        .sources()
        .len()
        .div_ceil(GEM_PRODUCT_COLLECTION_LANES)
        .max(1);
    let templates = manifest
        .sources()
        .par_chunks(chunk_size)
        .map(|chunk| {
            chunk
                .iter()
                .map(|source| {
                    let uri = Url::from_file_path(&source.physical_path).map_err(|_| {
                        anyhow!(
                            "gem dependency source is not a valid file URI: {}",
                            source.physical_path.display()
                        )
                    })?;
                    let facts = processor.collect_project_neutral_file_template_without_insertion(
                        &uri,
                        source.content.as_str(),
                        producer_engine.clone(),
                        ruby_analysis::core::SourceKind::Gem,
                        known_namespaces.clone(),
                    )?;
                    Ok(GemDependencyFileTemplate::new(
                        source.logical_path.clone(),
                        source.content_sha256,
                        facts,
                    ))
                })
                .collect::<Result<Vec<_>>>()
        })
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();
    GemDependencyProduct::new(manifest, templates)
}

fn gem_source_label(source: GemSource) -> &'static str {
    match source {
        GemSource::BundlerInstalled => "bundler",
        GemSource::GlobalInstalled => "global",
        GemSource::VendorGit => "vendor-git",
        GemSource::VendorArchive => "vendor-archive",
    }
}

fn normalized_relative_path(path: &Path) -> Result<String> {
    let mut parts = Vec::new();
    for component in path.components() {
        let Component::Normal(part) = component else {
            return Err(anyhow!(
                "gem dependency path is not normalized and relative: {}",
                path.display()
            ));
        };
        parts.push(
            part.to_str()
                .ok_or_else(|| {
                    anyhow!("gem dependency path is not valid UTF-8: {}", path.display())
                })?
                .to_string(),
        );
    }
    if parts.is_empty() {
        return Err(anyhow!("gem dependency relative path is empty"));
    }
    Ok(parts.join("/"))
}

impl IndexerGem {
    #[cfg(test)]
    async fn index_prepared_required_gems_with_shared_product(
        &self,
        ctx: &LoadContext,
        analysis_engine: std::sync::Arc<parking_lot::RwLock<ruby_analysis::engine::AnalysisEngine>>,
        manifests: Vec<GemDependencyManifest>,
        cancellation: Option<CancellationToken>,
    ) -> Result<Vec<Url>> {
        let mut indexed_files = Vec::new();
        for manifest in manifests {
            indexed_files.extend(
                self.bind_prepared_required_gem_with_shared_product(
                    ctx,
                    analysis_engine.clone(),
                    manifest,
                    cancellation.clone(),
                )
                .await?,
            );
        }
        if !indexed_files.is_empty() {
            self.resolve_bound_required_gems(ctx, analysis_engine, cancellation)
                .await?;
        }
        info!(
            "Indexed {} files from checksum-keyed gem dependency product",
            indexed_files.len()
        );
        Ok(indexed_files)
    }

    pub(crate) async fn bind_prepared_required_gem_with_shared_product(
        &self,
        ctx: &LoadContext,
        analysis_engine: std::sync::Arc<parking_lot::RwLock<ruby_analysis::engine::AnalysisEngine>>,
        manifest: GemDependencyManifest,
        cancellation: Option<CancellationToken>,
    ) -> Result<Vec<Url>> {
        let loaded = self
            .load_prepared_required_gem_with_shared_product(ctx, manifest)
            .await?;
        self.bind_loaded_required_gem_product(ctx, analysis_engine, loaded, cancellation)
            .await
    }

    pub(crate) async fn load_prepared_required_gem_with_shared_product(
        &self,
        ctx: &LoadContext,
        manifest: GemDependencyManifest,
    ) -> Result<LoadedGemDependencyProduct> {
        let processor = self.file_processor.clone().ok_or_else(|| {
            anyhow!("gem dependency product requires a configured file processor")
        })?;
        let dependency_seed = self.dependency_seed_engine.clone().ok_or_else(|| {
            anyhow!(
                "gem dependency product requires the dependency-only core/runtime semantic seed"
            )
        })?;
        let project_root = self.workspace_root.clone().expect_invariant(
            "shared gem product indexing has no owning project root",
            "resource priority and semantic provenance must use the same isolated project",
            "construct IndexerGem with the owning project root",
        );
        let key = manifest.key().clone();
        let producer_manifest = manifest.clone();
        let indexing_resources = ctx.resources.clone();
        let persistent_cache = ctx.products.persistent().clone();
        let lookup_spec = IndexingWorkSpec::new(
            Some(project_root.clone()),
            IndexingResourcePriority::Background,
            1,
            GEM_PRODUCT_TRANSIENT_MEMORY_BYTES,
            1,
        );
        let producer_lanes = indexing_resources.project_parallel_cpu_lanes(&project_root);
        let producer_spec = IndexingWorkSpec::new(
            Some(project_root.clone()),
            IndexingResourcePriority::Background,
            producer_lanes,
            GEM_PRODUCT_TRANSIENT_MEMORY_BYTES,
            0,
        )
        .as_project_parallel();
        let producer_uses_shared_pool = producer_lanes == indexing_resources.policy().cpu_lanes();
        let product = ctx
            .products
            .gem_dependencies()
            .get_or_try_init(key, move || async move {
                let lookup_manifest = producer_manifest.clone();
                let lookup = indexing_resources
                    .run_with_resources(
                        "persistent gem dependency product lookup",
                        lookup_spec,
                        None,
                        move || {
                            persistent_cache
                                .lookup_or_reserve::<GemDependencyProduct>(&lookup_manifest)
                        },
                    )
                    .await
                    .map_err(|error| {
                        format!("persistent gem-product lookup worker failed: {error}")
                    })?
                    .map_err(|error| format!("persistent gem-product lookup failed: {error:#}"))?;
                let crate::utils::persistent_cache::PersistentProductLookup::Reservation(
                    reservation,
                ) = lookup
                else {
                    let crate::utils::persistent_cache::PersistentProductLookup::Hit(product) =
                        lookup
                    else {
                        unreachable_invariant!(
                            what = "persistent gem-product lookup returned an unhandled state",
                            why = "lookup has exactly hit and reservation outcomes",
                            fix = "handle every PersistentProductLookup variant explicitly",
                        );
                    };
                    return Ok(
                        Arc::try_unwrap(product).unwrap_or_else(|shared| shared.as_ref().clone())
                    );
                };
                let build_product = move || {
                    build_gem_dependency_product(&producer_manifest, dependency_seed, processor)
                        .map_err(|error| error.to_string())
                };
                let product = if producer_uses_shared_pool {
                    indexing_resources
                        .run_parallel_with_resources(
                            "gem dependency product construction",
                            producer_spec,
                            None,
                            build_product,
                        )
                        .await
                } else {
                    indexing_resources
                        .run_partitioned_parallel_with_resources(
                            "gem dependency product construction",
                            producer_spec,
                            None,
                            build_product,
                        )
                        .await
                }
                .map_err(|error| format!("gem dependency product worker failed: {error}"))??;
                let publication_spec = IndexingWorkSpec::new(
                    Some(project_root),
                    IndexingResourcePriority::Background,
                    1,
                    GEM_PRODUCT_TRANSIENT_MEMORY_BYTES,
                    1,
                );
                indexing_resources
                    .run_with_resources(
                        "persistent gem dependency product publication",
                        publication_spec,
                        None,
                        move || {
                            reservation.publish(&product).map_err(|error| {
                                format!("persistent gem-product publication failed: {error:#}")
                            })?;
                            Ok(product)
                        },
                    )
                    .await
                    .map_err(|error| {
                        format!("persistent gem-product publication worker failed: {error}")
                    })?
            })
            .await
            .map_err(anyhow::Error::msg)?;
        Ok(LoadedGemDependencyProduct { manifest, product })
    }

    pub(crate) async fn bind_loaded_required_gem_product(
        &self,
        ctx: &LoadContext,
        analysis_engine: std::sync::Arc<parking_lot::RwLock<ruby_analysis::engine::AnalysisEngine>>,
        loaded: LoadedGemDependencyProduct,
        cancellation: Option<CancellationToken>,
    ) -> Result<Vec<Url>> {
        let project_root = self.workspace_root.clone().expect_invariant(
            "shared gem product binding has no owning project root",
            "resource priority and semantic provenance must use the same isolated project",
            "construct IndexerGem with the owning project root",
        );
        let LoadedGemDependencyProduct { manifest, product } = loaded;
        let binding_engine = analysis_engine;
        let binding_spec = IndexingWorkSpec::new(
            Some(project_root),
            IndexingResourcePriority::Background,
            1,
            GEM_PRODUCT_TRANSIENT_MEMORY_BYTES,
            0,
        );
        let binding = match ctx
            .resources
            .run_with_resources(
                "gem dependency product binding",
                binding_spec,
                cancellation,
                move || {
                    product.bind_owned_deferred_into_measured(manifest, &mut binding_engine.write())
                },
            )
            .await?
        {
            Ok(binding) => binding,
            Err(error) => {
                crate::loader::cache::dependency_product::GemDependencyBinding::record_failure(
                    ctx.products.gem_bindings(),
                );
                return Err(error);
            }
        };
        binding.record_success(ctx.products.gem_bindings());
        Ok(binding.uris)
    }

    pub(crate) async fn resolve_bound_required_gems(
        &self,
        ctx: &LoadContext,
        analysis_engine: std::sync::Arc<parking_lot::RwLock<ruby_analysis::engine::AnalysisEngine>>,
        cancellation: Option<CancellationToken>,
    ) -> Result<()> {
        let project_root = self.workspace_root.clone().expect_invariant(
            "gem dependency resolution has no owning project root",
            "resource priority and semantic provenance must use the same isolated project",
            "construct IndexerGem with the owning project root",
        );
        let resolving_engine = analysis_engine.clone();
        let resolution_spec = IndexingWorkSpec::new(
            Some(project_root),
            IndexingResourcePriority::Background,
            1,
            GEM_PRODUCT_TRANSIENT_MEMORY_BYTES,
            0,
        );
        ctx.resources
            .run_with_resources(
                "gem dependency semantic resolution",
                resolution_spec,
                cancellation,
                move || {
                    resolving_engine.write().resolve();
                },
            )
            .await?;
        Ok(())
    }

    #[cfg(test)]
    pub(super) async fn index_required_gems_with_shared_product(
        &self,
        ctx: &LoadContext,
        analysis_engine: std::sync::Arc<parking_lot::RwLock<ruby_analysis::engine::AnalysisEngine>>,
    ) -> Result<Vec<Url>> {
        let seed = self
            .dependency_seed_engine
            .as_ref()
            .ok_or_else(|| {
                anyhow!(
                    "gem dependency product requires the dependency-only core/runtime semantic seed"
                )
            })?
            .semantic_context_fingerprint();
        let manifests = self.required_gem_manifests(seed)?;
        self.index_prepared_required_gems_with_shared_product(ctx, analysis_engine, manifests, None)
            .await
    }

    pub(crate) fn prepare_required_gem_manifest_blocking(
        &self,
        gem_name: &str,
    ) -> Result<Option<GemDependencyManifest>> {
        let seed = self
            .dependency_seed_engine
            .as_ref()
            .ok_or_else(|| {
                anyhow!(
                    "gem dependency product requires the dependency-only core/runtime semantic seed"
                )
            })?
            .semantic_context_fingerprint();
        self.required_gem_manifest(gem_name, seed)
    }

    #[cfg(test)]
    pub(super) fn required_gem_manifests(
        &self,
        seed: ruby_analysis::engine::SemanticExportFingerprint,
    ) -> Result<Vec<GemDependencyManifest>> {
        let required_gems = self.required_gems_with_dependencies();
        let mut manifests = Vec::with_capacity(required_gems.len());
        for gem_name in &required_gems {
            if let Some(manifest) = self.required_gem_manifest(gem_name, seed)? {
                manifests.push(manifest);
            }
        }
        Ok(manifests)
    }

    pub(super) fn required_gem_manifest(
        &self,
        gem_name: &str,
        seed: ruby_analysis::engine::SemanticExportFingerprint,
    ) -> Result<Option<GemDependencyManifest>> {
        let Some(gem_versions) = self.discovered_gems.get(gem_name) else {
            debug!("Required gem not found: {}", gem_name);
            return Ok(None);
        };
        let Some(gem_info) = self.select_preferred_version(gem_versions) else {
            return Ok(None);
        };
        let mut semantic_identities = vec![format!(
            "{}:{}:{}:{}",
            gem_info.name,
            gem_info.locked_version,
            gem_info.platform,
            gem_source_label(gem_info.source)
        )];
        let mut dependencies = gem_info.dependencies.clone();
        dependencies.sort();
        semantic_identities.extend(
            dependencies
                .into_iter()
                .map(|dependency| format!("dependency:{dependency}")),
        );
        info!(
            "Preparing required gem product input: {} v{} platform={} source={:?}",
            gem_info.name, gem_info.version, gem_info.platform, gem_info.source
        );
        let mut sources = Vec::new();
        for (lib_index, lib_path) in gem_info.lib_paths.iter().enumerate() {
            if !lib_path.is_dir() {
                continue;
            }
            let mut ruby_files = utils::file_ops::collect_ruby_files(lib_path);
            ruby_files.sort();
            for file_path in ruby_files {
                let relative = file_path.strip_prefix(lib_path).with_context(|| {
                    format!(
                        "gem source {} is outside require path {}",
                        file_path.display(),
                        lib_path.display()
                    )
                })?;
                let logical_path = format!(
                    "gems/{}/{}/{}/{}/{}/{}",
                    gem_info.name,
                    gem_info.locked_version,
                    gem_info.platform,
                    gem_source_label(gem_info.source),
                    lib_index,
                    normalized_relative_path(relative)?
                );
                let content = std::fs::read_to_string(&file_path).with_context(|| {
                    format!("failed to read gem dependency {}", file_path.display())
                })?;
                sources.push(GemDependencySource::new(
                    0,
                    logical_path,
                    file_path,
                    content,
                    gem_info.name.clone(),
                    gem_info.locked_version.clone(),
                )?);
            }
        }
        if sources.is_empty() {
            return Ok(None);
        }
        let runtime_provider_fingerprint =
            self.runtime_provider_fingerprint.as_deref().filter(|_| {
                sources.iter().any(|source| {
                    ruby_fast_lsp_jruby_support::source_semantics_depend_on_jruby_catalog(
                        source.content.as_str(),
                    )
                })
            });
        GemDependencyManifest::new(
            seed,
            runtime_provider_fingerprint,
            &semantic_identities,
            sources,
        )
        .map(Some)
    }
}
