//! Command-line parsing and usage text for the profiler.

use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Phase {
    All,
    Index,
    Infer,
}

pub(crate) struct Config {
    pub(crate) workspace: Option<PathBuf>,
    pub(crate) config_path: Option<PathBuf>,
    pub(crate) extension_path: Option<PathBuf>,
    pub(crate) memory_profiling: bool,
    pub(crate) phase: Phase,
    pub(crate) hold_seconds: u64,
    pub(crate) benchmark_iterations: Option<usize>,
    pub(crate) scheduler_concurrency: Option<usize>,
    pub(crate) resource_cpu_lanes: Option<usize>,
    pub(crate) resource_task_limit: Option<usize>,
    pub(crate) resource_memory_mib: Option<usize>,
    pub(crate) resource_io_slots: Option<usize>,
    pub(crate) check_budgets: bool,
    pub(crate) diagnostics_files: Vec<PathBuf>,
    pub(crate) definition_probes: Vec<ReferenceProbe>,
    pub(crate) reference_probes: Vec<ReferenceProbe>,
    pub(crate) semantic_export_manifest: bool,
    pub(crate) diagnostic_manifest: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ReferenceProbe {
    pub(crate) path: PathBuf,
    pub(crate) line: u32,
    pub(crate) character: u32,
}

pub(crate) fn parse_args() -> Config {
    parse_args_from(env::args())
}

pub(crate) fn parse_args_from<I, S>(args: I) -> Config
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let args = args.into_iter().map(Into::into).collect::<Vec<_>>();
    let mut config = Config {
        workspace: None,
        config_path: None,
        extension_path: None,
        memory_profiling: false,
        phase: Phase::All,
        hold_seconds: 0,
        benchmark_iterations: None,
        scheduler_concurrency: None,
        resource_cpu_lanes: None,
        resource_task_limit: None,
        resource_memory_mib: None,
        resource_io_slots: None,
        check_budgets: false,
        diagnostics_files: Vec::new(),
        definition_probes: Vec::new(),
        reference_probes: Vec::new(),
        semantic_export_manifest: false,
        diagnostic_manifest: false,
    };

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--workspace" | "-w" => {
                if i + 1 < args.len() {
                    config.workspace = Some(PathBuf::from(&args[i + 1]));
                    i += 1;
                }
            }
            "--memory" | "-m" => {
                config.memory_profiling = true;
            }
            "--config" => {
                invariant!(
                    i + 1 < args.len(),
                    what = "profiler --config has no path",
                    why = "a configured profiling run requires an explicit JSON file",
                    fix = "pass --config /path/to/ruby-fast-lsp.json",
                );
                config.config_path = Some(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--extension-path" => {
                if i + 1 < args.len() {
                    config.extension_path = Some(PathBuf::from(&args[i + 1]));
                    i += 1;
                }
            }
            "--phase" | "-p" => {
                if i + 1 < args.len() {
                    config.phase = match args[i + 1].as_str() {
                        "index" => Phase::Index,
                        "infer" => Phase::Infer,
                        "all" => Phase::All,
                        _ => {
                            eprintln!("Unknown phase: {}. Using 'all'", args[i + 1]);
                            Phase::All
                        }
                    };
                    i += 1;
                }
            }
            "--hold-seconds" => {
                if i + 1 < args.len() {
                    config.hold_seconds = args[i + 1].parse().unwrap_or_else(|error| {
                        unreachable_invariant!(
                            what = "--hold-seconds must be an unsigned integer (error: {error})",
                            why = "profiler hold duration must be parseable seconds",
                            fix = "pass a numeric value like --hold-seconds 30",
                            error = error,
                        )
                    });
                    i += 1;
                }
            }
            "--benchmark-iterations" => {
                if i + 1 < args.len() {
                    let iterations = args[i + 1].parse().unwrap_or_else(|error| {
                        unreachable_invariant!(
                            what = "--benchmark-iterations must be a positive integer (error: {error})",
                            why = "p95 measurement requires a fixed nonzero sample count",
                            fix = "pass a numeric value like --benchmark-iterations 100",
                            error = error,
                        )
                    });
                    invariant!(
                        iterations > 0,
                        what = "--benchmark-iterations is zero",
                        why = "p95 measurement requires observations",
                        fix = "pass a positive iteration count",
                    );
                    config.benchmark_iterations = Some(iterations);
                    i += 1;
                }
            }
            "--scheduler-concurrency" => {
                invariant!(
                    i + 1 < args.len(),
                    what = "profiler --scheduler-concurrency has no value",
                    why = "scheduling evidence requires an explicit positive worker limit",
                    fix = "pass --scheduler-concurrency 1",
                );
                let concurrency = args[i + 1].parse().unwrap_or_else(|error| {
                    unreachable_invariant!(
                        what =
                            "--scheduler-concurrency must be a positive integer (error: {error})",
                        why = "profiler scheduling must be reproducible",
                        fix = "pass a numeric value such as 1 or 2",
                        error = error,
                    )
                });
                invariant!(
                    concurrency > 0,
                    what = "--scheduler-concurrency is zero",
                    why = "no project could be admitted",
                    fix = "pass a positive worker count",
                );
                config.scheduler_concurrency = Some(concurrency);
                i += 1;
            }
            "--resource-cpu-lanes" => {
                invariant!(
                    i + 1 < args.len(),
                    what = "profiler --resource-cpu-lanes has no value",
                    why = "resource evidence requires an explicit positive lane limit",
                    fix = "pass --resource-cpu-lanes 2",
                );
                let lanes = args[i + 1].parse().unwrap_or_else(|error| {
                    unreachable_invariant!(
                        what = "--resource-cpu-lanes must be a positive integer (error: {error})",
                        why = "profiler resource evidence must be reproducible",
                        fix = "pass a numeric value such as 2 or 6",
                        error = error,
                    )
                });
                invariant!(
                    lanes > 0,
                    what = "--resource-cpu-lanes is zero",
                    why = "no indexing CPU work could progress",
                    fix = "pass a positive lane count",
                );
                config.resource_cpu_lanes = Some(lanes);
                i += 1;
            }
            "--resource-task-limit" => {
                invariant!(
                    i + 1 < args.len(),
                    what = "profiler --resource-task-limit has no value",
                    why = "resource evidence requires an explicit positive admission limit",
                    fix = "pass --resource-task-limit 2",
                );
                let tasks = args[i + 1].parse().unwrap_or_else(|error| {
                    unreachable_invariant!(
                        what = "--resource-task-limit must be a positive integer (error: {error})",
                        why = "profiler resource evidence must be reproducible",
                        fix = "pass a numeric value such as 1 or 2",
                        error = error,
                    )
                });
                invariant!(
                    tasks > 0,
                    what = "--resource-task-limit is zero",
                    why = "no indexing task could enter the worker pool",
                    fix = "pass a positive task limit",
                );
                config.resource_task_limit = Some(tasks);
                i += 1;
            }
            "--resource-memory-mib" => {
                invariant!(
                    i + 1 < args.len(),
                    what = "profiler --resource-memory-mib has no value",
                    why = "memory evidence requires an explicit positive admission limit",
                    fix = "pass --resource-memory-mib 512",
                );
                let memory_mib = args[i + 1].parse::<usize>().unwrap_or_else(|error| {
                    unreachable_invariant!(
                        what = "--resource-memory-mib must be a positive integer (error: {error})",
                        why = "profiler resource evidence must be reproducible",
                        fix = "pass a numeric value such as 256 or 512",
                        error = error,
                    )
                });
                invariant!(
                    memory_mib > 0,
                    what = "--resource-memory-mib is zero",
                    why = "no indexing work could reserve temporary memory",
                    fix = "pass a positive MiB limit",
                );
                config.resource_memory_mib = Some(memory_mib);
                i += 1;
            }
            "--resource-io-slots" => {
                invariant!(
                    i + 1 < args.len(),
                    what = "profiler --resource-io-slots has no value",
                    why = "I/O evidence requires an explicit positive admission limit",
                    fix = "pass --resource-io-slots 2",
                );
                let io_slots = args[i + 1].parse::<usize>().unwrap_or_else(|error| {
                    unreachable_invariant!(
                        what = "--resource-io-slots must be a positive integer (error: {error})",
                        why = "profiler resource evidence must be reproducible",
                        fix = "pass a numeric value such as 1 or 2",
                        error = error,
                    )
                });
                invariant!(
                    io_slots > 0,
                    what = "--resource-io-slots is zero",
                    why = "source discovery could never enter the I/O budget",
                    fix = "pass a positive slot count",
                );
                config.resource_io_slots = Some(io_slots);
                i += 1;
            }
            "--check-budgets" => {
                config.check_budgets = true;
            }
            "--semantic-export-manifest" => {
                config.semantic_export_manifest = true;
            }
            "--diagnostic-manifest" => {
                config.diagnostic_manifest = true;
            }
            "--diagnostics-file" => {
                invariant!(
                    i + 1 < args.len(),
                    what = "--diagnostics-file has no path",
                    why = "diagnostic sampling requires an explicit workspace-relative file",
                    fix = "pass --diagnostics-file path/to/file.rb",
                );
                config.diagnostics_files.push(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--references-at" => {
                invariant!(
                    i + 1 < args.len(),
                    what = "--references-at has no path and position",
                    why = "reference sampling requires path:line:character",
                    fix = "pass --references-at spec/example_spec.rb:29:10",
                );
                config
                    .reference_probes
                    .push(parse_reference_probe(&args[i + 1]));
                i += 1;
            }
            "--definition-at" => {
                invariant!(
                    i + 1 < args.len(),
                    what = "--definition-at has no path and position",
                    why = "definition sampling requires path:line:character",
                    fix = "pass --definition-at lib/example.rb:4:10",
                );
                config
                    .definition_probes
                    .push(parse_position_probe("--definition-at", &args[i + 1]));
                i += 1;
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            _ => {
                // Treat as workspace path if no flag
                if !args[i].starts_with('-') {
                    config.workspace = Some(PathBuf::from(&args[i]));
                }
            }
        }
        i += 1;
    }

    config
}

fn parse_reference_probe(value: &str) -> ReferenceProbe {
    parse_position_probe("--references-at", value)
}

fn parse_position_probe(flag: &str, value: &str) -> ReferenceProbe {
    let (path_and_line, character) = value.rsplit_once(':').unwrap_or_else(|| {
        unreachable_invariant!(
            what = "{flag} `{value}` has no character component",
            why = "profiler query positions must be explicit",
            fix = "use path:line:character with zero-indexed LSP coordinates",
            flag = flag,
            value = value,
        )
    });
    let (path, line) = path_and_line.rsplit_once(':').unwrap_or_else(|| {
        unreachable_invariant!(
            what = "{flag} `{value}` has no line component",
            why = "profiler query positions must be explicit",
            fix = "use path:line:character with zero-indexed LSP coordinates",
            flag = flag,
            value = value,
        )
    });
    let line = line.parse().unwrap_or_else(|error| {
        unreachable_invariant!(
            what = "{flag} line `{line}` is invalid (error: {error})",
            why = "LSP lines are unsigned integers",
            fix = "pass a zero-indexed numeric line",
            flag = flag,
            line = line,
            error = error,
        )
    });
    let character = character.parse().unwrap_or_else(|error| {
        unreachable_invariant!(
            what = "{flag} character `{character}` is invalid (error: {error})",
            why = "LSP characters are unsigned integers",
            fix = "pass a zero-indexed numeric character",
            flag = flag,
            character = character,
            error = error,
        )
    });
    let path = PathBuf::from(path);
    invariant!(
        path.is_relative(),
        what = "{flag} path `{}` is absolute",
        why = "profiler probes must remain inside the selected workspace",
        fix = "pass a workspace-relative path",
        path.display(),
        flag = flag,
    );
    ReferenceProbe {
        path,
        line,
        character,
    }
}

fn print_help() {
    println!(
        r#"Ruby Fast LSP Profiler

USAGE:
    profiler [OPTIONS] [WORKSPACE]

OPTIONS:
    -w, --workspace <PATH>   Path to Ruby workspace (default: built-in sample project)
    -m, --memory             Enable dhat memory profiling (outputs dhat-heap.json)
    -p, --phase <PHASE>      Profile specific phase: index, infer, all (default: all)
    --config <PATH>          Load canonical Ruby Fast LSP JSON configuration
    --extension-path <PATH>  VS Code extension path for bundled stubs
    --hold-seconds <N>       Keep process alive after profiling for external memory tools
    --benchmark-iterations <N>
                             Measure edit and query p95 latency after indexing
    --scheduler-concurrency <N>
                             Override concurrently indexed projects (default: the task limit)
    --resource-cpu-lanes <N>
                             Override the server-owned indexing CPU pool width for evidence
    --resource-task-limit <N>
                             Override admitted top-level indexing tasks for evidence
    --resource-memory-mib <N>
                             Override transient-memory admission for evidence
    --resource-io-slots <N>
                             Override concurrent indexing I/O admission for evidence
    --check-budgets          Exit unsuccessfully when a production budget is exceeded
    --diagnostics-file <PATH>
                             Open a workspace-relative file through didOpen and print diagnostics as JSON; repeatable
    --definition-at <PATH:LINE:CHARACTER>
                             Open a workspace-relative file and print definitions as JSON; repeatable, zero-indexed
    --references-at <PATH:LINE:CHARACTER>
                             Open a workspace-relative file and print resolved references as JSON; repeatable, zero-indexed
    --semantic-export-manifest
                             Print stable per-project-file semantic export fingerprints as JSON
    --diagnostic-manifest
                             Print stable per-project resolved diagnostic facts as JSON
    -h, --help               Show this help message

EXAMPLES:
    # Profile with samply (CPU)
    cargo build --release -p devtools --bin profiler
    samply record ./target/release/profiler /path/to/ruby/project

    # Profile specific phase
    samply record ./target/release/profiler --phase infer /path/to/project

    # Memory profiling replaces the default jemalloc allocator with DHAT.
    cargo build --release -p devtools --bin profiler --no-default-features --features memory-profiling
    ./target/release/profiler --memory /path/to/project

    # Check deterministic built-in production budgets
    ./target/release/profiler --benchmark-iterations 100 --check-budgets

    # Prove warm-process dependency reuse with projects admitted sequentially
    ./target/release/profiler --workspace /path/to/umbrella --scheduler-concurrency 1
"#
    );
}
