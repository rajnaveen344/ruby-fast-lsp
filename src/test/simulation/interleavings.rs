//! Controlled collection/commit schedules over the production snapshot guard.
//! No timing sleeps decide which result wins: channels release each producer.

use crate::indexer::file_processor::FileProcessor;
use crate::test::harness::FakeEditor;
use parking_lot::RwLock;
use ruby_analysis::engine::{AnalysisEngine, FileFacts, SourceFileSnapshot};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::oneshot;

const FILE: &str = "release_alpha/service.rb";
const OLD: &str = "class Service\n  def old_value; 1; end\n  def call; old_value; end\nend\n";
const NEW: &str =
    "class Service\n  def new_value; \"ready\"; end\n  def call; new_value; end\nend\n";

struct Collected {
    path: PathBuf,
    snapshot: SourceFileSnapshot,
    facts: FileFacts,
}

fn collect(editor: &FakeEditor, filename: &str, source: &str) -> Collected {
    let uri = crate::test::harness::fixture_uri(format!("/{filename}"));
    let path = uri.to_file_path().unwrap();
    let engine = editor.server().analysis_engine_for_uri(&uri);
    let snapshot = engine.read().source_snapshot_for_path(&path).unwrap();
    let known = Arc::new(engine.read().query().known_namespace_fqns());
    let facts = FileProcessor::new()
        .collect_project_file_facts_and_jruby_navigation_plan_as_deferred_resolution(
            &uri,
            source.to_string(),
            engine.clone(),
            known,
        )
        .expect("simulation background collection must succeed")
        .file_facts;
    assert_eq!(
        engine.read().source_snapshot_for_path(&path),
        Some(snapshot),
        "collection must not replace the live source identity"
    );
    Collected {
        path,
        snapshot,
        facts,
    }
}

fn pending_commit(
    collected: Collected,
    engine: Arc<RwLock<AnalysisEngine>>,
) -> (oneshot::Sender<()>, tokio::task::JoinHandle<Option<bool>>) {
    let (release, wait) = oneshot::channel();
    let task = tokio::spawn(async move {
        if wait.await.is_err() {
            return None; // Cancellation before publication is an explicit outcome.
        }
        let accepted = FileProcessor::new()
            .replace_collected_project_file_facts_if_source_snapshot_as_deferred_resolution(
                &collected.path,
                &engine,
                collected.snapshot,
                collected.facts,
            );
        if accepted {
            engine.write().resolve();
        }
        Some(accepted)
    });
    (release, task)
}

async fn release_commit(
    pending: (oneshot::Sender<()>, tokio::task::JoinHandle<Option<bool>>),
) -> bool {
    pending
        .0
        .send(())
        .expect("scheduled producer must be awaiting its release");
    tokio::time::timeout(std::time::Duration::from_secs(10), pending.1)
        .await
        .expect("scheduled producer must not deadlock")
        .expect("scheduled producer must not panic")
        .expect("released producer must not report cancellation")
}

async fn assert_current(editor: &FakeEditor, filename: &str, source: &str) {
    let mut fresh = FakeEditor::new().await;
    fresh.add_workspace("release_alpha");
    fresh.add_workspace("release_beta");
    fresh.open(filename, source).await;
    assert_eq!(
        editor.document_symbols(filename).await,
        fresh.document_symbols(filename).await,
        "controlled background completion must retain exactly the current declarations"
    );
    assert_eq!(
        editor.goto_def_at(filename, 2, 15).await,
        fresh.goto_def_at(filename, 2, 15).await,
        "navigation after scheduled completion must equal fresh analysis"
    );
    assert!(
        !editor.goto_def_at(filename, 2, 15).await.is_empty(),
        "independent fixture expectation requires the current method call to resolve"
    );
    assert_eq!(
        editor.hover_at(filename, 2, 15).await,
        fresh.hover_at(filename, 2, 15).await,
        "hover after scheduled completion must equal fresh analysis"
    );
}

#[tokio::test]
async fn controlled_background_schedules_preserve_edits_isolation_and_recovery() {
    let seeds: Vec<u64> = match std::env::var("SIM_INTERLEAVING_SEED") {
        Ok(seed) => vec![seed
            .parse()
            .expect("SIM_INTERLEAVING_SEED must be an integer from 0 through 4")],
        Err(std::env::VarError::NotPresent) => (0..5).collect(),
        Err(std::env::VarError::NotUnicode(_)) => panic!("SIM_INTERLEAVING_SEED must be Unicode"),
    };
    for seed in seeds {
        assert!(
            seed <= 4,
            "SIM_INTERLEAVING_SEED must select a defined schedule, 0 through 4"
        );
        let artifact = tempfile::Builder::new()
            .prefix(&format!("ruby-lsp-schedule-{seed}-"))
            .tempdir()
            .expect("schedule artifact directory must be writable")
            .into_path();
        let steps = match seed {
            0 => "collect old; publish old; edit new; compare fresh",
            1 => "collect old; edit new; collect new; publish new; reject old; compare fresh",
            2 => "collect old; edit new; cancel old; collect new; publish new; compare fresh",
            3 => "collect old; restart server; open new; reject old into replacement engine; compare fresh",
            4 => "collect alpha; open beta; reject alpha into beta engine; compare both projects",
            5..=u64::MAX => unreachable!("INVARIANT VIOLATED: unchecked schedule seed reached dispatch. This is a test bug because schedules are bounded to 0 through 4. Fix: validate the seed before execution."),
        };
        std::fs::write(artifact.join("schedule.json"), serde_json::to_vec_pretty(&serde_json::json!({
            "schema_version": 1, "seed": seed, "steps": steps, "file": FILE,
            "old_source": OLD, "new_source": NEW,
            "replay": format!("SIM_INTERLEAVING_SEED={seed} cargo test controlled_background_schedules_preserve_edits_isolation_and_recovery -- --nocapture")
        })).unwrap()).expect("replay artifact must be written before execution");
        eprintln!(
            "controlled schedule {seed}: {steps}; artifact {}",
            artifact.display()
        );
        let mut editor = FakeEditor::new().await;
        editor.add_workspace("release_alpha");
        editor.add_workspace("release_beta");
        editor.open(FILE, OLD).await;
        let old = collect(&editor, FILE, OLD);
        let uri = crate::test::harness::fixture_uri(format!("/{FILE}"));
        let engine = editor.server().analysis_engine_for_uri(&uri);
        match seed {
            0 => {
                assert!(release_commit(pending_commit(old, engine)).await);
                editor.set(FILE, NEW).await;
                assert_current(&editor, FILE, NEW).await;
            }
            1 => {
                let pending = pending_commit(old, engine.clone());
                editor.set(FILE, NEW).await;
                let current = collect(&editor, FILE, NEW);
                assert!(release_commit(pending_commit(current, engine)).await);
                assert!(!release_commit(pending).await, "older collection must be rejected after newer publication");
                assert_current(&editor, FILE, NEW).await;
            }
            2 => {
                let (release, task) = pending_commit(old, engine.clone());
                editor.set(FILE, NEW).await;
                drop(release);
                assert_eq!(task.await.unwrap(), None);
                let current = collect(&editor, FILE, NEW);
                assert!(release_commit(pending_commit(current, engine)).await,
                    "a cancelled producer must not prevent the next valid publication");
                assert_current(&editor, FILE, NEW).await;
            }
            3 => {
                drop(editor);
                let mut replacement = FakeEditor::new().await;
                replacement.add_workspace("release_alpha");
                replacement.open(FILE, NEW).await;
                let target = replacement.server().analysis_engine_for_uri(&uri);
                assert!(!release_commit(pending_commit(old, target)).await,
                    "source snapshots must not survive engine replacement");
                assert_current(&replacement, FILE, NEW).await;
            }
            4 => {
                let other_file = "release_beta/service.rb";
                editor.open(other_file, NEW).await;
                let other_uri = crate::test::harness::fixture_uri(format!("/{other_file}"));
                let other_engine = editor.server().analysis_engine_for_uri(&other_uri);
                assert!(!release_commit(pending_commit(old, other_engine)).await,
                    "project-specific facts must never cross isolated engines");
                assert_current(&editor, FILE, OLD).await;
                assert_current(&editor, other_file, NEW).await;
            }
            5..=u64::MAX => unreachable!("INVARIANT VIOLATED: unchecked schedule seed reached dispatch. This is a test bug because schedules are bounded to 0 through 4. Fix: validate the seed before execution."),
        }
        std::fs::write(artifact.join("result.json"), "{\"passed\":true}\n")
            .expect("schedule completion evidence must be written");
    }
}
