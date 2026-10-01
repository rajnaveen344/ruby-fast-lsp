//! Gem discovery, startup-priority binding, and configured gem indexing.

use super::priority::{
    dependency_priority_key, prioritize_demanded_gem_names, prioritize_locked_gem_names,
};
use super::resources::{run_cpu_indexing_task, IndexingWorkClass};
use super::IndexingCoordinator;
use crate::environment::config::IndexingConfig;
use crate::environment::runtime::catalog::RuntimeImplementation;
use crate::invariant::ExpectInvariant;
use crate::loader::sources::gems::IndexerGem;
use crate::loader::version::ruby_version::RubyImplementation;
use crate::server::RubyLanguageServer;
use anyhow::{anyhow, Result};
use futures::stream::{self, StreamExt};
use log::info;
use ruby_analysis::engine::AnalysisEngine;
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// Overlap at most this many gem product loads ahead of lockfile-ordered bind.
/// Each load still claims gem-product transient memory, so two is the largest
/// prefetch that fits the 512 MiB governor without restoring unbounded hold.
const GEM_PRODUCT_LOAD_PREFETCH: usize = 2;

pub(super) fn configured_gem_selection(
    inferred: Vec<String>,
    config: &IndexingConfig,
) -> (HashSet<String>, HashSet<String>) {
    let excluded = config
        .excluded_gems
        .iter()
        .filter(|name| !name.is_empty())
        .cloned()
        .collect::<HashSet<_>>();
    let mut required = inferred.into_iter().collect::<HashSet<_>>();
    required.extend(
        config
            .included_gems
            .iter()
            .filter(|name| !name.is_empty())
            .cloned(),
    );
    required.retain(|name| !excluded.contains(name));
    (required, excluded)
}

impl IndexingCoordinator {
    pub(super) fn new_gem_indexer(&self) -> IndexerGem {
        let mut gem_indexer = IndexerGem::new(Some(self.workspace_root.clone()));
        gem_indexer.set_file_processor(
            self.file_processor
                .as_ref()
                .expect_invariant(
                    "gem indexing started before FileProcessor setup",
                    "every source kind must share the owning project's extension context",
                    "keep setup_file_processor before constructing the gem indexer",
                )
                .clone(),
        );
        gem_indexer.set_runtime_provider_fingerprint(
            self.jruby_import_provider
                .as_ref()
                .map(|provider| provider.classpath_fingerprint().to_string()),
        );
        if let Some(runtime) = self.effective_runtime.clone() {
            let implementation = match runtime.implementation {
                RuntimeImplementation::Mri => RubyImplementation::Mri,
                RuntimeImplementation::Jruby => RubyImplementation::JRuby,
                RuntimeImplementation::Truffleruby => RubyImplementation::TruffleRuby,
            };
            gem_indexer.set_selected_runtime(runtime.executable, implementation, runtime.java_home);
        }
        gem_indexer
    }

    pub(super) fn configure_discovered_gem_indexer(
        &self,
        mut gem_indexer: IndexerGem,
    ) -> IndexerGem {
        gem_indexer.set_file_processor(
            self.file_processor
                .as_ref()
                .expect_invariant(
                    "discovered gems were bound before the final project FileProcessor existed",
                    "JRuby-sensitive dependencies must use the exact completed runtime provider",
                    "install the final processor before dependency fact construction",
                )
                .clone(),
        );
        gem_indexer.set_runtime_provider_fingerprint(
            self.jruby_import_provider
                .as_ref()
                .map(|provider| provider.classpath_fingerprint().to_string()),
        );
        let mut inferred_required = self.get_required_gems();
        inferred_required.extend(
            gem_indexer
                .gemfile_required_roots_blocking()
                .expect_invariant(
                    "owning-project Gemfile could not be read while configuring discovered gems",
                    "bundler projects keep Gemfile next to the lockfile already used for discovery",
                    "keep Gemfile readable for the same project root that produced Gemfile.lock",
                ),
        );
        let (required_gems, excluded_gems) =
            configured_gem_selection(inferred_required, &self.config.indexing);

        gem_indexer.set_required_gems(required_gems);
        gem_indexer.set_explicitly_included_gems(
            self.config
                .indexing
                .included_gems
                .iter()
                .filter(|name| !name.is_empty() && !excluded_gems.contains(*name))
                .cloned()
                .collect(),
        );
        gem_indexer.set_excluded_gems(excluded_gems);
        gem_indexer.set_dependency_seed_engine(
            self.dependency_seed_engine.clone().expect_invariant(
                "gem indexing started before the dependency-only semantic seed was captured",
                "reusable gem facts must never depend on project-owned declarations",
                "capture the core/runtime engine immediately before indexing project sources",
            ),
        );
        gem_indexer
    }

    pub(super) async fn discover_and_bind_startup_priority_gems(
        server: &RubyLanguageServer,
        workspace_root: PathBuf,
        cancellation: Option<CancellationToken>,
        analysis_engine: Arc<parking_lot::RwLock<AnalysisEngine>>,
        gem_indexer: IndexerGem,
        dependency_seed: AnalysisEngine,
        priority_keys: HashSet<String>,
        excluded_gems: HashSet<String>,
        navigation_demands: Option<(
            crate::loader::scheduling::navigation_demand::NavigationDemandController,
            u64,
        )>,
        project_frontier_release: tokio::sync::oneshot::Sender<()>,
    ) -> Result<(IndexerGem, Duration)> {
        // An active constant may suggest a dependency name even in a plain
        // Ruby folder. Navigation discovery requires locked Bundler identity;
        // standalone roots use the complete path, which skips automatic gems.
        // Explicit includedGems is selected later, after project collection.
        if priority_keys.is_empty() || !workspace_root.join("Gemfile").is_file() {
            let (gem_indexer, discovery, discovery_dur) = run_cpu_indexing_task(
                server,
                Some(workspace_root),
                cancellation,
                IndexingWorkClass::ProjectCompanionIo,
                "gem dependency discovery",
                move || {
                    let started = Instant::now();
                    let mut gem_indexer = gem_indexer;
                    let discovery = gem_indexer.discover_gems_blocking();
                    (gem_indexer, discovery, started.elapsed())
                },
            )
            .await?;
            discovery?;
            let _ = project_frontier_release.send(());
            return Ok((gem_indexer, discovery_dur));
        }

        let navigation_priority_keys = priority_keys.clone();
        let (mut gem_indexer, discovery, navigation_discovery_dur) = run_cpu_indexing_task(
            server,
            Some(workspace_root.clone()),
            cancellation.clone(),
            IndexingWorkClass::ProjectCompanionIo,
            "active dependency discovery",
            move || {
                let started = Instant::now();
                let mut gem_indexer = gem_indexer;
                let discovery =
                    gem_indexer.discover_navigation_gems_blocking(&navigation_priority_keys);
                (gem_indexer, discovery, started.elapsed())
            },
        )
        .await?;
        discovery?;

        let mut priority_names = gem_indexer
            .priority_locked_gem_names(&priority_keys)
            .into_iter()
            .filter(|name| !excluded_gems.contains(name))
            .collect::<Vec<_>>();
        priority_names.sort();
        gem_indexer.set_required_gems(priority_names.iter().cloned().collect());
        gem_indexer.set_excluded_gems(excluded_gems);
        gem_indexer.set_dependency_seed_engine(dependency_seed);

        let mut bound_priority_files = 0usize;
        let priority_binding_started = Instant::now();
        for gem_name in &priority_names {
            if cancellation
                .as_ref()
                .is_some_and(CancellationToken::is_cancelled)
            {
                return Err(anyhow!(
                    "active dependency indexing for {} was cancelled before preparing {}",
                    workspace_root.display(),
                    gem_name
                ));
            }
            let (next_indexer, manifest, manifest_dur) = run_cpu_indexing_task(
                server,
                Some(workspace_root.clone()),
                cancellation.clone(),
                IndexingWorkClass::HeavyIo,
                "active dependency manifest preparation",
                {
                    let gem_name = gem_name.clone();
                    move || {
                        let started = Instant::now();
                        let manifest =
                            gem_indexer.prepare_required_gem_manifest_blocking(&gem_name);
                        (gem_indexer, manifest, started.elapsed())
                    }
                },
            )
            .await?;
            gem_indexer = next_indexer;
            let Some(manifest) = manifest? else {
                continue;
            };
            let binding_started = Instant::now();
            let bound = gem_indexer
                .bind_prepared_required_gem_with_shared_product(
                    server,
                    analysis_engine.clone(),
                    manifest,
                    cancellation.clone(),
                )
                .await?;
            bound_priority_files += bound.len();
            info!(
                "[PERF][active dependency binding] project={} gem={} manifest={:?} binding={:?} files={}",
                workspace_root.display(),
                gem_name,
                manifest_dur,
                binding_started.elapsed(),
                bound.len()
            );
        }
        if bound_priority_files > 0 {
            gem_indexer
                .resolve_bound_required_gems(server, analysis_engine, cancellation.clone())
                .await?;
            if let Some((demands, generation)) = &navigation_demands {
                for gem_name in &priority_names {
                    let key = dependency_priority_key(gem_name);
                    if demands.claim_if_requested(
                        *generation,
                        crate::loader::scheduling::navigation_demand::NavigationDemandStage::Dependency,
                        &key,
                    ) {
                        demands.complete_keys(
                            *generation,
                            crate::loader::scheduling::navigation_demand::NavigationDemandStage::Dependency,
                            std::slice::from_ref(&key),
                        );
                    }
                }
            }
        }
        info!(
            "[PERF][active dependency frontier] project={} gems={} files={} total={:?}",
            workspace_root.display(),
            priority_names.len(),
            bound_priority_files,
            priority_binding_started.elapsed()
        );
        let _ = project_frontier_release.send(());

        let (gem_indexer, completion, exhaustive_discovery_dur) = run_cpu_indexing_task(
            server,
            Some(workspace_root),
            cancellation,
            IndexingWorkClass::ProjectCompanionIo,
            "exhaustive vendor archive discovery",
            move || {
                let started = Instant::now();
                let mut gem_indexer = gem_indexer;
                let completion = gem_indexer.complete_navigation_gem_discovery_blocking();
                (gem_indexer, completion, started.elapsed())
            },
        )
        .await?;
        completion?;
        Ok((
            gem_indexer,
            navigation_discovery_dur + exhaustive_discovery_dur,
        ))
    }

    /// Bind already-discovered locked gems into one isolated project engine.
    ///
    /// Manifest preparation overlaps the next product's load through a bounded
    /// ordered pipeline. Bind remains strictly lockfile ordered. At most
    /// `GEM_PRODUCT_LOAD_PREFETCH` decoded products are in flight, plus the
    /// product currently being inserted.
    pub(super) async fn index_configured_gems(
        server: &RubyLanguageServer,
        workspace_root: PathBuf,
        cancellation: Option<CancellationToken>,
        analysis_engine: Arc<parking_lot::RwLock<AnalysisEngine>>,
        mut gem_indexer: IndexerGem,
        priority_keys: HashSet<String>,
        navigation_demands: Option<(
            crate::loader::scheduling::navigation_demand::NavigationDemandController,
            u64,
        )>,
    ) -> Result<IndexerGem> {
        if gem_indexer.needs_unlocked_explicit_discovery() {
            let (next_indexer, discovery) = run_cpu_indexing_task(
                server,
                Some(workspace_root.clone()),
                cancellation.clone(),
                IndexingWorkClass::HeavyIo,
                "explicit standalone gem discovery",
                move || {
                    let discovery = gem_indexer.discover_gems_blocking();
                    (gem_indexer, discovery)
                },
            )
            .await?;
            gem_indexer = next_indexer;
            discovery?;
        }
        let required_gem_names = prioritize_locked_gem_names(
            gem_indexer.ordered_required_gems_after_discovery(),
            &priority_keys,
        );
        let pipeline_started = Instant::now();
        let gem_indexer = Arc::new(gem_indexer);
        let producer_indexer = gem_indexer.clone();
        let producer_server = server.clone();
        let producer_root = workspace_root.clone();
        let producer_cancellation = cancellation.clone();
        let (manifest_sender, manifest_receiver) = tokio::sync::mpsc::channel::<
            Result<(
                String,
                crate::loader::cache::dependency_product::GemDependencyManifest,
                Vec<String>,
            )>,
        >(GEM_PRODUCT_LOAD_PREFETCH);
        let producer_navigation_demands = navigation_demands.clone();
        let manifest_producer = async move {
            let mut prepared_products = 0usize;
            let mut manifest_worker_wall = Duration::default();
            let mut manifest_wait_wall = Duration::default();
            let mut remaining_gem_names = VecDeque::from(required_gem_names);
            let mut demand_keys_by_gem = BTreeMap::<String, Vec<String>>::new();
            while !remaining_gem_names.is_empty() {
                if let Some((demands, generation)) = &producer_navigation_demands {
                    let keys = demands.drain(
                        *generation,
                        crate::loader::scheduling::navigation_demand::NavigationDemandStage::Dependency,
                    );
                    for (gem_name, mut keys) in
                        prioritize_demanded_gem_names(&mut remaining_gem_names, &keys)
                    {
                        demand_keys_by_gem
                            .entry(gem_name)
                            .or_default()
                            .append(&mut keys);
                    }
                }
                let gem_name = remaining_gem_names.pop_front().expect_invariant(
                    "non-empty gem pipeline queue had no first item",
                    "the loop and pop observe the same owned VecDeque",
                    "keep demand reprioritization within this producer",
                );
                let matched_demand_keys = demand_keys_by_gem.remove(&gem_name).unwrap_or_default();
                if producer_cancellation
                    .as_ref()
                    .is_some_and(CancellationToken::is_cancelled)
                {
                    let _ = manifest_sender
                        .send(Err(anyhow!(
                            "gem dependency indexing for {} was cancelled before preparing {}",
                            producer_root.display(),
                            gem_name
                        )))
                        .await;
                    break;
                }
                let manifest_wait_started = Instant::now();
                let worker_indexer = producer_indexer.clone();
                let manifest_gem_name = gem_name.clone();
                let preparation = run_cpu_indexing_task(
                    &producer_server,
                    Some(producer_root.clone()),
                    producer_cancellation.clone(),
                    IndexingWorkClass::HeavyIo,
                    "gem dependency manifest preparation",
                    move || {
                        let started = Instant::now();
                        let manifest = worker_indexer
                            .prepare_required_gem_manifest_blocking(&manifest_gem_name);
                        (manifest, started.elapsed())
                    },
                )
                .await;
                manifest_wait_wall += manifest_wait_started.elapsed();
                let (manifest, manifest_worker_elapsed) = match preparation {
                    Ok(prepared) => prepared,
                    Err(error) => {
                        let _ = manifest_sender.send(Err(error)).await;
                        break;
                    }
                };
                manifest_worker_wall += manifest_worker_elapsed;
                let manifest = match manifest {
                    Ok(Some(manifest)) => manifest,
                    Ok(None) => {
                        if !matched_demand_keys.is_empty() {
                            let (demands, generation) =
                                producer_navigation_demands.as_ref().expect_invariant(
                                    "matched dependency demand has no controller",
                                    "only a controller drain can create matched keys",
                                    "retain demand provenance with the gem producer",
                                );
                            demands.complete_keys(
                                *generation,
                                crate::loader::scheduling::navigation_demand::NavigationDemandStage::Dependency,
                                &matched_demand_keys,
                            );
                        }
                        continue;
                    }
                    Err(error) => {
                        let _ = manifest_sender.send(Err(error)).await;
                        break;
                    }
                };
                prepared_products += 1;
                if manifest_sender
                    .send(Ok((gem_name, manifest, matched_demand_keys)))
                    .await
                    .is_err()
                {
                    break;
                }
            }
            (prepared_products, manifest_worker_wall, manifest_wait_wall)
        };
        let consumer_indexer = gem_indexer.clone();
        let consumer_navigation_demands = navigation_demands;
        let consumer_root = workspace_root.clone();
        let manifest_consumer = async move {
            let mut indexed_files = 0usize;
            let mut product_load_wall = Duration::default();
            let mut product_binding_wall = Duration::default();
            let loads = stream::unfold(manifest_receiver, |mut receiver| async move {
                let item = receiver.recv().await?;
                Some((item, receiver))
            })
            .map(|item| {
                let indexer = consumer_indexer.clone();
                async move {
                    let (gem_name, manifest, matched_demand_keys) = item?;
                    let loading_started = Instant::now();
                    let loaded = indexer
                        .load_prepared_required_gem_with_shared_product(server, manifest)
                        .await?;
                    Ok::<_, anyhow::Error>((
                        gem_name,
                        loaded,
                        matched_demand_keys,
                        loading_started.elapsed(),
                    ))
                }
            })
            .buffered(GEM_PRODUCT_LOAD_PREFETCH);
            futures::pin_mut!(loads);
            while let Some(loaded) = loads.next().await {
                let (gem_name, loaded, matched_demand_keys, load_elapsed) = loaded?;
                product_load_wall += load_elapsed;
                let binding_started = Instant::now();
                let bound = consumer_indexer
                    .bind_loaded_required_gem_product(
                        server,
                        analysis_engine.clone(),
                        loaded,
                        cancellation.clone(),
                    )
                    .await?;
                indexed_files += bound.len();
                product_binding_wall += binding_started.elapsed();
                let dependency_key = dependency_priority_key(&gem_name);
                invariant!(
                    matched_demand_keys.iter().all(|key| key == &dependency_key),
                    what = "dependency product `{gem_name}` carried demand keys {matched_demand_keys:?} not matching `{dependency_key}`",
                    why = "producer and consumer must share one normalized gem identity",
                    fix = "attach demand provenance only to its exact gem",
                    gem_name = gem_name,
                    matched_demand_keys = matched_demand_keys,
                    dependency_key = dependency_key,
                );
                let requested =
                    consumer_navigation_demands
                        .as_ref()
                        .is_some_and(|(demands, generation)| {
                            demands.claim_if_requested(
                                *generation,
                                crate::loader::scheduling::navigation_demand::NavigationDemandStage::Dependency,
                                &dependency_key,
                            )
                        });
                if requested {
                    consumer_indexer
                        .resolve_bound_required_gems(
                            server,
                            analysis_engine.clone(),
                            cancellation.clone(),
                        )
                        .await?;
                    let (demands, generation) = consumer_navigation_demands
                        .as_ref()
                        .expect_invariant(
                        "bound dependency demand has no controller",
                        "matched keys originate only from the exact generation queue",
                        "retain the controller through product resolution and waiter completion",
                    );
                    demands.complete_keys(
                        *generation,
                        crate::loader::scheduling::navigation_demand::NavigationDemandStage::Dependency,
                        std::slice::from_ref(&dependency_key),
                    );
                    info!(
                        "[PERF][dependency navigation demand] project={} gem={} keys={} \
                         binding_and_resolution={:?}",
                        consumer_root.display(),
                        gem_name,
                        1,
                        binding_started.elapsed()
                    );
                }
            }
            Ok::<_, anyhow::Error>((indexed_files, product_load_wall, product_binding_wall))
        };
        let (producer_metrics, consumer_result) =
            tokio::join!(manifest_producer, manifest_consumer);
        let (prepared_products, manifest_worker_wall, manifest_wait_wall) = producer_metrics;
        let (indexed_files, product_load_wall, product_binding_wall) = consumer_result?;
        info!(
            "[PERF][gem dependency stream] project={} products={} files={} \
             manifest_worker={:?} manifest_wait={:?} product_load={:?} product_binding={:?} \
             pipeline={:?}",
            workspace_root.display(),
            prepared_products,
            indexed_files,
            manifest_worker_wall,
            manifest_wait_wall,
            product_load_wall,
            product_binding_wall,
            pipeline_started.elapsed()
        );
        Arc::try_unwrap(gem_indexer).map_err(|_| {
            anyhow!(
                "invariant violated: gem pipeline retained its IndexerGem after producer and consumer \
                 completion — bug: pipeline clones drop before coordinator ownership resumes — \
                 fix: scope IndexerGem clones to the joined producer and consumer futures"
            )
        })
    }

    /// Get the list of gems that the project needs
    fn get_required_gems(&self) -> Vec<String> {
        if let Some(ref project) = self.project_indexer {
            project.get_required_gems()
        } else {
            Vec::new()
        }
    }
}
