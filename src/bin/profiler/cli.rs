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
    pub(crate) scheduler_concurrency: usize,
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
        scheduler_concurrency: 2,
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
                assert!(
                    i + 1 < args.len(),
                    "INVARIANT VIOLATED: profiler --config has no path. This is a bug because a \
                     configured profiling run requires an explicit JSON file. Fix: pass \
                     --config /path/to/ruby-fast-lsp.json."
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
                        panic!(
                            "INVARIANT VIOLATED: --hold-seconds must be an unsigned integer. \
                             This is a bug because profiler hold duration must be parseable seconds. \
                             Fix: pass a numeric value like --hold-seconds 30. Error: {error}"
                        )
                    });
                    i += 1;
                }
            }
            "--benchmark-iterations" => {
                if i + 1 < args.len() {
                    let iterations = args[i + 1].parse().unwrap_or_else(|error| {
                        panic!(
                            "INVARIANT VIOLATED: --benchmark-iterations must be a positive integer. This is a bug because p95 measurement requires a fixed nonzero sample count. Fix: pass a numeric value like --benchmark-iterations 100. Error: {error}"
                        )
                    });
                    assert!(
                        iterations > 0,
                        "INVARIANT VIOLATED: --benchmark-iterations is zero. This is a bug because p95 measurement requires observations. Fix: pass a positive iteration count."
                    );
                    config.benchmark_iterations = Some(iterations);
                    i += 1;
                }
            }
            "--scheduler-concurrency" => {
                assert!(
                    i + 1 < args.len(),
                    "INVARIANT VIOLATED: profiler --scheduler-concurrency has no value. This is a bug because scheduling evidence requires an explicit positive worker limit. Fix: pass --scheduler-concurrency 1."
                );
                let concurrency = args[i + 1].parse().unwrap_or_else(|error| {
                    panic!(
                        "INVARIANT VIOLATED: --scheduler-concurrency must be a positive integer. This is a bug because profiler scheduling must be reproducible. Fix: pass a numeric value such as 1 or 2. Error: {error}"
                    )
                });
                assert!(
                    concurrency > 0,
                    "INVARIANT VIOLATED: --scheduler-concurrency is zero. This is a bug because no project could be admitted. Fix: pass a positive worker count."
                );
                config.scheduler_concurrency = concurrency;
                i += 1;
            }
            "--resource-cpu-lanes" => {
                assert!(
                    i + 1 < args.len(),
                    "INVARIANT VIOLATED: profiler --resource-cpu-lanes has no value. This is a bug because resource evidence requires an explicit positive lane limit. Fix: pass --resource-cpu-lanes 2."
                );
                let lanes = args[i + 1].parse().unwrap_or_else(|error| {
                    panic!(
                        "INVARIANT VIOLATED: --resource-cpu-lanes must be a positive integer. This is a bug because profiler resource evidence must be reproducible. Fix: pass a numeric value such as 2 or 6. Error: {error}"
                    )
                });
                assert!(
                    lanes > 0,
                    "INVARIANT VIOLATED: --resource-cpu-lanes is zero. This is a bug because no indexing CPU work could progress. Fix: pass a positive lane count."
                );
                config.resource_cpu_lanes = Some(lanes);
                i += 1;
            }
            "--resource-task-limit" => {
                assert!(
                    i + 1 < args.len(),
                    "INVARIANT VIOLATED: profiler --resource-task-limit has no value. This is a bug because resource evidence requires an explicit positive admission limit. Fix: pass --resource-task-limit 2."
                );
                let tasks = args[i + 1].parse().unwrap_or_else(|error| {
                    panic!(
                        "INVARIANT VIOLATED: --resource-task-limit must be a positive integer. This is a bug because profiler resource evidence must be reproducible. Fix: pass a numeric value such as 1 or 2. Error: {error}"
                    )
                });
                assert!(
                    tasks > 0,
                    "INVARIANT VIOLATED: --resource-task-limit is zero. This is a bug because no indexing task could enter the worker pool. Fix: pass a positive task limit."
                );
                config.resource_task_limit = Some(tasks);
                i += 1;
            }
            "--resource-memory-mib" => {
                assert!(
                    i + 1 < args.len(),
                    "INVARIANT VIOLATED: profiler --resource-memory-mib has no value. This is a bug because memory evidence requires an explicit positive admission limit. Fix: pass --resource-memory-mib 512."
                );
                let memory_mib = args[i + 1].parse::<usize>().unwrap_or_else(|error| {
                    panic!(
                        "INVARIANT VIOLATED: --resource-memory-mib must be a positive integer. This is a bug because profiler resource evidence must be reproducible. Fix: pass a numeric value such as 256 or 512. Error: {error}"
                    )
                });
                assert!(
                    memory_mib > 0,
                    "INVARIANT VIOLATED: --resource-memory-mib is zero. This is a bug because no indexing work could reserve temporary memory. Fix: pass a positive MiB limit."
                );
                config.resource_memory_mib = Some(memory_mib);
                i += 1;
            }
            "--resource-io-slots" => {
                assert!(
                    i + 1 < args.len(),
                    "INVARIANT VIOLATED: profiler --resource-io-slots has no value. This is a bug because I/O evidence requires an explicit positive admission limit. Fix: pass --resource-io-slots 2."
                );
                let io_slots = args[i + 1].parse::<usize>().unwrap_or_else(|error| {
                    panic!(
                        "INVARIANT VIOLATED: --resource-io-slots must be a positive integer. This is a bug because profiler resource evidence must be reproducible. Fix: pass a numeric value such as 1 or 2. Error: {error}"
                    )
                });
                assert!(
                    io_slots > 0,
                    "INVARIANT VIOLATED: --resource-io-slots is zero. This is a bug because source discovery could never enter the I/O budget. Fix: pass a positive slot count."
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
                assert!(
                    i + 1 < args.len(),
                    "INVARIANT VIOLATED: --diagnostics-file has no path. This is a bug because diagnostic sampling requires an explicit workspace-relative file. Fix: pass --diagnostics-file path/to/file.rb."
                );
                config.diagnostics_files.push(PathBuf::from(&args[i + 1]));
                i += 1;
            }
            "--references-at" => {
                assert!(
                    i + 1 < args.len(),
                    "INVARIANT VIOLATED: --references-at has no path and position. This is a bug because reference sampling requires path:line:character. Fix: pass --references-at spec/example_spec.rb:29:10."
                );
                config
                    .reference_probes
                    .push(parse_reference_probe(&args[i + 1]));
                i += 1;
            }
            "--definition-at" => {
                assert!(
                    i + 1 < args.len(),
                    "INVARIANT VIOLATED: --definition-at has no path and position. This is a bug because definition sampling requires path:line:character. Fix: pass --definition-at lib/example.rb:4:10."
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
        panic!("INVARIANT VIOLATED: {flag} `{value}` has no character component. This is a bug because profiler query positions must be explicit. Fix: use path:line:character with zero-indexed LSP coordinates.")
    });
    let (path, line) = path_and_line.rsplit_once(':').unwrap_or_else(|| {
        panic!("INVARIANT VIOLATED: {flag} `{value}` has no line component. This is a bug because profiler query positions must be explicit. Fix: use path:line:character with zero-indexed LSP coordinates.")
    });
    let line = line.parse().unwrap_or_else(|error| {
        panic!("INVARIANT VIOLATED: {flag} line `{line}` is invalid. This is a bug because LSP lines are unsigned integers. Fix: pass a zero-indexed numeric line. Error: {error}")
    });
    let character = character.parse().unwrap_or_else(|error| {
        panic!("INVARIANT VIOLATED: {flag} character `{character}` is invalid. This is a bug because LSP characters are unsigned integers. Fix: pass a zero-indexed numeric character. Error: {error}")
    });
    let path = PathBuf::from(path);
    assert!(
        path.is_relative(),
        "INVARIANT VIOLATED: {flag} path `{}` is absolute. This is a bug because profiler probes must remain inside the selected workspace. Fix: pass a workspace-relative path.",
        path.display()
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
    cargo build --release --bin profiler
    samply record ./target/release/profiler /path/to/ruby/project

    # Profile specific phase
    samply record ./target/release/profiler --phase infer /path/to/project

    # Memory profiling replaces the default jemalloc allocator with DHAT.
    cargo build --release --bin profiler --no-default-features --features memory-profiling
    ./target/release/profiler --memory /path/to/project

    # Check deterministic built-in production budgets
    ./target/release/profiler --benchmark-iterations 100 --check-budgets

    # Prove warm-process dependency reuse with projects admitted sequentially
    ./target/release/profiler --workspace /path/to/umbrella --scheduler-concurrency 1
"#
    );
}
