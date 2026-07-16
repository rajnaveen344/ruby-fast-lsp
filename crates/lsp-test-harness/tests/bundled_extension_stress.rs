use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ruby_fast_lsp::extensions::ExtensionStatusReport;
use ruby_fast_lsp_test_harness::FakeEditor;
use tempfile::TempDir;

const ITERATIONS: usize = 12;
const MAX_GUEST_CALL: Duration = Duration::from_millis(500);
const MAX_STRESS_WALL_TIME: Duration = Duration::from_secs(60);

const PACKAGES: [(&str, &str); 5] = [
    ("rspec-ruby", "target/wasm32-wasip1/release/rspec-ruby.wasm"),
    (
        "rails-ruby",
        "target/wasm32-wasip1/release/ruby_fast_lsp_rails_extension.wasm",
    ),
    (
        "minitest-ruby",
        "target/wasm32-wasip1/release/ruby_fast_lsp_minitest_extension.wasm",
    ),
    (
        "sinatra-rust",
        "target/wasm32-wasip1/release/ruby_fast_lsp_sinatra_extension.wasm",
    ),
    (
        "cucumber-rust",
        "target/wasm32-wasip1/release/ruby_fast_lsp_cucumber_extension.wasm",
    ),
];

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("test harness crate must live under crates/lsp-test-harness")
        .to_path_buf()
}

fn package_root() -> PathBuf {
    std::env::var_os("RUBY_FAST_LSP_BUNDLED_EXTENSION_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| repository_root().join("extensions"))
}

fn package_paths_or_skip() -> Option<Vec<PathBuf>> {
    let root = package_root();
    let missing = PACKAGES
        .iter()
        .filter_map(|(package, artifact)| {
            let path = root.join(package).join(artifact);
            (!path.is_file()).then_some(path)
        })
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return Some(
            PACKAGES
                .iter()
                .map(|(package, _)| root.join(package))
                .collect(),
        );
    }
    assert_ne!(
        std::env::var_os("RUBY_FAST_LSP_REQUIRE_BUNDLED_STRESS").as_deref(),
        Some(std::ffi::OsStr::new("1")),
        "INVARIANT VIOLATED: required bundled-extension stress artifacts are missing: {missing:?}. This is a packaging gate failure because all five official guests must be tested together. Fix: build the guests or point RUBY_FAST_LSP_BUNDLED_EXTENSION_ROOT at an extracted VSIX."
    );
    eprintln!(
        "skipping bundled-extension stress test because artifacts are absent: {missing:?}; use editors/scripts/stress_bundled_extensions.sh for the required gate"
    );
    None
}

fn write_project(root: &Path, name: &str, gems: &[(&str, &str)]) -> PathBuf {
    write_project_with_lock_padding(root, name, gems, 0)
}

fn write_project_with_lock_padding(
    root: &Path,
    name: &str,
    gems: &[(&str, &str)],
    extra_locked_gems: usize,
) -> PathBuf {
    let project = root.join(name);
    std::fs::create_dir_all(&project).expect("stress project directory must be created");
    let gemfile = gems
        .iter()
        .map(|(gem, _)| format!("gem '{gem}'\n"))
        .collect::<String>();
    std::fs::write(
        project.join("Gemfile"),
        format!("source 'https://rubygems.org'\n{gemfile}"),
    )
    .expect("stress Gemfile must be written");
    let mut specs = gems
        .iter()
        .map(|(gem, version)| format!("    {gem} ({version})\n"))
        .collect::<String>();
    for index in 0..extra_locked_gems {
        specs.push_str(&format!("    stress-dependency-{index:03} (1.0.0)\n"));
    }
    std::fs::write(
        project.join("Gemfile.lock"),
        format!("GEM\n  remote: https://rubygems.org/\n  specs:\n{specs}"),
    )
    .expect("stress lockfile must be written");
    project
}

fn filename(project: &Path, relative: &str) -> String {
    project.join(relative).to_string_lossy().to_string()
}

fn assert_healthy(status: &ExtensionStatusReport, expected_project_instances: u64) {
    assert_eq!(status.status, "loaded", "unhealthy extension: {status:?}");
    assert_eq!(
        status.telemetry.project_instances, expected_project_instances,
        "each official guest must instantiate exactly once per applicable project: {status:?}"
    );
    assert_eq!(
        status.telemetry.project_instance_creations, expected_project_instances,
        "each applicable project must construct one reusable Wasm instance: {status:?}"
    );
    assert_eq!(
        status.telemetry.project_instance_failures, 0,
        "cold project activation must succeed: {status:?}"
    );
    assert_eq!(
        status.telemetry.lifecycle_calls, 1,
        "registry activation must be observed once: {status:?}"
    );
    assert!(
        status.telemetry.index_calls >= ITERATIONS as u64,
        "every edit cycle must reach the applicable guest: {status:?}"
    );
    assert_eq!(
        status.telemetry.guest_failures, 0,
        "stress must not produce guest failures: {status:?}"
    );
    assert_eq!(
        status.telemetry.guest_traps, 0,
        "stress must not trap a guest: {status:?}"
    );
    assert_eq!(
        status.telemetry.resource_limit_failures, 0,
        "stress must stay inside resource limits: {status:?}"
    );
    assert_eq!(
        status.telemetry.rejected_outputs, 0,
        "stress output must pass host validation: {status:?}"
    );
    assert_eq!(
        status.telemetry.patch_conflicts, 0,
        "official guests must not conflict: {status:?}"
    );
    assert_eq!(
        status.telemetry.disablements, 0,
        "stress must not disable a guest: {status:?}"
    );
    assert!(
        status.telemetry.emitted_index_patches + status.telemetry.emitted_execution_contexts > 0,
        "each official guest must contribute semantic output: {status:?}"
    );
    assert!(
        status.telemetry.max_guest_time_ns < MAX_GUEST_CALL.as_nanos() as u64,
        "a guest call reached the enforced wall-clock ceiling: {status:?}"
    );
    assert!(status.telemetry.max_guest_time_ns <= status.telemetry.total_guest_time_ns);
}

#[tokio::test]
async fn all_official_guests_survive_repeatable_isolated_project_load() {
    let Some(package_paths) = package_paths_or_skip() else {
        return;
    };
    let started = Instant::now();
    let umbrella = TempDir::new().expect("stress umbrella must be created");
    let rspec = write_project_with_lock_padding(
        umbrella.path(),
        "rspec-service",
        &[("rspec-core", "3.13.5"), ("minitest", "6.0.6")],
        99,
    );
    let rails = write_project(umbrella.path(), "rails-service", &[("rails", "8.1.3")]);
    let minitest = write_project(
        umbrella.path(),
        "minitest-service",
        &[("minitest", "6.0.6")],
    );
    let sinatra = write_project(umbrella.path(), "sinatra-service", &[("sinatra", "4.2.1")]);
    let cucumber = write_project(
        umbrella.path(),
        "cucumber-service",
        &[("cucumber", "11.1.1")],
    );
    let unsupported = write_project(
        umbrella.path(),
        "unsupported-service",
        &[
            ("rspec-core", "4.0.0"),
            ("rails", "9.0.0"),
            ("minitest", "7.0.0"),
            ("sinatra", "5.0.0"),
            ("cucumber", "12.0.0"),
        ],
    );

    let mut editor =
        FakeEditor::with_extension_packages_and_workspace(package_paths, umbrella.path()).await;
    for (file, source) in [
        (
            filename(&rspec, "lib/rspec.rb"),
            "module RSpec\n  def self.describe(subject, &block)\n  end\nend\n",
        ),
        (
            filename(&minitest, "lib/minitest/spec.rb"),
            "module Kernel\n  def describe(subject, &block)\n  end\nend\nclass Object\n  include Kernel\nend\nmodule Minitest\n  class Spec\n    module DSL\n      def describe(subject, &block)\n      end\n    end\n    extend DSL\n  end\nend\n",
        ),
        (
            filename(&sinatra, "lib/sinatra.rb"),
            "module Sinatra\n  module Delegator\n  end\n  class Base\n  end\n  class Application < Base\n  end\nend\n",
        ),
        (
            filename(&cucumber, "lib/cucumber.rb"),
            "module Cucumber\n  module Glue\n    module Dsl\n    end\n  end\nend\nextend Cucumber::Glue::Dsl\nclass Object\nend\n",
        ),
    ] {
        editor.open(&file, source).await;
    }
    let files = [
        (filename(&rspec, "spec/service_spec.rb"), "rspec"),
        (filename(&rails, "app/models/user.rb"), "rails"),
        (filename(&minitest, "test/service_test.rb"), "minitest"),
        (filename(&sinatra, "app.rb"), "sinatra"),
        (
            filename(&cucumber, "features/step_definitions/service_steps.rb"),
            "cucumber",
        ),
    ];
    for (file, framework) in &files {
        let source = source_for(framework, 0);
        editor.open(file, &source).await;
    }
    let unsupported_file = filename(&unsupported, "all_dsl.rb");
    editor
        .open(
            &unsupported_file,
            "RSpec.describe Thing do\nend\ndescribe Thing do\nend\nbelongs_to :account\nget('/') { nil }\nGiven('x') { nil }\n",
        )
        .await;

    for iteration in 1..ITERATIONS {
        for (file, framework) in &files {
            let source = source_for(framework, iteration);
            editor.set(file, &source).await;
        }
    }
    for (file, _) in &files {
        let _ = editor.document_symbols(file).await;
        let _ = editor.code_lens(file).await;
    }

    let statuses = editor
        .extension_status()
        .await
        .into_iter()
        .map(|status| (status.id.clone(), status))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(
        statuses.len(),
        PACKAGES.len(),
        "exactly the five configured official packages must load: {statuses:?}"
    );
    for (id, _) in PACKAGES {
        let status = statuses
            .get(id)
            .unwrap_or_else(|| panic!("missing official extension status for {id}"));
        let expected_project_instances = if id == "minitest-ruby" { 2 } else { 1 };
        assert_healthy(status, expected_project_instances);
        eprintln!(
            "bundled-extension id={id} calls={} index_calls={} max_call_us={} cold_instance_ms={} patches={} contexts={}",
            status.telemetry.guest_calls,
            status.telemetry.index_calls,
            status.telemetry.max_guest_time_ns / 1_000,
            status.telemetry.max_project_instance_time_ns / 1_000_000,
            status.telemetry.emitted_index_patches,
            status.telemetry.emitted_execution_contexts,
        );
    }
    let elapsed = started.elapsed();
    assert!(
        elapsed < MAX_STRESS_WALL_TIME,
        "five-package isolated-project stress took {elapsed:?}, exceeding {MAX_STRESS_WALL_TIME:?}"
    );
    eprintln!(
        "bundled-extension-stress iterations={ITERATIONS} projects=6 extensions=5 elapsed_ms={}",
        elapsed.as_millis()
    );
}

fn source_for(framework: &str, iteration: usize) -> String {
    match framework {
        "rspec" => format!("RSpec.describe Service do\n  let(:value) {{ {iteration} }}\n  it('works') {{ value }}\nend\n"),
        "rails" => format!("class User\n  belongs_to :account\n  before_save :normalize_{iteration}\nend\n"),
        "minitest" => format!("describe Service do\n  let(:value) {{ {iteration} }}\n  it('works') {{ value }}\nend\n"),
        "sinatra" => format!("module Sinatra\n  class Base\n  end\nend\nclass App < Sinatra::Base\n  get('/{iteration}') {{ nil }}\nend\n"),
        "cucumber" => format!("Given('service {iteration}') {{ nil }}\nBefore {{ nil }}\n"),
        other => panic!("INVARIANT VIOLATED: unknown stress framework `{other}`. This is a test bug because every fixture must map to one official extension. Fix: add an explicit source_for match arm."),
    }
}
