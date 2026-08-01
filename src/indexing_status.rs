use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Instant;
use tokio_util::sync::CancellationToken;
use tower_lsp::lsp_types::Url;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IndexingPhase {
    Discovered,
    Queued,
    ResolvingRuntime,
    DiscoveringInputs,
    IndexingCore,
    IndexingProject,
    ProjectNavigationReady,
    IndexingDependencies,
    ResolvingSemantics,
    DependencyNavigationReady,
    PublishingDiagnostics,
    Ready,
    Failed,
    Cancelled,
}

impl IndexingPhase {
    fn rank(self) -> u8 {
        match self {
            Self::Discovered => 0,
            Self::Queued => 1,
            Self::ResolvingRuntime => 2,
            Self::DiscoveringInputs => 3,
            Self::IndexingCore => 4,
            Self::IndexingProject => 5,
            Self::ProjectNavigationReady => 6,
            Self::IndexingDependencies => 7,
            Self::ResolvingSemantics => 8,
            Self::DependencyNavigationReady => 9,
            Self::PublishingDiagnostics => 10,
            Self::Ready => 11,
            Self::Failed | Self::Cancelled => 12,
        }
    }

    fn is_terminal(self) -> bool {
        matches!(self, Self::Ready | Self::Failed | Self::Cancelled)
    }

    pub fn project_navigation_pending(self) -> bool {
        matches!(
            self,
            Self::Discovered
                | Self::Queued
                | Self::ResolvingRuntime
                | Self::DiscoveringInputs
                | Self::IndexingCore
                | Self::IndexingProject
        )
    }

    pub fn dependency_navigation_pending(self) -> bool {
        matches!(
            self,
            Self::Discovered
                | Self::Queued
                | Self::ResolvingRuntime
                | Self::DiscoveringInputs
                | Self::IndexingCore
                | Self::IndexingProject
                | Self::ProjectNavigationReady
                | Self::IndexingDependencies
                | Self::ResolvingSemantics
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIndexingSnapshot {
    pub root: PathBuf,
    pub generation: u64,
    pub sequence: u64,
    pub phase: IndexingPhase,
    pub completed: Option<u64>,
    pub total: Option<u64>,
    pub elapsed_ms: u64,
    pub project_navigation_ready_ms: Option<u64>,
    pub dependency_navigation_ready_ms: Option<u64>,
    pub failure: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexingAggregateSnapshot {
    pub discovered: usize,
    pub queued: usize,
    pub active: usize,
    pub ready: usize,
    pub failed: usize,
    pub cancelled: usize,
    pub concurrency_limit: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexingPersistentProductReuseSnapshot {
    pub lookups: u64,
    pub hits: u64,
    pub producers: u64,
    pub corruptions: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexingSingleFlightReuseSnapshot {
    pub lookups: u64,
    pub hits: u64,
    pub joined_flights: u64,
    pub producers: u64,
    pub failures: u64,
}

/// Process-lifetime reuse evidence. These counters are intentionally not
/// attributed to one project: identical immutable work may be produced once
/// while several isolated projects wait for and bind the result independently.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexingReuseSnapshot {
    pub persistent_gem_products: IndexingPersistentProductReuseSnapshot,
    pub persistent_java_artifacts: IndexingPersistentProductReuseSnapshot,
    pub persistent_compiled_wasm: IndexingPersistentProductReuseSnapshot,
    pub gem_single_flight: IndexingSingleFlightReuseSnapshot,
    pub classpath_file_single_flight: IndexingSingleFlightReuseSnapshot,
    pub java_artifact_single_flight: IndexingSingleFlightReuseSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexingStatusSnapshot {
    pub sequence: u64,
    pub projects: Vec<ProjectIndexingSnapshot>,
    pub aggregate: IndexingAggregateSnapshot,
    pub reuse: IndexingReuseSnapshot,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexingStatusParams {
    #[serde(default)]
    pub active_document_uri: Option<Url>,
}

#[derive(Clone, Debug)]
pub struct IndexingRun {
    generation: u64,
    cancellation: CancellationToken,
}

impl IndexingRun {
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn cancellation(&self) -> CancellationToken {
        self.cancellation.clone()
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }
}

pub enum IndexingStatusNotification {}

impl tower_lsp::lsp_types::notification::Notification for IndexingStatusNotification {
    type Params = IndexingStatusSnapshot;
    const METHOD: &'static str = "ruby-fast-lsp/indexing/statusChanged";
}

impl ProjectIndexingSnapshot {
    pub fn is_ready(&self) -> bool {
        self.phase == IndexingPhase::Ready
    }
}

#[derive(Debug)]
struct ProjectIndexingState {
    generation: u64,
    sequence: u64,
    phase: IndexingPhase,
    completed: Option<u64>,
    total: Option<u64>,
    started_at: Option<Instant>,
    completed_elapsed_ms: Option<u64>,
    project_navigation_ready_ms: Option<u64>,
    dependency_navigation_ready_ms: Option<u64>,
    failure: Option<String>,
    cancellation: CancellationToken,
}

#[derive(Debug)]
pub struct ProjectIndexingStatus {
    root: PathBuf,
    state: Mutex<ProjectIndexingState>,
}

impl ProjectIndexingStatus {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            state: Mutex::new(ProjectIndexingState {
                generation: 0,
                sequence: 0,
                phase: IndexingPhase::Discovered,
                completed: None,
                total: None,
                started_at: None,
                completed_elapsed_ms: None,
                project_navigation_ready_ms: None,
                dependency_navigation_ready_ms: None,
                failure: None,
                cancellation: CancellationToken::new(),
            }),
        }
    }

    pub fn begin_generation(&self) -> ProjectIndexingSnapshot {
        let _run = self.begin_run();
        self.snapshot()
    }

    pub fn begin_run(&self) -> IndexingRun {
        let mut state = self.state.lock();
        state.cancellation.cancel();
        state.generation = state.generation.checked_add(1).expect(
            "INVARIANT VIOLATED: indexing generation overflowed. This is a bug because a project cannot start 2^64 indexing generations in one server lifetime. Fix: inspect the rebuild loop that exhausted the generation counter.",
        );
        state.sequence = state.sequence.checked_add(1).expect(
            "INVARIANT VIOLATED: indexing sequence overflowed. This is a bug because status updates must remain globally ordered for one project. Fix: inspect the progress loop that exhausted the sequence counter.",
        );
        state.phase = IndexingPhase::Queued;
        state.completed = None;
        state.total = None;
        state.started_at = Some(Instant::now());
        state.completed_elapsed_ms = None;
        state.project_navigation_ready_ms = None;
        state.dependency_navigation_ready_ms = None;
        state.failure = None;
        state.cancellation = CancellationToken::new();
        IndexingRun {
            generation: state.generation,
            cancellation: state.cancellation.clone(),
        }
    }

    pub fn transition(
        &self,
        generation: u64,
        phase: IndexingPhase,
        completed: Option<u64>,
        total: Option<u64>,
    ) -> Option<ProjectIndexingSnapshot> {
        assert!(
            completed.is_some() == total.is_some(),
            "INVARIANT VIOLATED: indexing progress has only one of completed/total. This is a bug because partial progress cannot define a defensible denominator. Fix: provide both counters or neither."
        );
        if let (Some(completed), Some(total)) = (completed, total) {
            assert!(
                completed <= total,
                "INVARIANT VIOLATED: completed work exceeds total work. This is a bug because indexing progress cannot be greater than its known denominator. Fix: correct the phase counter before publishing status."
            );
        }
        assert!(
            !matches!(phase, IndexingPhase::Discovered | IndexingPhase::Queued),
            "INVARIANT VIOLATED: an active indexing generation transitioned back to discovery or queue state. This is a bug because only begin_generation may create queued work. Fix: begin a new generation instead of rewinding the current one."
        );

        let mut state = self.state.lock();
        if generation != state.generation || state.phase.is_terminal() {
            return None;
        }
        assert!(
            phase.rank() >= state.phase.rank(),
            "INVARIANT VIOLATED: indexing phase moved backwards from {:?} to {:?}. This is a bug because status must be monotonic within a generation. Fix: publish the transition from the current coordinator phase or start a new generation.",
            state.phase,
            phase,
        );
        if phase == state.phase {
            if let (Some(previous), Some(next)) = (state.completed, completed) {
                assert!(
                    next >= previous,
                    "INVARIANT VIOLATED: indexing progress moved backwards within phase {:?}. This is a bug because one phase's completed count must be monotonic. Fix: use a new phase or publish the cumulative completed count.",
                    phase,
                );
            }
            if let (Some(previous), Some(next)) = (state.total, total) {
                assert!(
                    next == previous,
                    "INVARIANT VIOLATED: indexing progress denominator changed within phase {:?}. This is a bug because a displayed percentage must have a stable meaning. Fix: discover the total before entering the phase or start a new phase.",
                    phase,
                );
            }
        }

        state.sequence = state.sequence.checked_add(1).expect(
            "INVARIANT VIOLATED: indexing sequence overflowed. This is a bug because status updates must remain globally ordered for one project. Fix: inspect the progress loop that exhausted the sequence counter.",
        );
        state.phase = phase;
        state.completed = completed;
        state.total = total;
        state.failure = None;
        let elapsed_ms = elapsed_ms(&state);
        match phase {
            IndexingPhase::ProjectNavigationReady => {
                state.project_navigation_ready_ms = Some(elapsed_ms);
            }
            IndexingPhase::DependencyNavigationReady => {
                state.dependency_navigation_ready_ms = Some(elapsed_ms);
            }
            IndexingPhase::Ready => {
                state.completed_elapsed_ms = Some(elapsed_ms);
            }
            IndexingPhase::Discovered
            | IndexingPhase::Queued
            | IndexingPhase::ResolvingRuntime
            | IndexingPhase::DiscoveringInputs
            | IndexingPhase::IndexingCore
            | IndexingPhase::IndexingProject
            | IndexingPhase::IndexingDependencies
            | IndexingPhase::ResolvingSemantics
            | IndexingPhase::PublishingDiagnostics
            | IndexingPhase::Failed
            | IndexingPhase::Cancelled => {}
        }
        Some(self.snapshot_locked(&state))
    }

    pub fn fail(&self, generation: u64, failure: String) -> Option<ProjectIndexingSnapshot> {
        assert!(
            !failure.trim().is_empty(),
            "INVARIANT VIOLATED: failed indexing state has an empty error. This is a bug because users need an actionable terminal reason. Fix: pass the coordinator failure summary."
        );
        let mut state = self.state.lock();
        if generation != state.generation || state.phase.is_terminal() {
            return None;
        }
        state.sequence = state.sequence.checked_add(1).expect(
            "INVARIANT VIOLATED: indexing sequence overflowed. This is a bug because status updates must remain globally ordered for one project. Fix: inspect the failure loop that exhausted the sequence counter.",
        );
        state.phase = IndexingPhase::Failed;
        state.completed = None;
        state.total = None;
        state.failure = Some(failure);
        state.completed_elapsed_ms = Some(elapsed_ms(&state));
        Some(self.snapshot_locked(&state))
    }

    pub fn cancel(&self, generation: u64) -> Option<ProjectIndexingSnapshot> {
        let mut state = self.state.lock();
        if generation != state.generation || state.phase.is_terminal() {
            return None;
        }
        state.sequence = state.sequence.checked_add(1).expect(
            "INVARIANT VIOLATED: indexing sequence overflowed. This is a bug because status updates must remain globally ordered for one project. Fix: inspect the cancellation loop that exhausted the sequence counter.",
        );
        state.phase = IndexingPhase::Cancelled;
        state.completed = None;
        state.total = None;
        state.failure = None;
        state.cancellation.cancel();
        state.completed_elapsed_ms = Some(elapsed_ms(&state));
        Some(self.snapshot_locked(&state))
    }

    pub fn cancel_current(&self) -> Option<ProjectIndexingSnapshot> {
        let generation = self.state.lock().generation;
        self.cancel(generation)
    }

    pub fn is_current_run(&self, run: &IndexingRun) -> bool {
        let state = self.state.lock();
        state.generation == run.generation
            && !state.phase.is_terminal()
            && !run.cancellation.is_cancelled()
    }

    pub fn snapshot(&self) -> ProjectIndexingSnapshot {
        let state = self.state.lock();
        self.snapshot_locked(&state)
    }

    fn snapshot_locked(&self, state: &ProjectIndexingState) -> ProjectIndexingSnapshot {
        ProjectIndexingSnapshot {
            root: self.root.clone(),
            generation: state.generation,
            sequence: state.sequence,
            phase: state.phase,
            completed: state.completed,
            total: state.total,
            elapsed_ms: state
                .completed_elapsed_ms
                .unwrap_or_else(|| elapsed_ms(state)),
            project_navigation_ready_ms: state.project_navigation_ready_ms,
            dependency_navigation_ready_ms: state.dependency_navigation_ready_ms,
            failure: state.failure.clone(),
        }
    }
}

fn elapsed_ms(state: &ProjectIndexingState) -> u64 {
    state
        .started_at
        .map(|started_at| {
            u64::try_from(started_at.elapsed().as_millis()).expect(
                "INVARIANT VIOLATED: one indexing generation elapsed longer than u64::MAX milliseconds. This is a bug because no server process can run for that duration. Fix: inspect the corrupted indexing start instant.",
            )
        })
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn new_generation_supersedes_stale_updates() {
        let status = ProjectIndexingStatus::new(PathBuf::from("/workspace/admin"));
        let first = status.begin_generation();
        let second = status.begin_generation();

        assert_eq!(first.generation + 1, second.generation);
        assert!(status
            .transition(
                first.generation,
                IndexingPhase::IndexingProject,
                Some(1),
                Some(2),
            )
            .is_none());
        assert_eq!(status.snapshot().generation, second.generation);
        assert_eq!(status.snapshot().phase, IndexingPhase::Queued);
    }

    #[test]
    fn replacement_generation_cancels_previous_run_token() {
        let status = ProjectIndexingStatus::new(PathBuf::from("/workspace/admin"));
        let first = status.begin_run();
        assert!(!first.is_cancelled());

        let second = status.begin_run();

        assert!(first.is_cancelled());
        assert!(!second.is_cancelled());
        assert_eq!(second.generation(), first.generation() + 1);
    }

    #[test]
    fn progress_sequence_is_monotonic_within_one_phase() {
        let status = ProjectIndexingStatus::new(PathBuf::from("/workspace/admin"));
        let queued = status.begin_generation();
        let first = status
            .transition(
                queued.generation,
                IndexingPhase::IndexingProject,
                Some(1),
                Some(3),
            )
            .expect("current generation transition must be accepted");
        let second = status
            .transition(
                queued.generation,
                IndexingPhase::IndexingProject,
                Some(2),
                Some(3),
            )
            .expect("same-phase progress must be accepted");

        assert!(second.sequence > first.sequence);
        assert_eq!(second.completed, Some(2));
        assert_eq!(second.total, Some(3));
    }

    #[test]
    fn failed_generation_is_terminal_and_never_ready() {
        let status = ProjectIndexingStatus::new(PathBuf::from("/workspace/admin"));
        let queued = status.begin_generation();
        let failed = status
            .fail(
                queued.generation,
                "Gemfile.lock could not be read".to_string(),
            )
            .expect("current generation failure must be accepted");

        assert_eq!(failed.phase, IndexingPhase::Failed);
        assert!(!failed.is_ready());
        assert!(status
            .transition(queued.generation, IndexingPhase::Ready, None, None,)
            .is_none());
    }

    #[test]
    fn readiness_milestones_are_retained_and_terminal_elapsed_time_is_frozen() {
        let status = ProjectIndexingStatus::new(PathBuf::from("/workspace/admin"));
        let queued = status.begin_generation();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let project_ready = status
            .transition(
                queued.generation,
                IndexingPhase::ProjectNavigationReady,
                None,
                None,
            )
            .expect("project readiness transition must be accepted");
        std::thread::sleep(std::time::Duration::from_millis(2));
        let dependencies_ready = status
            .transition(
                queued.generation,
                IndexingPhase::DependencyNavigationReady,
                None,
                None,
            )
            .expect("dependency readiness transition must be accepted");
        let ready = status
            .transition(queued.generation, IndexingPhase::Ready, None, None)
            .expect("ready transition must be accepted");

        assert_eq!(
            ready.project_navigation_ready_ms,
            project_ready.project_navigation_ready_ms
        );
        assert_eq!(
            ready.dependency_navigation_ready_ms,
            dependencies_ready.dependency_navigation_ready_ms
        );
        assert!(
            ready.project_navigation_ready_ms.unwrap()
                <= ready.dependency_navigation_ready_ms.unwrap()
        );
        assert!(ready.dependency_navigation_ready_ms.unwrap() <= ready.elapsed_ms);

        std::thread::sleep(std::time::Duration::from_millis(2));
        assert_eq!(
            status.snapshot().elapsed_ms,
            ready.elapsed_ms,
            "terminal elapsed time must not continue increasing"
        );
    }

    #[test]
    fn semantic_resolution_precedes_dependency_navigation_readiness() {
        let status = ProjectIndexingStatus::new(PathBuf::from("/workspace/admin"));
        let queued = status.begin_generation();

        for phase in [
            IndexingPhase::IndexingDependencies,
            IndexingPhase::ResolvingSemantics,
            IndexingPhase::DependencyNavigationReady,
            IndexingPhase::PublishingDiagnostics,
            IndexingPhase::Ready,
        ] {
            status
                .transition(queued.generation, phase, None, None)
                .expect("the truthful dependency readiness sequence must be accepted");
        }

        assert_eq!(status.snapshot().phase, IndexingPhase::Ready);
        assert!(
            status.snapshot().dependency_navigation_ready_ms.is_some(),
            "dependency readiness must be retained after final publication"
        );
    }

    #[test]
    #[should_panic(expected = "completed work exceeds total work")]
    fn impossible_progress_panics_loudly() {
        let status = ProjectIndexingStatus::new(PathBuf::from("/workspace/admin"));
        let queued = status.begin_generation();
        let _ = status.transition(
            queued.generation,
            IndexingPhase::IndexingProject,
            Some(4),
            Some(3),
        );
    }
}
