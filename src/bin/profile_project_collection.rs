//! Synchronous project-file fact collection microbench/profile harness.
//!
//! Mirrors the indexer project batch path (parse + FactCollector visit + deferred
//! replace) without Tokio scheduling or Rayon parallelism, so samply captures a
//! clean CPU flame of FileProcessor work.
//!
//! Usage:
//!   cargo build --release --bin profile_project_collection
//!   ./target/release/profile_project_collection /path/to/project
//!
//! For symbolized samply:
//!   CARGO_PROFILE_RELEASE_DEBUG=1 CARGO_PROFILE_RELEASE_STRIP=none \
//!     cargo build --release --bin profile_project_collection
//!   RAYON_NUM_THREADS=1 samply record -r 100 -s --unstable-presymbolicate \
//!     -o /tmp/project-collection-profile.json.gz \
//!     ./target/release/profile_project_collection /path/to/project

use std::env;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use log::info;
use ruby_analysis::core::SourceKind;
use ruby_analysis::engine::AnalysisEngine;
use ruby_fast_lsp::config::IndexingConfig;
use ruby_fast_lsp::indexer::file_processor::{FileProcessor, ProjectFileCollectionTiming};
use ruby_fast_lsp::server::RubyLanguageServer;
use ruby_fast_lsp::utils::file_ops::collect_project_files;
use tower_lsp::lsp_types::Url;

fn main() -> Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!(
            "Usage: {} <project_root> [--limit N]",
            args.first().map(String::as_str).unwrap_or("profile_project_collection")
        );
        std::process::exit(1);
    }

    let project_root = std::fs::canonicalize(&args[1])?;
    let mut limit: Option<usize> = None;
    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--limit" => {
                let value = args.get(i + 1).ok_or_else(|| anyhow!("--limit requires N"))?;
                limit = Some(value.parse().map_err(|error| {
                    anyhow!("invalid --limit {value}: {error}")
                })?);
                i += 2;
            }
            other => {
                return Err(anyhow!("unknown argument: {other}"));
            }
        }
    }

    let indexing = IndexingConfig::default();
    let mut files = collect_project_files(&project_root, &indexing)?;
    if let Some(limit) = limit {
        files.truncate(limit);
    }
    info!(
        "Sync project collection: root={} files={}",
        project_root.display(),
        files.len()
    );

    let project_uri = Url::from_directory_path(&project_root).map_err(|()| {
        anyhow!(
            "project root is not a valid file URI: {}",
            project_root.display()
        )
    })?;
    let server = RubyLanguageServer::default();
    server.add_workspace(project_uri.clone());
    let analysis_engine = server.analysis_engine_for_uri(&project_uri);
    let processor = FileProcessor::new();

    let read_started = Instant::now();
    let mut inputs = Vec::with_capacity(files.len());
    for path in &files {
        let content = std::fs::read_to_string(path)
            .map_err(|error| anyhow!("failed to read {}: {error}", path.display()))?;
        inputs.push((path.clone(), content));
    }
    let read_elapsed = read_started.elapsed();

    let registration_started = Instant::now();
    {
        let mut engine = analysis_engine.write();
        for (path, content) in &inputs {
            engine.register_file_borrowed(path.clone(), content, SourceKind::Project);
        }
    }
    let registration_elapsed = registration_started.elapsed();

    let known_namespaces = Arc::new({
        let engine = analysis_engine.read();
        ruby_analysis::engine::AnalysisQuery::new(&engine).known_namespace_fqns()
    });

    let mut timing = ProjectFileCollectionTiming::default();
    timing.registration += registration_elapsed;
    timing.total += registration_elapsed;

    let collect_started = Instant::now();
    for (path, content) in inputs {
        let uri = Url::from_file_path(&path).map_err(|()| {
            anyhow!(
                "project source path is not a valid file URI: {}",
                path.display()
            )
        })?;
        let collected = processor
            .collect_project_file_facts_and_jruby_navigation_plan_as_deferred_resolution(
                &uri,
                content,
                analysis_engine.clone(),
                known_namespaces.clone(),
            )?;
        let replacement_started = Instant::now();
        processor.replace_collected_project_file_facts_as_deferred_resolution(
            &path,
            &analysis_engine,
            collected.file_facts,
        );
        let replacement_elapsed = replacement_started.elapsed();

        timing.total += collected.timing.total + replacement_elapsed;
        timing.registration += collected.timing.registration;
        timing.parse += collected.timing.parse;
        timing.jruby_plan += collected.timing.jruby_plan;
        timing.semantic_seed += collected.timing.semantic_seed;
        timing.visitor += collected.timing.visitor;
        timing.assembly += collected.timing.assembly;
        timing.replacement += collected.timing.replacement + replacement_elapsed;
    }
    let wall = collect_started.elapsed();

    print_summary(files.len(), read_elapsed, wall, &timing, &analysis_engine);
    Ok(())
}

fn print_summary(
    file_count: usize,
    read_elapsed: Duration,
    wall: Duration,
    timing: &ProjectFileCollectionTiming,
    analysis_engine: &parking_lot::RwLock<AnalysisEngine>,
) {
    let method_count = {
        let engine = analysis_engine.read();
        engine.all_method_facts().len()
    };
    info!(
        "[PERF][sync project collection] files={} wall={:?} read={:?} \
         cpu_total={:?} registration={:?} parse={:?} jruby_plan={:?} \
         semantic_seed={:?} visitor={:?} assembly={:?} replacement={:?} methods={}",
        file_count,
        wall,
        read_elapsed,
        timing.total,
        timing.registration,
        timing.parse,
        timing.jruby_plan,
        timing.semantic_seed,
        timing.visitor,
        timing.assembly,
        timing.replacement,
        method_count
    );

    let ranked = [
        ("visitor", timing.visitor),
        ("parse", timing.parse),
        ("replacement", timing.replacement),
        ("semantic_seed", timing.semantic_seed),
        ("assembly", timing.assembly),
        ("jruby_plan", timing.jruby_plan),
        ("registration", timing.registration),
    ];
    let mut ranked = ranked.to_vec();
    ranked.sort_by(|left, right| right.1.cmp(&left.1));
    let denom = timing
        .parse
        + timing.visitor
        + timing.replacement
        + timing.semantic_seed
        + timing.assembly
        + timing.jruby_plan
        + timing.registration;
    info!("[PERF][sync project collection ranks] denom={denom:?}");
    for (name, duration) in ranked {
        let pct = if denom.is_zero() {
            0.0
        } else {
            (duration.as_secs_f64() / denom.as_secs_f64()) * 100.0
        };
        info!("  {name:>14} {duration:?} ({pct:.1}%)");
    }
}
