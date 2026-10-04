//! Unit tests for profiler argument parsing, configuration, and evidence helpers.

use ruby_fast_lsp::server::Server;
use std::path::PathBuf;
use std::time::Instant;
use tower_lsp::lsp_types::Url;

use crate::cli::{parse_args_from, ReferenceProbe};
use crate::evidence::{dataset_fingerprint_sha256, stable_fingerprint_hex, ProcessResourceUsage};
use crate::indexing_summary::millisecond_summary;
use crate::navigation_probes::{observe_first_live_definition, prepare_live_definition_probes};
use crate::workspace_indexing::load_profiler_config;
#[test]
fn diagnostics_files_are_collected_in_command_line_order() {
    let config = parse_args_from([
        "profiler",
        "--workspace",
        "/tmp/project",
        "--diagnostics-file",
        "lib/app.rb",
        "--diagnostics-file",
        "routes.rb",
    ]);

    assert_eq!(
        config.diagnostics_files,
        vec![PathBuf::from("lib/app.rb"), PathBuf::from("routes.rb")]
    );
}

#[test]
fn canonical_configuration_path_is_collected_without_editor_translation() {
    let config = parse_args_from([
        "profiler",
        "--workspace",
        "/tmp/project",
        "--config",
        "/tmp/ruby-fast-lsp.json",
    ]);

    assert_eq!(
        config.config_path,
        Some(PathBuf::from("/tmp/ruby-fast-lsp.json"))
    );
}

#[test]
fn semantic_export_manifest_is_explicit_and_disabled_by_default() {
    assert!(!parse_args_from(["profiler"]).semantic_export_manifest);
    assert!(parse_args_from(["profiler", "--semantic-export-manifest"]).semantic_export_manifest);
}

#[test]
fn diagnostic_manifest_is_explicit_and_disabled_by_default() {
    assert!(!parse_args_from(["profiler"]).diagnostic_manifest);
    assert!(parse_args_from(["profiler", "--diagnostic-manifest"]).diagnostic_manifest);
}

#[test]
fn scheduler_concurrency_is_explicit_and_positive() {
    let config = parse_args_from([
        "profiler",
        "--workspace",
        "/tmp/project",
        "--scheduler-concurrency",
        "1",
    ]);

    assert_eq!(config.scheduler_concurrency, Some(1));
    assert_eq!(
        parse_args_from(["profiler"]).scheduler_concurrency,
        None,
        "without the flag the server derives concurrency from its task budget"
    );
}

#[test]
fn indexing_resource_budget_is_explicit_and_positive() {
    let config = parse_args_from([
        "profiler",
        "--workspace",
        "/tmp/project",
        "--resource-cpu-lanes",
        "3",
        "--resource-task-limit",
        "2",
        "--resource-memory-mib",
        "384",
        "--resource-io-slots",
        "1",
    ]);

    assert_eq!(config.resource_cpu_lanes, Some(3));
    assert_eq!(config.resource_task_limit, Some(2));
    assert_eq!(config.resource_memory_mib, Some(384));
    assert_eq!(config.resource_io_slots, Some(1));
}

#[test]
fn process_resource_delta_is_machine_readable_and_saturating() {
    let start = ProcessResourceUsage {
        user_cpu_us: 10_000,
        system_cpu_us: 5_000,
        peak_rss_bytes: 100,
        input_blocks: 8,
        output_blocks: 4,
    };
    let end = ProcessResourceUsage {
        user_cpu_us: 13_500,
        system_cpu_us: 7_250,
        peak_rss_bytes: 4096,
        input_blocks: 11,
        output_blocks: 3,
    };

    let delta = ProcessResourceUsage::delta(Some(start), Some(end));

    assert_eq!(delta["user_cpu_ms"], 3.5);
    assert_eq!(delta["system_cpu_ms"], 2.25);
    assert_eq!(delta["peak_rss_bytes"], 4096);
    assert_eq!(delta["input_blocks"], 3);
    assert_eq!(delta["output_blocks"], 0);
}

#[test]
fn readiness_summary_uses_nearest_rank_percentiles() {
    let summary = millisecond_summary(&[100, 200, 300, 400, 500]);

    assert_eq!(summary["samples"], 5);
    assert_eq!(summary["min"], 100);
    assert_eq!(summary["p50"], 300);
    assert_eq!(summary["p95"], 500);
    assert_eq!(summary["max"], 500);
}

#[test]
fn semantic_fingerprint_hex_is_fixed_width_and_byte_exact() {
    assert_eq!(
        stable_fingerprint_hex([
            0x00, 0x01, 0x0f, 0x10, 0xab, 0xcd, 0xef, 0xff, 0x12, 0x34, 0x56, 0x78, 0x90, 0xaa,
            0xbb, 0xcc,
        ]),
        "00010f10abcdefff1234567890aabbcc"
    );
}

#[test]
fn dataset_fingerprint_excludes_measurement_results() {
    let first = serde_json::json!([{
        "root": "/workspace/app",
        "runtime": {"implementation": "jruby", "engineVersion": "9.2.21.0"},
        "detected_ruby_version": "2.5",
        "runtime_classpath_fingerprint_sha256": "classpath",
        "project_files": 2,
        "project_source_bytes": 42,
        "project_source_fingerprint_sha256": "source",
        "semantic_result_fingerprint_hex": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        "project_navigation_ready_ms": 100,
        "dependency_navigation_ready_ms": 200,
        "semantic_complete_ms": 300
    }]);
    let second = serde_json::json!([{
        "root": "/workspace/app",
        "runtime": {"implementation": "jruby", "engineVersion": "9.2.21.0"},
        "detected_ruby_version": "2.5",
        "runtime_classpath_fingerprint_sha256": "classpath",
        "project_files": 2,
        "project_source_bytes": 42,
        "project_source_fingerprint_sha256": "source",
        "semantic_result_fingerprint_hex": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        "project_navigation_ready_ms": 900,
        "dependency_navigation_ready_ms": 1000,
        "semantic_complete_ms": 1100
    }]);

    assert_eq!(
        dataset_fingerprint_sha256(first.as_array().unwrap()),
        dataset_fingerprint_sha256(second.as_array().unwrap()),
        "readiness measurements and semantic outputs must not mutate input dataset identity"
    );
}

#[test]
fn loads_and_validates_bounded_canonical_configuration() {
    let fixture = tempfile::tempdir().expect("profiler config fixture must be created");
    let path = fixture.path().join("config.json");
    let project_root = fixture.path().join("workspace/admin");
    let executable = fixture
        .path()
        .join("runtimes/jruby/bin")
        .join(format!("jruby{}", std::env::consts::EXE_SUFFIX));
    let java_home = fixture.path().join("jdks/17");
    std::fs::write(
        &path,
        serde_json::to_vec(&serde_json::json!({
            "runtime": {
                "mode": "auto",
                "projects": [{
                    "root": project_root,
                    "selection": {
                        "implementation": "jruby",
                        "family": "9.2",
                        "engineVersion": "9.2.21.0",
                        "compatibilityVersion": "2.5",
                        "executable": executable,
                        "discoverySource": "rvm",
                        "javaHome": java_home
                    }
                }]
            }
        }))
        .expect("profiler configuration must serialize"),
    )
    .expect("profiler config fixture must be written");

    let config = load_profiler_config(&path);
    assert_eq!(config.runtime.projects.len(), 1);
    assert_eq!(
        PathBuf::from(&config.runtime.projects[0].root),
        project_root
    );
}

#[test]
fn reference_probes_parse_zero_indexed_positions_in_command_line_order() {
    let config = parse_args_from([
        "profiler",
        "--workspace",
        "/tmp/project",
        "--references-at",
        "spec/user_spec.rb:29:10",
        "--references-at",
        "lib/user.rb:4:2",
    ]);

    assert_eq!(
        config.reference_probes,
        vec![
            ReferenceProbe {
                path: PathBuf::from("spec/user_spec.rb"),
                line: 29,
                character: 10,
            },
            ReferenceProbe {
                path: PathBuf::from("lib/user.rb"),
                line: 4,
                character: 2,
            },
        ]
    );
}

#[test]
fn definition_probes_parse_zero_indexed_positions_in_command_line_order() {
    let config = parse_args_from([
        "profiler",
        "--workspace",
        "/tmp/project",
        "--definition-at",
        "lib/runtime.rb:1:12",
        "--definition-at",
        "lib/runtime.rb:14:45",
    ]);

    assert_eq!(
        config.definition_probes,
        vec![
            ReferenceProbe {
                path: PathBuf::from("lib/runtime.rb"),
                line: 1,
                character: 12,
            },
            ReferenceProbe {
                path: PathBuf::from("lib/runtime.rb"),
                line: 14,
                character: 45,
            },
        ]
    );
}

#[tokio::test]
async fn live_definition_probe_requires_a_real_semantic_answer() {
    let fixture = tempfile::tempdir().expect("live probe fixture must be created");
    std::fs::write(
        fixture.path().join("Gemfile"),
        "source 'https://rubygems.org'\n",
    )
    .expect("Gemfile must be written");
    std::fs::write(
        fixture.path().join("live.rb"),
        "class Live\n  def target; end\n  def call; target; end\nend\n",
    )
    .expect("live Ruby source must be written");
    let server = Server::default();
    let canonical_root =
        std::fs::canonicalize(fixture.path()).expect("fixture root must canonicalize");
    server.add_workspace(Url::from_directory_path(&canonical_root).unwrap());
    let prepared = prepare_live_definition_probes(
        &server,
        &canonical_root,
        &[ReferenceProbe {
            path: PathBuf::from("live.rb"),
            line: 2,
            character: 14,
        }],
    )
    .await
    .expect("live definition probe must be prepared");

    let evidence =
        observe_first_live_definition(&server, prepared[0].clone(), Instant::now()).await;

    assert_eq!(evidence["file"], "live.rb");
    assert_eq!(evidence["phase"], "discovered");
    assert_eq!(evidence["target_source_kinds"][0], "Project");
    assert_eq!(evidence["locations"].as_array().unwrap().len(), 1);
}
