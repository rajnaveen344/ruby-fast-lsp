//! Server configuration and the measured workspace indexing phases.

use crate::invariant::ExpectInvariant;
use log::info;
use ruby_analysis::core::TypeSubject;
use ruby_fast_lsp::environment::config::RubyFastLspConfig;
use ruby_fast_lsp::loader::scheduling::{scheduler, status};
use ruby_fast_lsp::lsp::lifecycle::indexing;
use ruby_fast_lsp::server::Server;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::evidence::ProcessResourceUsage;
use crate::indexing_summary::{indexing_summary_json, indexing_timing_json};
use crate::navigation_probes::{observe_first_live_definition, PreparedDefinitionProbe};

pub(crate) fn configure_server(
    server: &mut Server,
    config_path: Option<&PathBuf>,
    extension_path: Option<&PathBuf>,
) -> Duration {
    let mut lsp_config = config_path
        .map(|path| load_profiler_config(path))
        .unwrap_or_default();
    if let Some(path) = extension_path {
        let absolute = std::fs::canonicalize(path).unwrap_or_else(|error| {
            unreachable_invariant!(
                what = "profiler --extension-path must point to an existing path (path: {}, error: {error})",
                why = "VS Code parity profiling requires real bundled stubs",
                fix = "pass the installed extension directory",
                path.display(),
                error = error,
            )
        });
        info!("Using extension path: {}", absolute.display());
        let bundled_extensions = absolute.join("extensions");
        invariant!(
            bundled_extensions.is_dir(),
            what = "profiler --extension-path has no bundled extensions directory (missing: {})",
            why = "parity profiling must load the installed package's guests",
            fix = "pass the extracted or installed VS Code extension root",
            bundled_extensions.display(),
        );
        lsp_config.extension_path = Some(absolute.to_string_lossy().to_string());
        lsp_config
            .extension_dirs
            .push(bundled_extensions.to_string_lossy().to_string());
    }
    server.configure_embedded(lsp_config)
}

pub(crate) fn load_profiler_config(path: &PathBuf) -> RubyFastLspConfig {
    const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
    let absolute = std::fs::canonicalize(path).unwrap_or_else(|error| {
        unreachable_invariant!(
            what = "profiler --config path cannot be canonicalized (path: {}, error: {error})",
            why = "production evidence must record one exact configuration file",
            fix = "pass an existing readable JSON file",
            path.display(),
            error = error,
        )
    });
    let metadata = std::fs::metadata(&absolute).unwrap_or_else(|error| {
        unreachable_invariant!(
            what = "profiler --config metadata is unreadable (path: {}, error: {error})",
            why = "configuration input must be bounded before reading",
            fix = "make the file readable",
            absolute.display(),
            error = error,
        )
    });
    invariant!(
        metadata.is_file() && metadata.len() <= MAX_CONFIG_BYTES,
        what = "profiler --config is not a regular JSON file under 1 MiB (path: {}, bytes: {})",
        why = "profiler configuration must stay bounded",
        fix = "pass a small canonical configuration file",
        absolute.display(),
        metadata.len(),
    );
    let bytes = std::fs::read(&absolute).unwrap_or_else(|error| {
        unreachable_invariant!(
            what = "profiler --config cannot be read (path: {}, error: {error})",
            why = "the selected evidence configuration must be reproducible",
            fix = "make the file readable",
            absolute.display(),
            error = error,
        )
    });
    let config: RubyFastLspConfig = serde_json::from_slice(&bytes).unwrap_or_else(|error| {
        unreachable_invariant!(
            what =
                "profiler --config is not canonical Ruby Fast LSP JSON (path: {}, error: {error})",
            why = "measurements cannot silently use defaults after malformed input",
            fix = "correct the JSON configuration",
            absolute.display(),
            error = error,
        )
    });
    config
        .validate_runtime_configuration()
        .unwrap_or_else(|error| {
            unreachable_invariant!(
                what = "profiler --config runtime selection is invalid (path: {}, error: {error})",
                why = "evidence must use a defensible runtime identity",
                fix = "correct the runtime/JRuby project configuration",
                absolute.display(),
                error = error,
            )
        });
    config
}

pub(crate) async fn run_full_indexing(
    server: &Server,
    definition_probes: &[PreparedDefinitionProbe],
) -> Duration {
    let start = Instant::now();
    run_registered_workspace_indexing(server, definition_probes).await;
    info!("Full indexing completed in {:?}", start.elapsed());
    start.elapsed()
}

pub(crate) async fn run_indexing_only(
    server: &Server,
    definition_probes: &[PreparedDefinitionProbe],
) -> Duration {
    let start = Instant::now();
    run_registered_workspace_indexing(server, definition_probes).await;
    info!("Indexing completed in {:?}", start.elapsed());
    start.elapsed()
}

async fn run_registered_workspace_indexing(
    server: &Server,
    definition_probes: &[PreparedDefinitionProbe],
) {
    let workspaces = server.list_workspaces();
    invariant!(
        !workspaces.is_empty(),
        what = "profiler reached indexing without registered projects",
        why = "workspace containers must be expanded before indexing",
        fix = "call add_workspace_folder before run_registered_workspace_indexing",
    );
    let wall_started = Instant::now();
    let resources_started = ProcessResourceUsage::capture();
    let mut live_probe_tasks = tokio::task::JoinSet::new();
    for probe in definition_probes.iter().cloned() {
        let server = server.clone();
        live_probe_tasks.spawn(async move {
            observe_first_live_definition(&server, probe, wall_started).await
        });
    }
    let scheduled = workspaces
        .into_iter()
        .map(|workspace| {
            let run = workspace.begin_indexing_run();
            let admission = server.register_indexing_run(
                workspace.root_path.clone(),
                scheduler::IndexingPriority::Background,
                &run,
            );
            (workspace, run, admission)
        })
        .collect::<Vec<_>>();
    let mut tasks = tokio::task::JoinSet::new();
    for (workspace, run, admission) in scheduled {
        let server = server.clone();
        tasks.spawn(async move {
            let uri = workspace.root_uri.clone();
            let Some(_permit) = admission.wait().await else {
                return (
                    uri,
                    Err(anyhow::anyhow!(
                        "profiler indexing generation {} was cancelled before admission",
                        run.generation()
                    )),
                );
            };
            let _ = workspace.indexing_status.transition(
                run.generation(),
                status::IndexingPhase::ResolvingRuntime,
                None,
                None,
            );
            let result = indexing::init_workspace_for_run(&server, uri.clone(), run.clone()).await;
            match &result {
                Ok(_) => {
                    let _ = workspace.indexing_status.transition(
                        run.generation(),
                        status::IndexingPhase::Ready,
                        None,
                        None,
                    );
                }
                Err(error) => {
                    let _ = workspace
                        .indexing_status
                        .fail(run.generation(), error.to_string());
                }
            }
            (uri, result)
        });
    }
    let mut completed = Vec::new();
    while let Some(joined) = tasks.join_next().await {
        let (uri, result) = joined.expect_invariant(
            "profiler workspace indexing task panicked",
            "a production measurement cannot omit one isolated project",
            "inspect the indexing task panic and keep every discovered project in the gate",
        );
        match result {
            Ok(timings) => completed.push((uri, timings)),
            Err(error) => {
                unreachable_invariant!(
                    what = "profiler project `{uri}` indexing failed (error: {error})",
                    why = "performance measurements require every isolated project to complete",
                    fix = "repair the corpus or indexing failure before benchmarking",
                    uri = uri,
                    error = error,
                );
            }
        }
    }
    completed.sort_by(|(left, _), (right, _)| left.as_str().cmp(right.as_str()));
    let mut live_definition_evidence = Vec::new();
    while let Some(joined) = live_probe_tasks.join_next().await {
        live_definition_evidence.push(joined.expect_invariant(
            "live navigation probe task panicked",
            "staged readiness evidence cannot silently omit a configured probe",
            "inspect the probe panic and keep every configured position in the profiler result",
        ));
    }
    live_definition_evidence.sort_by(|left, right| {
        left["file"]
            .as_str()
            .cmp(&right["file"].as_str())
            .then_with(|| left["line"].as_u64().cmp(&right["line"].as_u64()))
            .then_with(|| left["character"].as_u64().cmp(&right["character"].as_u64()))
    });
    for (uri, timings) in &completed {
        println!(
            "{}",
            serde_json::json!({
                "indexing_timing": indexing_timing_json(uri, *timings)
            })
        );
    }

    let wall = wall_started.elapsed();
    let resources_finished = ProcessResourceUsage::capture();
    println!(
        "{}",
        serde_json::json!({
            "indexing_summary": indexing_summary_json(
                server,
                &completed,
                wall,
                resources_started,
                resources_finished,
                &live_definition_evidence,
            )
        })
    );
}

pub(crate) async fn run_type_inference_only(server: &Server) {
    let start = Instant::now();
    let inferred_count = server
        .list_workspaces()
        .into_iter()
        .map(|workspace| {
            workspace.handle().view(|view| {
                view.all_type_facts()
                    .into_iter()
                    .filter(|fact| matches!(fact.subject, TypeSubject::MethodReturn(_)))
                    .count()
            })
        })
        .sum::<usize>();
    info!("Type inference completed in {:?}", start.elapsed());
    info!(
        "Analysis engine has {} method return type facts",
        inferred_count
    );
}
