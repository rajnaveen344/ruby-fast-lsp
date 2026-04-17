//! bench_references — targeted perf harness for Phase 2 (reference indexing).
//!
//! Loads a named corpus (via `src/perf/corpus.rs`), runs full two-phase
//! indexing, and reports wall-time breakdown — Phase 1 (definitions), Phase 2
//! (references), Phase 3 (diagnostics), and total. Optionally repeats the full
//! pass K times on a fresh index each iteration to measure variance.
//!
//! Usage:
//!   cargo run --release --bin bench_references -- --corpus discourse
//!   cargo run --release --bin bench_references -- --corpus mastodon --repeats 3
//!   cargo run --release --bin bench_references -- --corpus <name> --workers 4
//!   RUBY_FAST_LSP_CORPUS_DIR=/path/to/parent cargo run --release --bin bench_references -- --corpus myproj
//!
//! For CPU profiling, prefer `samply record` around this binary. For
//! lock-contention data, wait for task #5 (tracing) to land.

use anyhow::{anyhow, Context, Result};
use log::{info, LevelFilter};
use ruby_fast_lsp::indexer::coordinator::{IndexingCoordinator, PhaseTimings};
use ruby_fast_lsp::perf::corpus;
use ruby_fast_lsp::server::RubyLanguageServer;
use std::env;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tower_lsp::lsp_types::Url;

struct Config {
    corpus: String,
    repeats: usize,
    workers: Option<usize>,
}

fn parse_args() -> Result<Config> {
    let args: Vec<String> = env::args().collect();
    let mut corpus = None;
    let mut repeats = 1usize;
    let mut workers = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--corpus" | "-c" => {
                corpus = Some(
                    args.get(i + 1)
                        .ok_or_else(|| anyhow!("--corpus requires a value"))?
                        .clone(),
                );
                i += 2;
            }
            "--repeats" | "-r" => {
                repeats = args
                    .get(i + 1)
                    .ok_or_else(|| anyhow!("--repeats requires a value"))?
                    .parse()
                    .context("--repeats must be a positive integer")?;
                i += 2;
            }
            "--workers" | "-w" => {
                workers = Some(
                    args.get(i + 1)
                        .ok_or_else(|| anyhow!("--workers requires a value"))?
                        .parse()
                        .context("--workers must be a positive integer")?,
                );
                i += 2;
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            other => return Err(anyhow!("unknown argument: {}", other)),
        }
    }

    let corpus = corpus.ok_or_else(|| anyhow!("--corpus is required (try: discourse, mastodon)"))?;
    assert!(repeats > 0, "repeats must be > 0");
    Ok(Config {
        corpus,
        repeats,
        workers,
    })
}

fn print_help() {
    println!(
        r#"bench_references — Phase 2 (reference-indexing) perf harness

USAGE:
    bench_references --corpus <NAME> [--repeats N] [--workers N]

OPTIONS:
    -c, --corpus   <NAME>   Corpus name, e.g. discourse, mastodon.
                            Looked up via src/perf/corpus.rs ensure_corpus.
    -r, --repeats  <N>      Full indexing passes on a fresh index each (default 1).
    -w, --workers  <N>      Override rayon thread pool size. Default: num_cpus.

ENV:
    RUBY_FAST_LSP_CORPUS_DIR=/parent
                            Overrides corpus lookup to /parent/<corpus>/.
                            Skip snapshot_corpus.sh for local iteration.

EXAMPLES:
    cargo run --release --bin bench_references -- --corpus discourse
    samply record ./target/release/bench_references --corpus mastodon --workers 1
"#
    );
}

fn main() -> Result<()> {
    env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .format_timestamp_millis()
        .init();

    let cfg = parse_args()?;

    if let Some(n) = cfg.workers {
        rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build_global()
            .context("configuring global rayon pool")?;
        info!("rayon pool pinned to {} threads", n);
    } else {
        info!("rayon pool at default size ({} threads)", rayon::current_num_threads());
    }

    let workspace_path = corpus::ensure_corpus(&cfg.corpus)
        .with_context(|| format!("loading corpus {:?}", cfg.corpus))?;
    info!("corpus {:?} at {}", cfg.corpus, workspace_path.display());
    log_corpus_shape(&workspace_path);

    let rt = Runtime::new()?;
    let mut all_timings = Vec::with_capacity(cfg.repeats);

    for iter in 0..cfg.repeats {
        info!("=== RUN {}/{} ===", iter + 1, cfg.repeats);
        let timings = rt.block_on(run_once(&workspace_path))?;
        all_timings.push(timings);
        print_timings(iter + 1, &timings);
    }

    if cfg.repeats > 1 {
        print_summary(&all_timings);
    }

    Ok(())
}

async fn run_once(workspace_path: &PathBuf) -> Result<PhaseTimings> {
    let workspace_uri = Url::from_file_path(workspace_path)
        .map_err(|_| anyhow!("invalid workspace path: {}", workspace_path.display()))?;

    let server = RubyLanguageServer::default();
    server.add_workspace(workspace_uri.clone());

    let index = server.index_for_uri(&workspace_uri);
    let config = server.config.lock().clone();
    let mut coordinator = IndexingCoordinator::new(workspace_path.clone(), config, index);

    let wall_start = Instant::now();
    coordinator
        .run_complete_indexing(&server)
        .await
        .context("indexing failed")?;
    let wall = wall_start.elapsed();

    let mut t = coordinator.last_timings();
    // `total` on the coordinator excludes the tiny setup before start_time;
    // overwrite with the true wall-clock for consistency with external
    // observation.
    t.total = wall;

    let index = server.index_for_uri(&workspace_uri);
    let idx = index.lock();
    info!(
        "index after pass: {} entries, {} files, {} definitions",
        idx.entries_len(),
        idx.files_count(),
        idx.definitions_len()
    );
    drop(idx);

    Ok(t)
}

fn log_corpus_shape(dir: &PathBuf) {
    let mut count = 0usize;
    let mut bytes = 0u64;
    for entry in walkdir::WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() && entry.path().extension().map_or(false, |e| e == "rb") {
            count += 1;
            if let Ok(m) = entry.metadata() {
                bytes += m.len();
            }
        }
    }
    info!(
        "corpus shape: {} *.rb files, {:.2} MB",
        count,
        bytes as f64 / 1_048_576.0
    );
}

fn print_timings(iter: usize, t: &PhaseTimings) {
    let p1_pct = pct(t.phase1, t.total);
    let p2_pct = pct(t.phase2, t.total);
    let p3_pct = pct(t.phase3, t.total);
    println!(
        "\n  run {iter}: total {:>8.2?} | p1 defs {:>8.2?} ({:>4.1}%) | p2 refs+diag {:>8.2?} ({:>4.1}%) | p3 publish {:>8.2?} ({:>4.1}%)\n",
        t.total, t.phase1, p1_pct, t.phase2, p2_pct, t.phase3, p3_pct
    );
}

fn print_summary(runs: &[PhaseTimings]) {
    println!("\n=== SUMMARY over {} runs ===", runs.len());
    print_stat("p1 defs      ", runs.iter().map(|t| t.phase1));
    print_stat("p2 refs+diag ", runs.iter().map(|t| t.phase2));
    print_stat("p3 publish   ", runs.iter().map(|t| t.phase3));
    print_stat("total        ", runs.iter().map(|t| t.total));
}

fn print_stat(label: &str, iter: impl Iterator<Item = Duration> + Clone) {
    let samples: Vec<Duration> = iter.collect();
    assert!(!samples.is_empty(), "INVARIANT VIOLATED: no samples");
    let sum: Duration = samples.iter().sum();
    let mean = sum / samples.len() as u32;
    let min = samples.iter().min().copied().unwrap();
    let max = samples.iter().max().copied().unwrap();
    println!(
        "  {label}: min {:>8.2?} | mean {:>8.2?} | max {:>8.2?}",
        min, mean, max
    );
}

fn pct(part: Duration, whole: Duration) -> f64 {
    if whole.is_zero() {
        0.0
    } else {
        part.as_secs_f64() / whole.as_secs_f64() * 100.0
    }
}
