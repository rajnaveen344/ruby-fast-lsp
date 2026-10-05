use serde_json::Value;
use std::process::Command;

#[test]
fn check_without_installed_ruby_keeps_project_diagnostics() {
    let project = tempfile::tempdir().expect("runtime-free project must be created");
    std::fs::write(
        project.path().join("Gemfile"),
        "source 'https://example.test'\n",
    )
    .expect("project Gemfile must be written");
    std::fs::write(project.path().join(".ruby-version"), "ruby-0.0.0\n")
        .expect("unavailable runtime marker must be written");
    std::fs::write(
        project.path().join("main.rb"),
        "def greet(name)\n  name\nend\n\ngreet\n",
    )
    .expect("project Ruby source must be written");

    let output = Command::new(env!("CARGO_BIN_EXE_ruby-fast-lsp"))
        .args(["check", "--format", "json"])
        .arg(project.path())
        .env("PATH", project.path())
        .output()
        .expect("runtime-free CLI check must start");
    assert_eq!(
        output.status.code(),
        Some(1),
        "missing Ruby must preserve the diagnostic exit code; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout)
        .expect("missing Ruby must still produce a complete check report");
    assert_eq!(report["files_checked"], 2);
    assert_eq!(report["summary"]["warnings"], 1);
    assert_eq!(report["diagnostics"][0]["code"], "wrong-arity");
    assert_eq!(report["diagnostics"][0]["path"], "main.rb");
}

#[test]
fn check_json_uses_stable_output_and_failure_exit_code() {
    let project = tempfile::tempdir().expect("temporary CLI check project must be created");
    std::fs::write(
        project.path().join("main.rb"),
        "def greet(name)\n  name\nend\n\ngreet\n",
    )
    .expect("CLI check fixture must be written");

    // Windows executables start with a 1 MiB main-thread stack. Exercise the
    // same bound on Unix so its larger default cannot mask CLI stack growth.
    #[cfg(unix)]
    let mut command = {
        let mut command = Command::new("sh");
        command.args([
            "-c",
            "ulimit -s 1024 && exec \"$@\"",
            "check-with-small-stack",
        ]);
        command.arg(env!("CARGO_BIN_EXE_ruby-fast-lsp"));
        command
    };
    #[cfg(not(unix))]
    let mut command = Command::new(env!("CARGO_BIN_EXE_ruby-fast-lsp"));
    command
        .args(["check", "--format", "json"])
        .arg(project.path());
    let output = command.output().expect("ruby-fast-lsp check must start");

    assert_eq!(
        output.status.code(),
        Some(1),
        "a proven warning must make the check command fail; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
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

fn lsp_frame(message: &Value) -> Vec<u8> {
    let body = message.to_string();
    format!("Content-Length: {}\r\n\r\n{body}", body.len()).into_bytes()
}

fn read_lsp_frame(output: &mut impl std::io::BufRead) -> Value {
    let mut length = None;
    loop {
        let mut header = String::new();
        assert_ne!(
            output
                .read_line(&mut header)
                .expect("server output must be readable"),
            0,
            "stdio server closed its output mid-session"
        );
        if header == "\r\n" {
            break;
        }
        if let Some(value) = header.strip_prefix("Content-Length: ") {
            length = value.trim().parse::<usize>().ok();
        }
    }
    let mut body = vec![0; length.expect("server frame must carry Content-Length")];
    output
        .read_exact(&mut body)
        .expect("server frame body must be readable");
    serde_json::from_slice(&body).expect("server frame must be JSON")
}

#[test]
fn stdio_server_exits_when_client_input_closes_with_a_request_unanswered() {
    use std::io::{BufReader, Write};
    use std::process::Stdio;
    use std::time::{Duration, Instant};

    let workspace = tempfile::tempdir().expect("stdio workspace must be created");
    let mut server = Command::new(env!("CARGO_BIN_EXE_ruby-fast-lsp"))
        .arg("--stdio")
        .current_dir(workspace.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("stdio server must start");
    let mut input = server.stdin.take().expect("server input must be piped");
    let mut output = BufReader::new(server.stdout.take().expect("server output must be piped"));
    let dynamic = serde_json::json!({ "dynamicRegistration": true });
    input
        .write_all(&lsp_frame(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "capabilities": {
                    "textDocument": { "typeHierarchy": dynamic, "callHierarchy": dynamic }
                }
            }
        })))
        .expect("initialize must reach the stdio server");
    while read_lsp_frame(&mut output)["id"] != 1 {}
    input
        .write_all(&lsp_frame(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        })))
        .expect("initialized must reach the stdio server");
    // Leave the server waiting on a client request this client never answers.
    while read_lsp_frame(&mut output)["method"] != "client/registerCapability" {}
    drop(input);

    // A hang is the failure; the deadline only bounds how long the test waits for it.
    let deadline = Instant::now() + Duration::from_secs(60);
    let status = loop {
        if let Some(status) = server.try_wait().expect("server status must be readable") {
            break Some(status);
        }
        if Instant::now() >= deadline {
            break None;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    if status.is_none() {
        let _ = server.kill();
        let _ = server.wait();
    }
    assert!(
        status.is_some(),
        "a stdio server must exit once its client input closes, even while a request to that client is unanswered"
    );
}
