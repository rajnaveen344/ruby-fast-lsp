//! Indexing admission and process-wide status publication; no semantic store.
use super::RubyLanguageServer;
use crate::indexing_resources::IndexingResourceGovernor;
use crate::indexing_scheduler::IndexingScheduler;
use crate::indexing_status::{
    IndexingAggregateSnapshot, IndexingPersistentProductReuseSnapshot, IndexingPhase,
    IndexingReuseSnapshot, IndexingSingleFlightReuseSnapshot, IndexingStatusNotification,
    IndexingStatusParams, IndexingStatusSnapshot,
};
use log::warn;
#[cfg(test)]
use parking_lot::Mutex;
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::Client;

pub(super) const INDEXING_COUNTER_PUBLICATION_INTERVAL: Duration = Duration::from_millis(200);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum IndexingStatusPublicationDecision {
    Immediate,
    ScheduleCounterFlush,
    Coalesced,
}

#[derive(Debug, Default)]
pub(super) struct IndexingStatusPublicationState {
    last_published: Option<IndexingStatusSnapshot>,
    /// Latest sequenced snapshot waiting for the dedicated sender task.
    /// Intermediate updates are dropped so multi-project phase storms cannot
    /// fill tower-lsp's capacity-1 client channel and backpressure stdin dispatch.
    pending_send: Option<IndexingStatusSnapshot>,
    sender_scheduled: bool,
    counter_flush_scheduled: bool,
    counter_pending: bool,
}

impl IndexingStatusPublicationState {
    pub(super) fn observe(
        &mut self,
        snapshot: &IndexingStatusSnapshot,
    ) -> IndexingStatusPublicationDecision {
        let immediate = self
            .last_published
            .as_ref()
            .is_none_or(|published| !same_immediate_indexing_state(published, snapshot));
        if immediate {
            self.last_published = Some(snapshot.clone());
            self.counter_pending = false;
            return IndexingStatusPublicationDecision::Immediate;
        }

        self.counter_pending = true;
        if self.counter_flush_scheduled {
            IndexingStatusPublicationDecision::Coalesced
        } else {
            self.counter_flush_scheduled = true;
            IndexingStatusPublicationDecision::ScheduleCounterFlush
        }
    }

    pub(super) fn flush_counter(&mut self, snapshot: &IndexingStatusSnapshot) -> bool {
        self.counter_flush_scheduled = false;
        if !self.counter_pending {
            return false;
        }
        self.counter_pending = false;
        self.last_published = Some(snapshot.clone());
        true
    }

    pub(super) fn queue_send(&mut self, snapshot: IndexingStatusSnapshot) -> bool {
        self.pending_send = Some(snapshot);
        if self.sender_scheduled {
            return false;
        }
        self.sender_scheduled = true;
        true
    }

    pub(super) fn take_pending_send(&mut self) -> Option<IndexingStatusSnapshot> {
        match self.pending_send.take() {
            Some(snapshot) => Some(snapshot),
            None => {
                self.sender_scheduled = false;
                None
            }
        }
    }
}

fn same_immediate_indexing_state(
    published: &IndexingStatusSnapshot,
    next: &IndexingStatusSnapshot,
) -> bool {
    published.aggregate == next.aggregate
        && published.projects.len() == next.projects.len()
        && published
            .projects
            .iter()
            .zip(&next.projects)
            .all(|(published, next)| {
                published.root == next.root
                    && published.generation == next.generation
                    && published.phase == next.phase
                    && published.project_navigation_ready_ms == next.project_navigation_ready_ms
                    && published.dependency_navigation_ready_ms
                        == next.dependency_navigation_ready_ms
                    && published.failure == next.failure
            })
}

#[derive(Clone)]
pub(crate) struct IndexingServices {
    scheduler: IndexingScheduler,
    resources: IndexingResourceGovernor,
    pub(super) status: IndexingStatusPublisher,
    #[cfg(test)]
    pub(crate) schedule: Arc<crate::indexer::test_schedule::TestSchedule>,
    #[cfg(test)]
    pub(super) progress_reports: Arc<Mutex<Vec<(PathBuf, u64, u64)>>>,
}

#[derive(Clone)]
pub(super) struct IndexingStatusPublisher {
    sequence: Arc<AtomicU64>,
    publication: Arc<tokio::sync::Mutex<IndexingStatusPublicationState>>,
    wakeup: Arc<AtomicBool>,
    runtime: Option<tokio::runtime::Handle>,
}

impl Default for IndexingServices {
    fn default() -> Self {
        Self {
            scheduler: IndexingScheduler::default(),
            resources: IndexingResourceGovernor::default(),
            status: IndexingStatusPublisher {
                sequence: Arc::new(AtomicU64::new(0)),
                publication: Arc::new(tokio::sync::Mutex::new(
                    IndexingStatusPublicationState::default(),
                )),
                wakeup: Arc::new(AtomicBool::new(false)),
                runtime: tokio::runtime::Handle::try_current().ok(),
            },
            #[cfg(test)]
            schedule: Arc::default(),
            #[cfg(test)]
            progress_reports: Arc::default(),
        }
    }
}

impl IndexingServices {
    pub(crate) fn scheduler(&self) -> &IndexingScheduler {
        &self.scheduler
    }
    pub(crate) fn resources(&self) -> &IndexingResourceGovernor {
        &self.resources
    }
    /// Select admission policy before starting work (also used by the profiler).
    pub(crate) fn set_scheduler(&mut self, scheduler: IndexingScheduler) {
        self.scheduler = scheduler;
    }
    pub(crate) fn set_resources(&mut self, resources: IndexingResourceGovernor) {
        self.resources = resources;
    }
}

impl RubyLanguageServer {
    /// Select scheduling concurrency before starting work or sharing the server.
    pub fn set_indexing_concurrency(&mut self, concurrency: usize) {
        self.indexing
            .set_scheduler(IndexingScheduler::new(concurrency));
    }

    /// Select the resource budget before starting work or sharing the server.
    pub fn set_indexing_resource_policy(
        &mut self,
        policy: crate::indexing_resources::IndexingResourcePolicy,
    ) {
        self.indexing
            .set_resources(IndexingResourceGovernor::new(policy));
    }

    pub fn indexing_resource_policy(&self) -> crate::indexing_resources::IndexingResourcePolicy {
        self.indexing.resources().policy()
    }

    pub fn indexing_resource_snapshot(
        &self,
    ) -> crate::indexing_resources::IndexingResourceSnapshot {
        self.indexing.resources().snapshot()
    }

    pub fn indexing_scheduler_snapshot(
        &self,
    ) -> crate::indexing_scheduler::IndexingSchedulerSnapshot {
        self.indexing.scheduler().snapshot()
    }

    /// Register one project generation with the ordinary cancellation-aware scheduler.
    pub fn register_indexing_run(
        &self,
        project_root: std::path::PathBuf,
        priority: crate::indexing_scheduler::IndexingPriority,
        run: &crate::indexing_status::IndexingRun,
    ) -> crate::indexing_scheduler::IndexingAdmission {
        self.indexing
            .scheduler()
            .register_cancellable(project_root, priority, run.cancellation())
    }

    #[cfg(test)]
    pub(crate) fn indexing_progress_reports(&self) -> Vec<(PathBuf, u64, u64)> {
        self.indexing.progress_reports.lock().clone()
    }

    /// Returns true once every registered workspace has finished its initial
    /// indexing pass. With no workspaces registered, returns true vacuously
    /// (orphan-only mode has no coordinator to wait on).
    pub fn is_indexing_complete(&self) -> bool {
        self.projects
            .read()
            .iter()
            .all(|workspace| workspace.indexing_status.snapshot().is_ready())
    }

    pub fn prioritize_indexing_project(&self, project_root: &Path) {
        let navigation_pending = self
            .projects
            .read()
            .iter()
            .find(|workspace| workspace.root_path == project_root)
            .is_some_and(|workspace| {
                workspace
                    .indexing_status
                    .snapshot()
                    .phase
                    .project_navigation_pending()
            });
        self.indexing
            .scheduler()
            .prioritize_active_project(project_root);
        self.indexing
            .resources()
            .prioritize_active_project_with_navigation_pending(project_root, navigation_pending);
    }

    pub fn indexing_status_snapshot(&self) -> IndexingStatusSnapshot {
        let mut projects = self
            .projects
            .read()
            .iter()
            .map(|workspace| workspace.indexing_status.snapshot())
            .collect::<Vec<_>>();
        projects.sort_by(|left, right| left.root.cmp(&right.root));
        let scheduler = self.indexing.scheduler().snapshot();
        let mut aggregate = IndexingAggregateSnapshot {
            discovered: 0,
            queued: 0,
            active: scheduler.active,
            ready: 0,
            failed: 0,
            cancelled: 0,
            concurrency_limit: scheduler.concurrency_limit,
        };
        for project in &projects {
            match project.phase {
                IndexingPhase::Discovered => aggregate.discovered += 1,
                IndexingPhase::Queued => aggregate.queued += 1,
                IndexingPhase::Ready => aggregate.ready += 1,
                IndexingPhase::Failed => aggregate.failed += 1,
                IndexingPhase::Cancelled => aggregate.cancelled += 1,
                IndexingPhase::ResolvingRuntime
                | IndexingPhase::DiscoveringInputs
                | IndexingPhase::IndexingCore
                | IndexingPhase::IndexingProject
                | IndexingPhase::ProjectNavigationReady
                | IndexingPhase::IndexingDependencies
                | IndexingPhase::DependencyNavigationReady
                | IndexingPhase::ResolvingSemantics
                | IndexingPhase::PublishingDiagnostics => {}
            }
        }
        let persistent_gem_products = self.products.persistent().gem_product_snapshot();
        let persistent_java_artifacts = self.products.persistent().java_artifact_snapshot();
        let persistent_compiled_wasm = self.products.persistent().compiled_wasm_snapshot();
        let gem_single_flight = self.products.gem_dependencies().snapshot();
        let classpath_file_single_flight = self.products.classpath_files().snapshot();
        let java_artifact_single_flight = self.products.java_artifacts().snapshot();
        IndexingStatusSnapshot {
            sequence: self.indexing.status.sequence.load(Ordering::Acquire),
            projects,
            aggregate,
            reuse: IndexingReuseSnapshot {
                persistent_gem_products: IndexingPersistentProductReuseSnapshot {
                    lookups: persistent_gem_products.lookups,
                    hits: persistent_gem_products.hits,
                    producers: persistent_gem_products.producers,
                    corruptions: persistent_gem_products.corruptions,
                },
                persistent_java_artifacts: IndexingPersistentProductReuseSnapshot {
                    lookups: persistent_java_artifacts.lookups,
                    hits: persistent_java_artifacts.hits,
                    producers: persistent_java_artifacts.producers,
                    corruptions: persistent_java_artifacts.corruptions,
                },
                persistent_compiled_wasm: IndexingPersistentProductReuseSnapshot {
                    lookups: persistent_compiled_wasm.lookups,
                    hits: persistent_compiled_wasm.hits,
                    producers: persistent_compiled_wasm.producers,
                    corruptions: persistent_compiled_wasm.corruptions,
                },
                gem_single_flight: IndexingSingleFlightReuseSnapshot {
                    lookups: gem_single_flight.lookups,
                    hits: gem_single_flight.hits,
                    joined_flights: gem_single_flight.joined_flights,
                    producers: gem_single_flight.producers,
                    failures: gem_single_flight.failures,
                },
                classpath_file_single_flight: IndexingSingleFlightReuseSnapshot {
                    lookups: classpath_file_single_flight.lookups,
                    hits: classpath_file_single_flight.hits,
                    joined_flights: classpath_file_single_flight.joined_flights,
                    producers: classpath_file_single_flight.producers,
                    failures: classpath_file_single_flight.failures,
                },
                java_artifact_single_flight: IndexingSingleFlightReuseSnapshot {
                    lookups: java_artifact_single_flight.lookups,
                    hits: java_artifact_single_flight.hits,
                    joined_flights: java_artifact_single_flight.joined_flights,
                    producers: java_artifact_single_flight.producers,
                    failures: java_artifact_single_flight.failures,
                },
            },
        }
    }

    pub fn report_project_indexing_progress(
        &self,
        root: &Path,
        generation: Option<u64>,
        completed: u64,
        total: u64,
    ) {
        #[cfg(test)]
        self.indexing
            .progress_reports
            .lock()
            .push((root.to_path_buf(), completed, total));
        let Some(generation) = generation else {
            return;
        };
        let workspace = self
            .projects
            .read()
            .iter()
            .find(|workspace| workspace.root_path == root)
            .cloned();
        let Some(workspace) = workspace else {
            return;
        };
        if workspace
            .indexing_status
            .report_project_progress(generation, completed, total)
            .is_none()
        {
            return;
        }
        self.schedule_publish_indexing_status();
    }

    pub async fn handle_indexing_status(
        &self,
        params: IndexingStatusParams,
    ) -> LspResult<IndexingStatusSnapshot> {
        if let Some(active_document_uri) = params.active_document_uri {
            if let Some(workspace) = self.workspace_for_uri(&active_document_uri) {
                self.prioritize_indexing_project(&workspace.root_path);
            }
        }
        Ok(self.next_indexing_status_snapshot().await)
    }
}

impl IndexingStatusPublisher {
    pub(super) async fn next_indexing_status_snapshot(
        &self,
        server: &RubyLanguageServer,
    ) -> IndexingStatusSnapshot {
        let _publication = self.publication.lock().await;
        self.sequence_indexing_status_snapshot(server.indexing_status_snapshot())
    }

    pub(super) fn sequence_indexing_status_snapshot(
        &self,
        mut snapshot: IndexingStatusSnapshot,
    ) -> IndexingStatusSnapshot {
        let sequence = self.sequence
            .fetch_add(1, Ordering::AcqRel)
            .checked_add(1)
            .expect(
                "INVARIANT VIOLATED: global indexing status sequence overflowed. This is a bug because one server cannot publish 2^64 snapshots. Fix: inspect the status publication loop.",
            );
        snapshot.sequence = sequence;
        snapshot
    }

    pub(super) fn schedule_publish_indexing_status(&self, server: &RubyLanguageServer) {
        if self.wakeup.swap(true, Ordering::AcqRel) {
            return;
        }
        let server = server.clone();
        self.spawn_on_server_runtime(async move {
            loop {
                server
                    .indexing
                    .status
                    .wakeup
                    .store(false, Ordering::Release);
                server.publish_indexing_status().await;
                if !server.indexing.status.wakeup.swap(false, Ordering::AcqRel) {
                    return;
                }
            }
        });
    }

    pub(super) fn spawn_on_server_runtime<Fut>(&self, fut: Fut)
    where
        Fut: std::future::Future<Output = ()> + Send + 'static,
    {
        let handle = tokio::runtime::Handle::try_current()
            .ok()
            .or_else(|| self.runtime.clone());
        if let Some(handle) = handle {
            handle.spawn(fut);
        }
    }

    pub async fn publish_indexing_status(&self, server: &RubyLanguageServer) {
        let schedule_sender;
        let schedule_counter_flush;
        {
            let mut publication = self.publication.lock().await;
            let snapshot = server.indexing_status_snapshot();
            match publication.observe(&snapshot) {
                IndexingStatusPublicationDecision::Immediate => {
                    let snapshot = self.sequence_indexing_status_snapshot(snapshot);
                    schedule_sender = publication.queue_send(snapshot);
                    schedule_counter_flush = false;
                }
                IndexingStatusPublicationDecision::ScheduleCounterFlush => {
                    schedule_sender = false;
                    schedule_counter_flush = true;
                }
                IndexingStatusPublicationDecision::Coalesced => {
                    return;
                }
            }
        }
        if schedule_counter_flush {
            let server = server.clone();
            tokio::spawn(async move {
                sleep(INDEXING_COUNTER_PUBLICATION_INTERVAL).await;
                server.flush_indexing_counter_status().await;
            });
        }
        if schedule_sender {
            self.spawn_indexing_status_sender(server.client.clone());
        }
    }

    pub(super) async fn flush_indexing_counter_status(&self, server: &RubyLanguageServer) {
        let schedule_sender;
        {
            let mut publication = self.publication.lock().await;
            let snapshot = server.indexing_status_snapshot();
            if !publication.flush_counter(&snapshot) {
                return;
            }
            let snapshot = self.sequence_indexing_status_snapshot(snapshot);
            schedule_sender = publication.queue_send(snapshot);
        }
        if schedule_sender {
            self.spawn_indexing_status_sender(server.client.clone());
        }
    }

    pub(super) fn spawn_indexing_status_sender(&self, client: Option<Client>) {
        let publisher = self.clone();
        tokio::spawn(async move {
            publisher.drain_indexing_status_sends(client).await;
        });
    }

    pub(super) async fn drain_indexing_status_sends(&self, client: Option<Client>) {
        loop {
            let snapshot = {
                let mut publication = self.publication.lock().await;
                match publication.take_pending_send() {
                    Some(snapshot) => snapshot,
                    None => return,
                }
            };
            if let Some(client) = &client {
                let start = Instant::now();
                let _ = client
                    .send_notification::<IndexingStatusNotification>(snapshot)
                    .await;
                let elapsed = start.elapsed();
                if elapsed >= Duration::from_millis(50) {
                    warn!(
                        "[PERF] indexing status notification send took {:?} — stdout backpressure \
                         can stall LSP request dispatch when the client falls behind",
                        elapsed
                    );
                }
            }
        }
    }
}

impl RubyLanguageServer {
    async fn next_indexing_status_snapshot(&self) -> IndexingStatusSnapshot {
        self.indexing
            .status
            .next_indexing_status_snapshot(self)
            .await
    }
    #[cfg(test)]
    pub(super) fn sequence_indexing_status_snapshot(
        &self,
        snapshot: IndexingStatusSnapshot,
    ) -> IndexingStatusSnapshot {
        self.indexing
            .status
            .sequence_indexing_status_snapshot(snapshot)
    }
    fn schedule_publish_indexing_status(&self) {
        self.indexing.status.schedule_publish_indexing_status(self);
    }
    pub async fn publish_indexing_status(&self) {
        self.indexing.status.publish_indexing_status(self).await;
    }
    async fn flush_indexing_counter_status(&self) {
        self.indexing
            .status
            .flush_indexing_counter_status(self)
            .await;
    }
}
