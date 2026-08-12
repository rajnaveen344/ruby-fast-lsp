use serde_json::Value;
use std::process::Command;

#[test]
fn check_json_uses_stable_output_and_failure_exit_code() {
    let project = tempfile::tempdir().expect("temporary CLI check project must be created");
    std::fs::write(
        project.path().join("main.rb"),
        "def greet(name)\n  name\nend\n\ngreet\n",
    )
    .expect("CLI check fixture must be written");

    let output = Command::new(env!("CARGO_BIN_EXE_ruby-fast-lsp"))
        .args(["check", "--format", "json"])
        .arg(project.path())
        .output()
        .expect("ruby-fast-lsp check must start");

    assert_eq!(
        output.status.code(),
        Some(1),
        "a proven warning must make the check command fail"
    );
    assert!(
        output.stderr.is_empty(),
        "successful check execution must keep stderr empty: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value =
        serde_json::from_slice(&output.stdout).expect("check output must be valid JSON");
    assert_eq!(report["schema_version"], 5);
    assert_eq!(report["files_checked"], 1);
    assert_eq!(report["summary"]["warnings"], 1);
    assert_eq!(report["inference"]["method_return_outcomes"], 1);
    assert_eq!(report["inference"]["unknown_method_returns"], 1);
    assert_eq!(
        report["inference"]["unknown_reasons"]["unresolved_method_return"],
        1
    );
    assert_eq!(report["inferred_types"][0]["subject"], "#greet");
    assert_eq!(report["inferred_types"][0]["kind"], "method_return");
    assert_eq!(report["inferred_types"][0]["outcome"]["status"], "unknown");
    assert_eq!(
        report["inferred_types"][0]["outcome"]["reason"],
        "unresolved_method_return"
    );
    assert_eq!(report["diagnostics"][0]["code"], "wrong-arity");
    assert_eq!(report["diagnostics"][0]["path"], "main.rb");
}
