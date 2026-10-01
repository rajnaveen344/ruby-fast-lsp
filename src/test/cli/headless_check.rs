//! Headless `check` sessions: project loading, discovery, file scoping, and report contents.

use crate::lsp::check::CheckSession;
use ruby_analysis::core::UnknownReason;

#[tokio::test]
async fn headless_check_reports_engine_diagnostics_without_an_lsp_client() {
    let project = tempfile::tempdir().expect("temporary check project must be created");
    let source = project.path().join("main.rb");
    std::fs::write(
        &source,
        r#"
def greet(name)
  name
end

greet
"#,
    )
    .expect("check fixture must be written");

    let report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the project");

    assert_eq!(report.files_checked, 1);
    invariant!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.as_deref() == Some("wrong-arity")),
        what = "the headless check session did not report the engine's wrong-arity diagnostic",
        why = "CLI and LSP must consume the same semantic facts without starting an LSP client",
        fix = "route check inputs through the shared FileProcessor and AnalysisEngine lifecycle",
    );
}

#[tokio::test]
async fn headless_check_reports_dependency_claims_after_complete_loading() {
    let project = tempfile::tempdir().expect("temporary check project must be created");
    std::fs::write(
        project.path().join("main.rb"),
        r#"
require "external_package"

class User < ExternalPackage::Base
  def save_record
    save!
  end
end
"#,
    )
    .expect("check fixture must be written");

    let report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the project");

    invariant!(
        report.diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.code.as_deref(),
                Some("unresolved-method" | "unresolved-constant" | "unresolved-require")
            )
        }),
        what = "the complete check session withheld every dependency diagnostic",
        why = "a project without a Gemfile has a closed dependency universe",
        fix = "suppress absence claims only when the loader reports incomplete state",
    );
    assert!(report.dependency_loading_complete);
    assert_eq!(report.suppressed_inconclusive_diagnostics, 0);
}

#[tokio::test]
async fn headless_check_uses_the_complete_shared_project_loader() {
    let project = tempfile::tempdir().expect("temporary check project must be created");
    std::fs::write(
        project.path().join("main.rb"),
        "def greet(name)\n  name\nend\n\ngreet\n",
    )
    .expect("check fixture must be written");

    let report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the project");

    invariant!(
        report.dependency_loading_complete,
        what = "a successful check skipped part of the LSP cold-indexing lifecycle",
        why = "the CLI cannot prove absence against a partial universe",
        fix = "route the check session through the shared IndexingCoordinator",
    );
    assert_eq!(report.suppressed_inconclusive_diagnostics, 0);
}

#[tokio::test]
async fn headless_check_reports_file_owned_unknown_reasons_and_solver_work() {
    let project = tempfile::tempdir().expect("temporary telemetry project must be created");
    std::fs::write(
        project.path().join("main.rb"),
        r#"
class Types
  def known
    "text"
  end

  def left
    right
  end

  def right
    left
  end
end
"#,
    )
    .expect("telemetry fixture must be written");

    let report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must retain inference telemetry");

    assert_eq!(report.inference.method_return_outcomes, 3);
    assert_eq!(report.inference.proven_method_returns, 1);
    assert_eq!(report.inference.unknown_method_returns, 2);
    assert_eq!(
        report
            .inference
            .unknown_reasons
            .get(&UnknownReason::UnprovenRecursiveCycle),
        Some(&2),
        "unexpected inference telemetry: {:?}",
        report.inference
    );
    assert_eq!(report.inference.recursive_components, 1);
    assert_eq!(report.inference.recursive_methods, 2);
    assert_eq!(report.inference.solver_iterations, 1);
    assert_eq!(report.inference.solver_bound_hits, 0);
}

#[tokio::test]
async fn explicit_file_check_indexes_the_project_but_reports_only_that_file() {
    let project = tempfile::tempdir().expect("temporary check project must be created");
    let selected = project.path().join("selected.rb");
    std::fs::write(&selected, "def selected(value)\n  value\nend\nselected\n")
        .expect("selected fixture must be written");
    std::fs::write(
        project.path().join("other.rb"),
        "def other(value)\n  value\nend\nother\n",
    )
    .expect("unselected fixture must be written");

    let report = CheckSession::default()
        .check_path(&selected)
        .await
        .expect("explicit file check must analyze its owning project");

    assert_eq!(report.files_checked, 1);
    assert!(
        report
            .diagnostics
            .iter()
            .all(|diagnostic| diagnostic.path == std::path::Path::new("selected.rb")),
        "explicit-file output must not leak sibling project diagnostics: {:?}",
        report.diagnostics
    );
}

#[tokio::test]
async fn umbrella_check_uses_the_same_isolated_project_discovery_as_lsp() {
    let umbrella = tempfile::tempdir().expect("temporary umbrella must be created");
    for project_name in ["alpha", "beta"] {
        let project = umbrella.path().join(project_name);
        std::fs::create_dir_all(&project).expect("nested project must be created");
        std::fs::write(project.join("Gemfile"), "source \"https://rubygems.org\"\n")
            .expect("empty project Gemfile must be written");
        std::fs::write(
            project.join("main.rb"),
            "def greet(name)\n  name\nend\n\ngreet\n",
        )
        .expect("nested project fixture must be written");
    }

    let report = CheckSession::default()
        .check_path(umbrella.path())
        .await
        .expect("umbrella check must analyze each discovered project");

    assert_eq!(report.files_checked, 4);
    assert_eq!(
        report
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.code.as_deref() == Some("wrong-arity"))
            .map(|diagnostic| diagnostic.path.clone())
            .collect::<Vec<_>>(),
        [
            std::path::PathBuf::from("alpha/main.rb"),
            std::path::PathBuf::from("beta/main.rb")
        ]
    );
}
