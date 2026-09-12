//! Pause the actual cold coordinator at fact commit or diagnostic publication
//! and deliver a real editor operation first. Gates determine order; timeouts
//! only bound hangs.

use crate::capabilities::indexing::init_workspace_for_run;
use crate::indexer::test_schedule::Point;
use crate::test::harness::FakeEditor;
use std::time::Duration;
use tower_lsp::lsp_types::{Location, Position, Range, Url};

const OLD: &str = "class Vessel\n  def old_value = 1\nend\nVessel.new.old_value\n";
const NEW: &str = "class Vessel\n  def new_value = 2\nend\nVessel.new.new_value\n";

#[derive(Clone, Copy)]
enum Completion {
    Commit,
    CancelAndRestart,
}

async fn edit_between_real_collection_and_commit(
    open_before_start: bool,
    completion: Completion,
    cpu_lanes: usize,
) {
    let fixture = tempfile::tempdir().expect("neutral coordinator fixture must be writable");
    let root = fixture.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let path = root.join("vessel.rb");
    std::fs::write(&path, OLD).unwrap();
    let uri = Url::from_file_path(&path).unwrap();
    let root_uri = Url::from_directory_path(&root).unwrap();
    let filename = path.to_str().unwrap().trim_start_matches('/').to_string();
    let mut editor = FakeEditor::with_cache_root(fixture.path().join("cache")).await;
    // The concurrent schedule needs one lane for each producer. Set its
    // resource contract explicitly instead of inheriting the host CPU count.
    editor.set_indexing_resource_policy(crate::indexing_resources::IndexingResourcePolicy::new(
        cpu_lanes, cpu_lanes,
    ));
    let server = editor.server().clone();
    server.set_discovered_runtimes_for_tests(Vec::new());
    let workspace = server.add_workspace(root_uri.clone());
    if open_before_start {
        editor.open(&filename, OLD).await;
    }

    let mut collected = server
        .indexing
        .schedule
        .arm(Point::ProjectFactsCollected, path.clone());
    let run = workspace.begin_indexing_run();
    let cold_server = server.clone();
    let cold_root = root_uri.clone();
    let indexing =
        tokio::spawn(async move { init_workspace_for_run(&cold_server, cold_root, run).await });
    collected.wait().await;
    let old_snapshot = workspace
        .analysis_engine
        .read()
        .source_snapshot_for_path(&path)
        .expect("cold collection must register its input before reaching the commit boundary");
    let mut updated = server
        .indexing
        .schedule
        .arm(Point::DocumentSourceUpdated, path.clone());
    let operation_file = filename.clone();
    let editing = tokio::spawn(async move {
        if open_before_start {
            editor.set(&operation_file, NEW).await;
        } else {
            editor.open(&operation_file, NEW).await;
        }
        editor
    });
    updated.wait().await;
    assert_ne!(
        workspace
            .analysis_engine
            .read()
            .source_snapshot_for_path(&path),
        Some(old_snapshot),
        "the real document handler must invalidate the collected source before cold commit"
    );
    if matches!(completion, Completion::CancelAndRestart) {
        server.cancel_all_indexing();
    }
    updated.release();
    let mut attempted = server
        .indexing
        .schedule
        .arm(Point::ProjectCommitAttempted, path.clone());
    let editor = if cpu_lanes == 1 {
        // A one-lane governor correctly queues the edit while cold work owns
        // that lane. Check admission, then let the real worker release it.
        tokio::time::timeout(Duration::from_secs(30), async {
            while server.indexing_resource_snapshot().queued_tasks == 0 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("the edit must queue behind the occupied indexing lane");
        let resources = server.indexing_resource_snapshot();
        assert_eq!(resources.active_cpu_lanes, 1);
        assert_eq!(resources.active_tasks, 1);
        assert!(
            !editing.is_finished(),
            "the edit must respect resource admission"
        );
        collected.release();
        attempted.wait().await;
        attempted.release();
        tokio::time::timeout(Duration::from_secs(30), editing)
            .await
            .expect("the queued edit must finish after cold work releases its lane")
            .expect("real document handler must not panic")
    } else {
        // Finish the edit while cold work is still paused, then observe before
        // a coordinator tail could repair a stale commit. The source-guard
        // mutant must continue to fail this exact intermediate assertion.
        let editor = tokio::time::timeout(Duration::from_secs(30), editing)
            .await
            .expect("interactive editing must finish independently of paused cold work")
            .expect("real document handler must not panic");
        collected.release();
        attempted.wait().await;
        assert_eq!(
            editor.goto_def_at(&filename, 3, 14).await,
            vec![Location::new(
                uri.clone(),
                Range::new(Position::new(1, 2), Position::new(1, 19))
            )],
            "a delayed cold commit must preserve the exact new method definition"
        );
        attempted.release();
        editor
    };
    let completed = tokio::time::timeout(Duration::from_secs(30), indexing)
        .await
        .expect("cold indexing must finish after the coordinator is released")
        .expect("coordinator task must not panic");
    match completion {
        Completion::Commit => {
            completed.expect("uncancelled cold indexing must succeed");
        }
        Completion::CancelAndRestart => {
            let error = completed.expect_err("the cancelled coordinator must not report success");
            assert!(
                format!("{error:#}").contains("cancelled"),
                "the intended cancellation must explain termination: {error:#}"
            );
            tokio::time::timeout(
                Duration::from_secs(30),
                init_workspace_for_run(&server, root_uri, workspace.begin_indexing_run()),
            )
            .await
            .expect("a replacement indexing generation must make progress")
            .expect("a replacement indexing generation must recover after cancellation");
        }
    }

    assert_eq!(editor.content(&filename), NEW);
    assert_eq!(
        editor.goto_def_at(&filename, 3, 14).await,
        vec![Location::new(
            uri.clone(),
            Range::new(Position::new(1, 2), Position::new(1, 19))
        )],
        "a delayed cold commit must preserve the exact new method definition"
    );
    let file_id = workspace.analysis_engine.read().file_id(&path).unwrap();
    assert!(
        workspace
            .analysis_engine
            .read()
            .file_content_matches(file_id, NEW),
        "cold indexing must never restore disk source over the editor buffer"
    );
    let mut fresh = FakeEditor::new().await;
    fresh.add_workspace(root.to_str().unwrap().trim_start_matches('/'));
    fresh.open(&filename, NEW).await;
    assert_eq!(
        editor.document_symbols(&filename).await,
        fresh.document_symbols(&filename).await,
        "the coordinator must retain exactly the current symbols, without the removed method"
    );
    assert_eq!(
        editor.diagnostics(&filename).await,
        fresh.diagnostics(&filename).await,
        "the coordinator must publish current diagnostic output after the interleaving"
    );
    assert_eq!(
        server.indexing.schedule.trace(),
        vec![
            (Point::ProjectFactsCollected, path.clone()),
            (Point::DocumentSourceUpdated, path.clone()),
            (Point::ProjectCommitAttempted, path),
        ],
        "all checkpoints must be reached through the intended production order"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_coordinator_rejects_cold_facts_after_a_startup_edit() {
    edit_between_real_collection_and_commit(true, Completion::Commit, 2).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_coordinator_rejects_disk_facts_after_an_unsaved_open() {
    edit_between_real_collection_and_commit(false, Completion::Commit, 2).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_coordinator_recovers_after_cancelling_collected_work() {
    edit_between_real_collection_and_commit(true, Completion::CancelAndRestart, 2).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_coordinator_and_edit_share_a_single_resource_lane() {
    edit_between_real_collection_and_commit(true, Completion::Commit, 1).await;
}

#[tokio::test]
async fn dropped_schedule_controller_releases_its_worker() {
    let schedule = std::sync::Arc::new(crate::indexer::test_schedule::TestSchedule::default());
    let path = std::path::PathBuf::from("neutral.rb");
    let mut pause = schedule.arm(Point::ProjectFactsCollected, path.clone());
    let worker_schedule = schedule.clone();
    let worker = tokio::task::spawn_blocking(move || {
        worker_schedule.checkpoint_blocking(Point::ProjectFactsCollected, &path);
    });
    pause.wait().await;
    drop(pause);
    tokio::time::timeout(Duration::from_secs(5), worker)
        .await
        .expect("a failed or cancelled simulation must release its blocked worker")
        .expect("controller drop must not panic the worker");
}

#[derive(Clone, Copy)]
enum PublicationChange {
    Edit,
    DefinitionOpen,
    CloseAndReopen,
    CancelAndRestart,
}

async fn delayed_coordinator_diagnostics_after_change(change: PublicationChange) {
    let fixture = tempfile::tempdir().expect("neutral publication fixture must be writable");
    let root = fixture.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let path = root.join("caller.rb");
    let broken = "_value = MISSING\n";
    std::fs::write(&path, broken).unwrap();
    let root_uri = Url::from_directory_path(&root).unwrap();
    let filename = path.to_str().unwrap().trim_start_matches('/').to_string();
    let mut editor = FakeEditor::with_cache_root(fixture.path().join("cache")).await;
    let server = editor.server().clone();
    server.set_discovered_runtimes_for_tests(Vec::new());
    let workspace = server.add_workspace(root_uri.clone());
    editor.open(&filename, broken).await;
    let initial = editor.diagnostics(&filename).await;
    assert_eq!(
        initial.len(),
        1,
        "the initial missing constant must produce one error"
    );
    assert_eq!(
        initial[0].code,
        Some(tower_lsp::lsp_types::NumberOrString::String(
            "unresolved-constant".into()
        ))
    );

    let mut pending = server
        .indexing
        .schedule
        .arm(Point::ColdDiagnosticsPending, path.clone());
    let run = workspace.begin_indexing_run();
    let cold_server = server.clone();
    let cold_root = root_uri.clone();
    let indexing =
        tokio::spawn(async move { init_workspace_for_run(&cold_server, cold_root, run).await });
    pending.wait().await;
    match change {
        PublicationChange::Edit | PublicationChange::CancelAndRestart => {
            editor.set(&filename, "_value = 1\n").await;
        }
        PublicationChange::DefinitionOpen => {
            let definition = root.join("definition.rb");
            editor
                .open(
                    definition.to_str().unwrap().trim_start_matches('/'),
                    "MISSING = 1\n",
                )
                .await;
            assert_eq!(editor.content(&filename), broken);
        }
        PublicationChange::CloseAndReopen => {
            editor.close(&filename).await;
            editor.open(&filename, "_value = 1\n").await;
        }
    }
    assert_eq!(
        editor.diagnostics(&filename).await,
        Vec::new(),
        "the real editor operation must clear the error before delayed publication"
    );
    if matches!(change, PublicationChange::CancelAndRestart) {
        let replacement = workspace.begin_indexing_run();
        pending.release();
        let error = tokio::time::timeout(Duration::from_secs(30), indexing)
            .await
            .expect("superseded coordinator must terminate")
            .expect("superseded coordinator must not panic")
            .expect_err("superseded indexing must not report success");
        assert!(
            format!("{error:#}").contains("cancelled")
                || format!("{error:#}").contains("superseded"),
            "the indexing generation must explain its termination: {error:#}"
        );
        assert_eq!(
            editor.diagnostics(&filename).await,
            Vec::new(),
            "a superseded coordinator must not publish before its cancellation checkpoint"
        );
        tokio::time::timeout(
            Duration::from_secs(30),
            init_workspace_for_run(&server, root_uri, replacement),
        )
        .await
        .expect("replacement indexing must make progress")
        .expect("replacement indexing must succeed");
        assert_eq!(editor.diagnostics(&filename).await, Vec::new());
        return;
    }
    let mut attempted = server
        .indexing
        .schedule
        .arm(Point::ColdDiagnosticsAttempted, path.clone());
    pending.release();
    attempted.wait().await;
    assert_eq!(
        editor.diagnostics(&filename).await,
        Vec::new(),
        "a delayed coordinator publication must not restore an error for an older buffer"
    );
    attempted.release();
    tokio::time::timeout(Duration::from_secs(30), indexing)
        .await
        .expect("coordinator must complete after publication is released")
        .expect("coordinator task must not panic")
        .expect("uncancelled coordinator must finish");
    assert_eq!(editor.diagnostics(&filename).await, Vec::new());
    assert_eq!(
        server.indexing.schedule.trace(),
        vec![
            (Point::ColdDiagnosticsPending, path.clone()),
            (Point::ColdDiagnosticsAttempted, path),
        ]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delayed_coordinator_diagnostics_cannot_restore_an_error_after_an_edit() {
    delayed_coordinator_diagnostics_after_change(PublicationChange::Edit).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delayed_coordinator_diagnostics_use_current_dependency_facts() {
    delayed_coordinator_diagnostics_after_change(PublicationChange::DefinitionOpen).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delayed_coordinator_diagnostics_cannot_overwrite_a_reopened_document() {
    delayed_coordinator_diagnostics_after_change(PublicationChange::CloseAndReopen).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delayed_coordinator_diagnostics_stop_when_indexing_is_superseded() {
    delayed_coordinator_diagnostics_after_change(PublicationChange::CancelAndRestart).await;
}

#[tokio::test]
async fn cold_coordinator_diagnostics_preserve_current_syntax_warnings() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("project");
    std::fs::create_dir(&root).unwrap();
    let path = root.join("caller.rb");
    let source = "_value = MISSING\nreturn\n1\n";
    std::fs::write(&path, source).unwrap();
    let root_uri = Url::from_directory_path(&root).unwrap();
    let filename = path.to_str().unwrap().trim_start_matches('/');
    let mut editor = FakeEditor::with_cache_root(fixture.path().join("cache")).await;
    let server = editor.server().clone();
    server.set_discovered_runtimes_for_tests(Vec::new());
    let workspace = server.add_workspace(root_uri.clone());
    editor.open(filename, source).await;
    let initial = editor.diagnostics(filename).await;
    assert!(initial.iter().any(|diagnostic| diagnostic.code
        == Some(tower_lsp::lsp_types::NumberOrString::String(
            "unreachable-code".into()
        ))));
    assert!(initial.iter().any(|diagnostic| diagnostic.code
        == Some(tower_lsp::lsp_types::NumberOrString::String(
            "unresolved-constant".into()
        ))));

    init_workspace_for_run(&server, root_uri, workspace.begin_indexing_run())
        .await
        .expect("cold indexing must succeed");
    assert_eq!(
        editor.diagnostics(filename).await,
        initial,
        "cold publication must retain both syntax warnings and semantic errors"
    );
}
