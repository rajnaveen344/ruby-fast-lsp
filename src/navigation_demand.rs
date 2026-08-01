use parking_lot::Mutex;
use std::collections::BTreeSet;
use std::sync::Arc;
use tokio::sync::watch;

pub(crate) const MAX_PENDING_NAVIGATION_DEMAND_KEYS: usize =
    crate::indexer::indexer_project::MAX_PROJECT_NAVIGATION_DEMAND_KEYS;

pub(crate) fn normalize_navigation_key(name: &str) -> String {
    name.chars()
        .filter(|character| character.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NavigationDemandStage {
    Project,
    Dependency,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NavigationDemandOutcome {
    TargetProcessed,
    StageComplete,
    Superseded,
    Cancelled,
    Saturated,
}

#[derive(Debug, Default)]
struct StageState {
    pending: BTreeSet<String>,
    in_flight: BTreeSet<String>,
    processed: BTreeSet<String>,
    complete: bool,
}

impl StageState {
    fn outstanding_len(&self) -> usize {
        self.pending.len().checked_add(self.in_flight.len()).expect(
            "INVARIANT VIOLATED: navigation demand outstanding-key count overflowed. This is \
                 a bug because admission caps each stage at a tiny fixed bound. Fix: keep all \
                 demand insertion routed through the bounded controller.",
        )
    }
}

#[derive(Debug, Default)]
struct NavigationDemandState {
    generation: Option<u64>,
    terminal: Option<NavigationDemandOutcome>,
    project: StageState,
    dependency: StageState,
}

impl NavigationDemandState {
    fn stage(&self, stage: NavigationDemandStage) -> &StageState {
        match stage {
            NavigationDemandStage::Project => &self.project,
            NavigationDemandStage::Dependency => &self.dependency,
        }
    }

    fn stage_mut(&mut self, stage: NavigationDemandStage) -> &mut StageState {
        match stage {
            NavigationDemandStage::Project => &mut self.project,
            NavigationDemandStage::Dependency => &mut self.dependency,
        }
    }
}

#[derive(Debug)]
struct NavigationDemandInner {
    state: Mutex<NavigationDemandState>,
    changed: watch::Sender<u64>,
}

#[derive(Clone, Debug)]
pub(crate) struct NavigationDemandController {
    inner: Arc<NavigationDemandInner>,
}

impl Default for NavigationDemandController {
    fn default() -> Self {
        let (changed, _receiver) = watch::channel(0);
        Self {
            inner: Arc::new(NavigationDemandInner {
                state: Mutex::new(NavigationDemandState::default()),
                changed,
            }),
        }
    }
}

impl NavigationDemandController {
    pub(crate) fn begin_generation(&self, generation: u64) {
        assert!(
            generation > 0,
            "INVARIANT VIOLATED: navigation demand generation is zero. This is a bug because \
             project indexing generations begin at one. Fix: call begin_generation only with \
             the exact IndexingRun generation."
        );
        {
            let mut state = self.inner.state.lock();
            if let Some(previous) = state.generation {
                assert!(
                    generation > previous,
                    "INVARIANT VIOLATED: navigation demand generation moved from {previous} to \
                     {generation}. This is a bug because replacement indexing generations must \
                     increase monotonically. Fix: begin demands from the exact new IndexingRun."
                );
            }
            *state = NavigationDemandState {
                generation: Some(generation),
                terminal: None,
                project: StageState::default(),
                dependency: StageState::default(),
            };
        }
        self.notify_change();
    }

    pub(crate) fn request(
        &self,
        generation: u64,
        stage: NavigationDemandStage,
        key: &str,
    ) -> NavigationDemandTicket {
        assert!(
            !key.is_empty() && key.chars().all(char::is_alphanumeric),
            "INVARIANT VIOLATED: navigation demand key `{key}` is not normalized. This is a bug \
             because demand selection accepts semantic identifiers, never paths or arbitrary \
             request text. Fix: normalize the identifier at the query adapter boundary."
        );
        let mut immediate = None;
        let mut inserted = false;
        {
            let mut state = self.inner.state.lock();
            if state.generation != Some(generation) {
                immediate = Some(NavigationDemandOutcome::Superseded);
            } else if let Some(terminal) = state.terminal {
                immediate = Some(terminal);
            } else {
                let stage_state = state.stage_mut(stage);
                if stage_state.processed.contains(key) {
                    immediate = Some(NavigationDemandOutcome::TargetProcessed);
                } else if stage_state.complete {
                    immediate = Some(NavigationDemandOutcome::StageComplete);
                } else if stage_state.pending.contains(key) || stage_state.in_flight.contains(key) {
                    // The existing exact key owns the bounded coordinator work.
                } else if stage_state.outstanding_len() >= MAX_PENDING_NAVIGATION_DEMAND_KEYS {
                    immediate = Some(NavigationDemandOutcome::Saturated);
                } else {
                    inserted = stage_state.pending.insert(key.to_string());
                    assert!(
                        inserted,
                        "INVARIANT VIOLATED: a new navigation demand key was not inserted. This \
                         is a bug because pending and in-flight duplicate cases were handled \
                         above. Fix: keep duplicate detection and insertion under one state lock."
                    );
                }
            }
        }
        if inserted {
            self.notify_change();
        }
        NavigationDemandTicket {
            controller: self.clone(),
            generation,
            stage,
            key: key.to_string(),
            changed: self.inner.changed.subscribe(),
            immediate,
        }
    }

    pub(crate) fn drain(&self, generation: u64, stage: NavigationDemandStage) -> Vec<String> {
        let drained = {
            let mut state = self.inner.state.lock();
            if state.generation != Some(generation) || state.terminal.is_some() {
                return Vec::new();
            }
            let stage_state = state.stage_mut(stage);
            if stage_state.complete {
                return Vec::new();
            }
            let pending = std::mem::take(&mut stage_state.pending);
            for key in &pending {
                assert!(
                    stage_state.in_flight.insert(key.clone()),
                    "INVARIANT VIOLATED: drained navigation key `{key}` was already in flight. \
                     This is a bug because request deduplication and drain run under one state \
                     lock. Fix: never copy a key between pending and in-flight sets."
                );
            }
            pending.into_iter().collect::<Vec<_>>()
        };
        if !drained.is_empty() {
            self.notify_change();
        }
        drained
    }

    pub(crate) fn claim_if_requested(
        &self,
        generation: u64,
        stage: NavigationDemandStage,
        key: &str,
    ) -> bool {
        assert!(
            !key.is_empty() && key.chars().all(char::is_alphanumeric),
            "INVARIANT VIOLATED: coordinator navigation key `{key}` is not normalized. This is \
             a bug because exact input completion must use the same identity as request \
             admission. Fix: derive coordinator keys through normalize_navigation_key."
        );
        let mut moved_to_in_flight = false;
        let requested = {
            let mut state = self.inner.state.lock();
            if state.generation != Some(generation) || state.terminal.is_some() {
                false
            } else {
                let stage_state = state.stage_mut(stage);
                if stage_state.complete || stage_state.processed.contains(key) {
                    false
                } else if stage_state.in_flight.contains(key) {
                    true
                } else if stage_state.pending.remove(key) {
                    assert!(
                        stage_state.in_flight.insert(key.to_string()),
                        "INVARIANT VIOLATED: pending navigation key `{key}` was already in \
                         flight. This is a bug because both sets are mutated under one lock. \
                         Fix: keep claim transitions atomic."
                    );
                    moved_to_in_flight = true;
                    true
                } else {
                    false
                }
            }
        };
        if moved_to_in_flight {
            self.notify_change();
        }
        requested
    }

    pub(crate) fn complete_keys(
        &self,
        generation: u64,
        stage: NavigationDemandStage,
        keys: &[String],
    ) {
        let changed = {
            let mut state = self.inner.state.lock();
            if state.generation != Some(generation) || state.terminal.is_some() {
                return;
            }
            let stage_state = state.stage_mut(stage);
            let mut changed = false;
            for key in keys {
                assert!(
                    stage_state.in_flight.remove(key),
                    "INVARIANT VIOLATED: completed navigation key `{key}` was not in flight. \
                     This is a coordinator bug because only drained keys may be completed. Fix: \
                     retain the exact drained key list until its bounded semantic insertion \
                     finishes."
                );
                changed |= stage_state.processed.insert(key.clone());
            }
            changed
        };
        if changed {
            self.notify_change();
        }
    }

    pub(crate) fn complete_stage(&self, generation: u64, stage: NavigationDemandStage) {
        let changed = {
            let mut state = self.inner.state.lock();
            if state.generation != Some(generation) || state.terminal.is_some() {
                return;
            }
            let stage_state = state.stage_mut(stage);
            if stage_state.complete {
                false
            } else {
                stage_state.complete = true;
                stage_state.pending.clear();
                stage_state.in_flight.clear();
                true
            }
        };
        if changed {
            self.notify_change();
        }
    }

    pub(crate) fn cancel_generation(&self, generation: u64) {
        let changed = {
            let mut state = self.inner.state.lock();
            if state.generation != Some(generation) || state.terminal.is_some() {
                false
            } else {
                state.terminal = Some(NavigationDemandOutcome::Cancelled);
                state.project.pending.clear();
                state.project.in_flight.clear();
                state.dependency.pending.clear();
                state.dependency.in_flight.clear();
                true
            }
        };
        if changed {
            self.notify_change();
        }
    }

    fn outcome_for(
        &self,
        generation: u64,
        stage: NavigationDemandStage,
        key: &str,
    ) -> Option<NavigationDemandOutcome> {
        let state = self.inner.state.lock();
        if state.generation != Some(generation) {
            return Some(NavigationDemandOutcome::Superseded);
        }
        if let Some(terminal) = state.terminal {
            return Some(terminal);
        }
        let stage_state = state.stage(stage);
        if stage_state.processed.contains(key) {
            Some(NavigationDemandOutcome::TargetProcessed)
        } else if stage_state.complete {
            Some(NavigationDemandOutcome::StageComplete)
        } else {
            None
        }
    }

    fn notify_change(&self) {
        self.inner.changed.send_modify(|revision| {
            *revision = revision.checked_add(1).expect(
                "INVARIANT VIOLATED: navigation demand revision overflowed. This is a bug because \
                 one process cannot publish 2^64 demand changes. Fix: inspect the request loop \
                 causing unbounded demand churn.",
            );
        });
    }
}

pub(crate) struct NavigationDemandTicket {
    controller: NavigationDemandController,
    generation: u64,
    stage: NavigationDemandStage,
    key: String,
    changed: watch::Receiver<u64>,
    immediate: Option<NavigationDemandOutcome>,
}

impl NavigationDemandTicket {
    #[cfg(test)]
    pub(crate) fn is_pending(&self) -> bool {
        self.immediate.is_none()
    }

    #[cfg(test)]
    pub(crate) fn immediate_outcome(&self) -> Option<NavigationDemandOutcome> {
        self.immediate
    }

    pub(crate) async fn wait(mut self) -> NavigationDemandOutcome {
        if let Some(outcome) = self.immediate {
            return outcome;
        }
        loop {
            if let Some(outcome) =
                self.controller
                    .outcome_for(self.generation, self.stage, &self.key)
            {
                return outcome;
            }
            self.changed.changed().await.expect(
                "INVARIANT VIOLATED: navigation demand change channel closed while a ticket \
                 retained its controller. This is a bug because the ticket's strong controller \
                 reference keeps the sender alive. Fix: keep ticket ownership and the change \
                 sender in the same shared controller.",
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn exact_generation_demand_completes_only_after_the_coordinator_processes_its_key() {
        let controller = NavigationDemandController::default();
        controller.begin_generation(7);

        let first = controller.request(7, NavigationDemandStage::Project, "userpmm");
        let duplicate = controller.request(7, NavigationDemandStage::Project, "userpmm");
        assert_eq!(
            controller.drain(7, NavigationDemandStage::Project),
            vec!["userpmm".to_string()],
            "duplicate requests must coalesce into one bounded coordinator input"
        );
        controller.complete_keys(7, NavigationDemandStage::Project, &["userpmm".to_string()]);

        assert_eq!(first.wait().await, NavigationDemandOutcome::TargetProcessed);
        assert_eq!(
            duplicate.wait().await,
            NavigationDemandOutcome::TargetProcessed
        );
    }

    #[tokio::test]
    async fn replacement_generation_supersedes_old_waiters_without_completing_the_new_stage() {
        let controller = NavigationDemandController::default();
        controller.begin_generation(11);
        let stale = controller.request(11, NavigationDemandStage::Project, "oldtarget");

        controller.begin_generation(12);

        assert_eq!(stale.wait().await, NavigationDemandOutcome::Superseded);
        let current = controller.request(12, NavigationDemandStage::Project, "newtarget");
        assert_eq!(
            controller.drain(11, NavigationDemandStage::Project),
            Vec::<String>::new(),
            "a stale coordinator must never consume a replacement generation's demand"
        );
        assert_eq!(
            controller.drain(12, NavigationDemandStage::Project),
            vec!["newtarget".to_string()]
        );
        controller.complete_stage(12, NavigationDemandStage::Project);
        assert_eq!(current.wait().await, NavigationDemandOutcome::StageComplete);
    }

    #[tokio::test]
    async fn coordinator_can_claim_a_request_that_arrives_while_its_input_is_in_flight() {
        let controller = NavigationDemandController::default();
        controller.begin_generation(19);
        let ticket = controller.request(19, NavigationDemandStage::Dependency, "bson");

        assert!(
            controller.claim_if_requested(19, NavigationDemandStage::Dependency, "bson"),
            "the consumer finishing an already-started gem must observe a request that arrived \
             after producer reordering"
        );
        controller.complete_keys(19, NavigationDemandStage::Dependency, &["bson".to_string()]);

        assert_eq!(
            ticket.wait().await,
            NavigationDemandOutcome::TargetProcessed
        );
        assert!(
            !controller.claim_if_requested(19, NavigationDemandStage::Dependency, "unrequested"),
            "ordinary exhaustive dependency binding must not create unbounded processed-key state"
        );
    }

    #[test]
    fn pending_keys_are_bounded_and_stage_completion_is_immediate() {
        let controller = NavigationDemandController::default();
        controller.begin_generation(3);
        for index in 0..MAX_PENDING_NAVIGATION_DEMAND_KEYS {
            assert!(controller
                .request(3, NavigationDemandStage::Project, &format!("target{index}"))
                .is_pending());
        }
        assert_eq!(
            controller
                .request(3, NavigationDemandStage::Project, "overflow")
                .immediate_outcome(),
            Some(NavigationDemandOutcome::Saturated)
        );

        controller.complete_stage(3, NavigationDemandStage::Dependency);
        assert_eq!(
            controller
                .request(3, NavigationDemandStage::Dependency, "bson")
                .immediate_outcome(),
            Some(NavigationDemandOutcome::StageComplete)
        );
    }
}
