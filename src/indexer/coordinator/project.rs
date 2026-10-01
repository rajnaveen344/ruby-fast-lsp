//! Project-file navigation frontier and exhaustive fact collection.

use super::priority::ActiveDocumentPriorityKeys;
use super::resources::{run_cpu_indexing_task, IndexingWorkClass};
use super::IndexingCoordinator;
use crate::indexer::sources::project::IndexerProject;
use crate::runtime::jruby::imports::JrubyImportProvider;
use crate::server::RubyLanguageServer;
use anyhow::{anyhow, Result};
use log::info;
use std::sync::Arc;
use std::time::Instant;
use tokio_util::sync::CancellationToken;

// Deterministic collection retains one batch's complete file-owned facts until
// every worker has observed the same immutable engine context. Keep the batch
// small enough that those retained facts stay inside the measured multi-root
// memory envelope while still amortizing registration and resolution work.
// File-progress counters still advance as each file in the batch is collected.
const EXHAUSTIVE_PROJECT_FILE_BATCH_SIZE: usize = 512;

impl IndexingCoordinator {
    /// Collect facts from project files (skips already-indexed files)
    pub(super) async fn collect_project_navigation_facts(
        &mut self,
        server: &RubyLanguageServer,
        priority_keys: ActiveDocumentPriorityKeys,
    ) -> Result<()> {
        let mut project_indexer = self.project_indexer.take().unwrap_or_else(|| {
            IndexerProject::new(
                self.workspace_root.clone(),
                self.file_processor.as_ref().unwrap().clone(),
                self.config.indexing.clone(),
            )
        });
        project_indexer
            .set_progress_generation(self.indexing_run.as_ref().map(|run| run.generation()));
        let frontier_demands = if let Some(run) = self.indexing_run.as_ref() {
            let workspace = server
                .list_workspaces()
                .into_iter()
                .find(|workspace| workspace.root_path == self.workspace_root)
                .ok_or_else(|| {
                    anyhow!(
                        "Indexing generation {} was cancelled because project {} is no longer registered",
                        run.generation(),
                        self.workspace_root.display()
                    )
                })?;
            Some((workspace.navigation_demands.clone(), run.generation()))
        } else {
            None
        };
        let worker_server = server.clone();
        let worker_root = self.workspace_root.clone();
        let (project_indexer, result) = run_cpu_indexing_task(
            server,
            Some(self.workspace_root.clone()),
            self.resource_cancellation(),
            IndexingWorkClass::ProjectParallelIo,
            "project navigation fact frontier",
            move || {
                project_indexer.set_navigation_priority_keys(
                    priority_keys.project_terminals.into_iter().collect(),
                    priority_keys.dependency_roots,
                );
                let initial_demand_keys = frontier_demands
                    .as_ref()
                    .map(|(demands, generation)| {
                        demands.drain(
                            *generation,
                            crate::navigation_demand::NavigationDemandStage::Project,
                        )
                    })
                    .unwrap_or_default();
                let result = project_indexer
                    .collect_initial_project_navigation_demand_facts(
                        &initial_demand_keys,
                        &worker_server,
                    )
                    .and_then(|selection| {
                        if let Some((demands, generation)) = frontier_demands.as_ref() {
                            if !selection.completed_keys.is_empty() {
                                demands.complete_keys(
                                    *generation,
                                    crate::navigation_demand::NavigationDemandStage::Project,
                                    &selection.completed_keys,
                                );
                            }
                            if !selection.deferred_keys.is_empty() {
                                info!(
                                    "Deferred {} ambiguous initial project navigation demand(s) \
                                     until exhaustive project completion for {}",
                                    selection.deferred_keys.len(),
                                    worker_root.display()
                                );
                            }
                        }
                        project_indexer.finish_project_navigation_facts(&worker_server)
                    })
                    .and_then(|()| {
                        let Some((demands, generation)) = frontier_demands.as_ref() else {
                            return Ok(());
                        };
                        let demand_keys = demands.drain(
                            *generation,
                            crate::navigation_demand::NavigationDemandStage::Project,
                        );
                        let selection = project_indexer.take_navigation_demand_files(&demand_keys);
                        let demanded_file_count = selection.files.len();
                        project_indexer.collect_project_file_batch(
                            &selection.files,
                            &worker_server,
                            true,
                        )?;
                        if !selection.completed_keys.is_empty() {
                            demands.complete_keys(
                                *generation,
                                crate::navigation_demand::NavigationDemandStage::Project,
                                &selection.completed_keys,
                            );
                        }
                        if !selection.deferred_keys.is_empty() {
                            info!(
                                "Deferred {} ambiguous project navigation demand(s) until \
                                 exhaustive project completion for {}",
                                selection.deferred_keys.len(),
                                worker_root.display()
                            );
                        }
                        if !demand_keys.is_empty() {
                            info!(
                                "[PERF][project demand frontier] project={} keys={} files={}",
                                worker_root.display(),
                                demand_keys.len(),
                                demanded_file_count
                            );
                        }
                        Ok(())
                    });
                (project_indexer, result)
            },
        )
        .await?;
        self.project_indexer = Some(project_indexer);
        result?;
        self.complete_processed_frontier_demands(server)?;
        Ok(())
    }

    fn complete_processed_frontier_demands(&self, server: &RubyLanguageServer) -> Result<()> {
        let Some(run) = self.indexing_run.as_ref() else {
            return Ok(());
        };
        let generation = run.generation();
        let workspace = server
            .list_workspaces()
            .into_iter()
            .find(|workspace| workspace.root_path == self.workspace_root)
            .ok_or_else(|| {
                anyhow!(
                    "Indexing generation {} was cancelled because project {} is no longer registered",
                    generation,
                    self.workspace_root.display()
                )
            })?;
        let priority_keys = self
            .project_indexer
            .as_ref()
            .expect(
                "INVARIANT VIOLATED: project frontier demand completion has no retained \
                 IndexerProject. This is a coordinator bug because processed-file evidence \
                 belongs to the exact frontier indexer. Fix: retain it before completing \
                 request waiters.",
            )
            .processed_navigation_priority_keys();
        let completed_keys = priority_keys
            .into_iter()
            .filter(|key| {
                workspace.navigation_demands.claim_if_requested(
                    generation,
                    crate::navigation_demand::NavigationDemandStage::Project,
                    key,
                )
            })
            .collect::<Vec<_>>();
        if !completed_keys.is_empty() {
            workspace.navigation_demands.complete_keys(
                generation,
                crate::navigation_demand::NavigationDemandStage::Project,
                &completed_keys,
            );
            info!(
                "Completed {} project navigation demand(s) from the active frontier for {}",
                completed_keys.len(),
                self.workspace_root.display()
            );
        }
        Ok(())
    }

    pub(super) async fn collect_remaining_project_facts(
        &mut self,
        server: &RubyLanguageServer,
        mut runtime_provider_ready_rx: Option<
            tokio::sync::oneshot::Receiver<Result<Option<Arc<JrubyImportProvider>>, String>>,
        >,
    ) -> Result<()> {
        let Some(run) = self.indexing_run.as_ref() else {
            let exact_runtime_provider = match runtime_provider_ready_rx.take() {
                Some(receiver) => receiver
                    .await
                    .map_err(|_| {
                        anyhow!(
                            "exact runtime provider task for {} ended before releasing exhaustive \
                             project collection",
                            self.workspace_root.display()
                        )
                    })?
                    .map_err(|error| {
                        anyhow!(
                            "exact runtime provider for {} failed before exhaustive project \
                             collection: {error}",
                            self.workspace_root.display()
                        )
                    })?,
                None => None,
            };
            return self
                .collect_remaining_project_facts_without_demands(server, exact_runtime_provider)
                .await;
        };
        let generation = run.generation();
        let workspace = server
            .list_workspaces()
            .into_iter()
            .find(|workspace| workspace.root_path == self.workspace_root)
            .ok_or_else(|| {
                anyhow!(
                    "Indexing generation {} was cancelled because project {} is no longer registered",
                    generation,
                    self.workspace_root.display()
                )
        })?;
        let demands = workspace.navigation_demands.clone();
        let started = Instant::now();
        self.indexing_checkpoint(server)?;
        let mut project_indexer = self.project_indexer.take().expect(
            "INVARIANT VIOLATED: bounded project collection has no retained navigation \
             frontier. This is a coordinator bug because dynamic demands and exhaustive batches \
             must mutate the exact IndexerProject that discovered the file set. Fix: retain the \
             frontier IndexerProject across every batch.",
        );
        let worker_server = server.clone();
        let worker_demands = demands.clone();
        let worker_root = self.workspace_root.clone();
        let worker_cancellation = self.resource_cancellation();
        let loop_cancellation = worker_cancellation.clone();
        let (next_project_indexer, result) = run_cpu_indexing_task(
            server,
            Some(self.workspace_root.clone()),
            worker_cancellation,
            IndexingWorkClass::ProjectParallelIo,
            "bounded exhaustive project fact collection",
            move || {
                let result = (|| -> Result<(usize, usize, usize, usize, usize)> {
                    project_indexer.refresh_exhaustive_semantic_context(&worker_server)?;
                    let mut batch_count = 0usize;
                    let mut demanded_batch_count = 0usize;
                    let mut collected_file_count = 0usize;
                    let mut providerless_batch_count = 0usize;
                    let mut provider_aware_batch_count = 0usize;
                    let mut provider_handoff_complete = runtime_provider_ready_rx.is_none();
                    let mut provider_installed = project_indexer.has_jruby_import_provider();
                    loop {
                        if loop_cancellation
                            .as_ref()
                            .is_some_and(CancellationToken::is_cancelled)
                        {
                            return Err(anyhow!(
                                "project source indexing generation {} for {} was cancelled \
                                 between bounded batches",
                                generation,
                                worker_root.display()
                            ));
                        }
                        if !provider_handoff_complete {
                            let receiver = runtime_provider_ready_rx.as_mut().expect(
                                "INVARIANT VIOLATED: an incomplete JRuby provider handoff has no receiver. This is a coordinator bug because the receiver and completion flag have one generation-owned lifecycle. Fix: retain the receiver until it yields one value or fails.",
                            );
                            match receiver.try_recv() {
                                Ok(Ok(Some(provider))) => {
                                    project_indexer.install_jruby_import_provider(provider);
                                    provider_installed = true;
                                    provider_handoff_complete = true;
                                }
                                Ok(Ok(None)) => {
                                    provider_handoff_complete = true;
                                }
                                Ok(Err(error)) => {
                                    return Err(anyhow!(
                                        "exact runtime provider for {} failed during bounded \
                                         project collection: {error}",
                                        worker_root.display()
                                    ));
                                }
                                Err(tokio::sync::oneshot::error::TryRecvError::Empty) => {}
                                Err(tokio::sync::oneshot::error::TryRecvError::Closed) => {
                                    return Err(anyhow!(
                                        "exact runtime provider task for {} ended without \
                                         publishing its generation-local handoff",
                                        worker_root.display()
                                    ));
                                }
                            }
                        }
                        let demand_keys = worker_demands.drain(
                            generation,
                            crate::navigation_demand::NavigationDemandStage::Project,
                        );
                        let selection = project_indexer.take_navigation_demand_files(&demand_keys);
                        let demanded = !selection.files.is_empty();
                        let files = if demanded {
                            selection.files
                        } else {
                            project_indexer.take_next_remaining_project_files(
                                EXHAUSTIVE_PROJECT_FILE_BATCH_SIZE,
                            )
                        };
                        let collected = files.len();
                        let remaining = project_indexer.remaining_project_file_count();
                        project_indexer.collect_project_file_batch(
                            &files,
                            &worker_server,
                            demanded || remaining == 0,
                        )?;
                        if provider_installed {
                            provider_aware_batch_count = provider_aware_batch_count
                                .checked_add(1)
                                .expect(
                                    "INVARIANT VIOLATED: provider-aware project batch count overflowed. This is a bug because one generation cannot contain 2^64 bounded batches. Fix: inspect the batch loop for a failure to consume pending files.",
                                );
                        } else {
                            providerless_batch_count = providerless_batch_count
                                .checked_add(1)
                                .expect(
                                    "INVARIANT VIOLATED: providerless project batch count overflowed. This is a bug because one generation cannot contain 2^64 bounded batches. Fix: inspect the batch loop for a failure to consume pending files.",
                                );
                        }
                        batch_count = batch_count.checked_add(1).expect(
                            "INVARIANT VIOLATED: project fact batch count overflowed. This is a \
                             bug because one generation cannot contain 2^64 bounded batches. \
                             Fix: inspect the batch loop for a failure to consume pending files.",
                        );
                        collected_file_count = collected_file_count.checked_add(collected).expect(
                            "INVARIANT VIOLATED: project fact batch file count overflowed. \
                                 This is a bug because the deterministic pending set is bounded by \
                                 the filesystem. Fix: inspect batch accounting and duplicate \
                                 extraction.",
                        );
                        if demanded {
                            demanded_batch_count = demanded_batch_count.checked_add(1).expect(
                                "INVARIANT VIOLATED: demanded project batch count overflowed. \
                                     This is a bug because the bounded request queue admits at \
                                     most a fixed number of keys per drain. Fix: inspect demand \
                                     completion and batch termination.",
                            );
                        }
                        if !selection.completed_keys.is_empty() {
                            worker_demands.complete_keys(
                                generation,
                                crate::navigation_demand::NavigationDemandStage::Project,
                                &selection.completed_keys,
                            );
                        }
                        if !selection.deferred_keys.is_empty() {
                            info!(
                                "Deferred {} ambiguous project navigation demand(s) until \
                                 exhaustive project completion for {}",
                                selection.deferred_keys.len(),
                                worker_root.display()
                            );
                        }
                        if remaining == 0 {
                            project_indexer.finish_remaining_project_facts();
                            worker_demands.complete_stage(
                                generation,
                                crate::navigation_demand::NavigationDemandStage::Project,
                            );
                            break;
                        }
                    }
                    Ok((
                        collected_file_count,
                        batch_count,
                        demanded_batch_count,
                        providerless_batch_count,
                        provider_aware_batch_count,
                    ))
                })();
                (project_indexer, result)
            },
        )
        .await?;
        self.project_indexer = Some(next_project_indexer);
        let (
            collected_file_count,
            batch_count,
            demanded_batch_count,
            providerless_batch_count,
            provider_aware_batch_count,
        ) = result?;
        self.indexing_checkpoint(server)?;
        info!(
            "[PERF][project batch stream] project={} files={} batches={} demanded_batches={} \
             providerless_batches={} provider_aware_batches={} total={:?}",
            self.workspace_root.display(),
            collected_file_count,
            batch_count,
            demanded_batch_count,
            providerless_batch_count,
            provider_aware_batch_count,
            started.elapsed()
        );
        Ok(())
    }

    async fn collect_remaining_project_facts_without_demands(
        &mut self,
        server: &RubyLanguageServer,
        exact_runtime_provider: Option<Arc<JrubyImportProvider>>,
    ) -> Result<()> {
        let mut project_indexer = self.project_indexer.take().expect(
            "INVARIANT VIOLATED: exhaustive project collection has no retained navigation \
             frontier. This is a coordinator bug because the remaining file set belongs to the \
             exact IndexerProject that discovered and indexed the priority files. Fix: retain \
             the frontier IndexerProject until exhaustive collection completes.",
        );
        if let Some(provider) = exact_runtime_provider {
            project_indexer.install_jruby_import_provider(provider);
        }
        let worker_server = server.clone();
        let (project_indexer, result) = run_cpu_indexing_task(
            server,
            Some(self.workspace_root.clone()),
            self.resource_cancellation(),
            IndexingWorkClass::ProjectParallelIo,
            "exhaustive project fact collection",
            move || {
                let result = project_indexer.collect_remaining_project_facts(&worker_server);
                (project_indexer, result)
            },
        )
        .await?;
        self.project_indexer = Some(project_indexer);
        result
    }

    pub(super) async fn replay_jruby_catalog_sensitive_project_facts(
        &mut self,
        server: &RubyLanguageServer,
    ) -> Result<usize> {
        let mut project_indexer = self.project_indexer.take().expect(
            "INVARIANT VIOLATED: JRuby catalog-sensitive replay started before the first project \
             pass. This is a coordinator bug because replay candidates are compact evidence \
             collected by the providerless active frontier. Fix: finish the exhaustive tail with \
             the exact provider before replaying the bounded frontier candidates.",
        );
        let file_processor = self.file_processor.clone().expect(
            "INVARIANT VIOLATED: JRuby catalog-sensitive replay has no final FileProcessor. This \
             is a coordinator bug because exact runtime facts must use the same extension and \
             project context as the ordinary project pass. Fix: rebuild the processor with the \
             completed provider before replay.",
        );
        let worker_server = server.clone();
        let (project_indexer, result) = run_cpu_indexing_task(
            server,
            Some(self.workspace_root.clone()),
            self.resource_cancellation(),
            IndexingWorkClass::ProjectParallelIo,
            "JRuby catalog-sensitive project replay",
            move || {
                let result = project_indexer
                    .replay_jruby_catalog_sensitive_files(file_processor, &worker_server);
                (project_indexer, result)
            },
        )
        .await?;
        self.project_indexer = Some(project_indexer);
        result
    }

    pub(super) async fn discard_jruby_replay_semantic_context(
        &mut self,
        server: &RubyLanguageServer,
    ) -> Result<()> {
        let mut project_indexer = self.project_indexer.take().expect(
            "INVARIANT VIOLATED: non-JRuby project completion has no retained project indexer. This is a coordinator bug because the immutable exhaustive read context belongs to that exact generation. Fix: retain the IndexerProject until replay context is consumed or discarded.",
        );
        let project_indexer = run_cpu_indexing_task(
            server,
            Some(self.workspace_root.clone()),
            self.resource_cancellation(),
            IndexingWorkClass::LightCpu,
            "discard non-JRuby exhaustive semantic context",
            move || {
                project_indexer.discard_jruby_replay_semantic_context();
                project_indexer
            },
        )
        .await?;
        self.project_indexer = Some(project_indexer);
        Ok(())
    }
}
