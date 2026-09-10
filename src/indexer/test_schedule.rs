//! Test-only one-shot gates at real lifecycle boundaries. A dropped controller
//! releases its producer, including when an assertion unwinds. Production builds
//! contain neither these gates nor their calls.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::oneshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Point {
    ProjectFactsCollected,
    ProjectCommitAttempted,
    DocumentSourceUpdated,
    ColdDiagnosticsPending,
    ColdDiagnosticsAttempted,
    RequireRefreshCollected,
    RequireRefreshAttempted,
}

struct Gate {
    reached: oneshot::Sender<()>,
    release: oneshot::Receiver<()>,
}

#[derive(Default)]
pub(crate) struct TestSchedule {
    gates: Mutex<HashMap<(Point, PathBuf), Gate>>,
    trace: Mutex<Vec<(Point, PathBuf)>>,
}

pub(crate) struct Pause {
    reached: Option<oneshot::Receiver<()>>,
    release: Option<oneshot::Sender<()>>,
}

impl Pause {
    pub(crate) async fn wait(&mut self) {
        tokio::time::timeout(Duration::from_secs(15), self.reached.take().expect(
            "INVARIANT VIOLATED: a simulation pause was awaited twice. This is a test bug because each boundary has one arrival. Fix: arm another pause for another operation."
        )).await.expect("production lifecycle did not reach the armed simulation boundary")
            .expect("production lifecycle dropped its armed simulation boundary");
    }

    pub(crate) fn release(mut self) {
        self.release
            .take()
            .expect("armed simulation pause must own its release")
            .send(())
            .expect("paused production work must still await release");
    }
}

impl TestSchedule {
    pub(crate) fn arm(&self, point: Point, path: PathBuf) -> Pause {
        let (arrive, reached) = oneshot::channel();
        let (release, wait) = oneshot::channel();
        assert!(self.gates.lock().insert((point, path), Gate { reached: arrive, release: wait }).is_none(),
            "INVARIANT VIOLATED: duplicate simulation gate. This is a test bug because a second gate would hide the first waiter. Fix: await and release the existing boundary before arming it again.");
        Pause {
            reached: Some(reached),
            release: Some(release),
        }
    }

    fn arrive(&self, point: Point, path: &Path) -> Option<Gate> {
        let gate = self.gates.lock().remove(&(point, path.to_path_buf()))?;
        self.trace.lock().push((point, path.to_path_buf()));
        Some(gate)
    }

    /// Called only from the resource governor's synchronous indexing worker.
    pub(crate) fn checkpoint_blocking(&self, point: Point, path: &Path) {
        if let Some(gate) = self.arrive(point, path) {
            if gate.reached.send(()).is_ok() {
                // Controller drop releases cancelled or failed tests as well.
                let _ = gate.release.blocking_recv();
            }
        }
    }

    pub(crate) async fn checkpoint(&self, point: Point, path: &Path) {
        if let Some(gate) = self.arrive(point, path) {
            if gate.reached.send(()).is_ok() {
                let _ = gate.release.await;
            }
        }
    }

    pub(crate) fn trace(&self) -> Vec<(Point, PathBuf)> {
        self.trace.lock().clone()
    }
}
