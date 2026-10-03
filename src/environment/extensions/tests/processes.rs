use super::*;

#[test]
fn process_manifest_requires_process_exec_permission() {
    let temp_dir = TempDir::new().expect("test temp dir must be created");
    let package_dir = temp_dir.path().join("process");
    fs::create_dir(&package_dir).expect("test package dir must be created");
    fs::write(package_dir.join("extension.wasm"), b"not real wasm")
        .expect("test wasm marker must be written");
    fs::write(
        package_dir.join("extension.toml"),
        r#"
id = "process"
name = "Process"
version = "0.1.0"
abi_version = 1
server_version = ">=0.2.3, <0.4.0"
runtime = "mruby-wasm"
wasm = "extension.wasm"
capabilities = ["process"]
permissions = []

[process]
commands = ["standardrb"]
"#,
    )
    .expect("test manifest must be written");

    let config = ExtensionLoadConfig {
        package_paths: vec![ConfiguredExtensionPath {
            path: package_dir,
            source: ExtensionPathSource::InitializationOptions,
        }],
        directory_paths: Vec::new(),
        project_package_paths: Vec::new(),
        settings: BTreeMap::new(),
    };

    let extensions = load_wasm_extensions(&config);
    invariant!(
        extensions.is_empty(),
        what = "process command manifest loaded without process.exec",
        why = "external process permissions must be explicit",
        fix = "require process.exec when [process].commands is present",
    );
}

#[test]
fn extension_process_request_requires_trust_permission_and_allowlist() {
    let root = crate::test::harness::fixture_path("/workspace");
    let request = ruby_fast_lsp_extension_api::ProcessRequest {
        request_id: "routes".to_string(),
        command: "bundle".to_string(),
        arguments: vec![
            "exec".to_string(),
            "rails".to_string(),
            "routes".to_string(),
        ],
        stdin: None,
        workspace_root: None,
        timeout_ms: None,
    };

    for (trusted, permissions, commands, expected) in [
        (
            false,
            vec!["process.exec"],
            vec!["bundle"],
            "trusted workspace",
        ),
        (true, Vec::new(), vec!["bundle"], "process.exec"),
        (true, vec!["process.exec"], vec!["ruby"], "allowlisted"),
    ] {
        let err = validate_extension_process_request(
            "process-test",
            trusted,
            &permissions
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>(),
            &commands.into_iter().map(str::to_string).collect::<Vec<_>>(),
            &[root.clone()],
            &[root.clone()],
            &request,
        )
        .expect_err("unsafe extension process request must be denied");
        assert!(
            err.to_string().contains(expected),
            "process denial must explain the violated policy: {err}"
        );
    }
}

#[tokio::test]
async fn extension_process_host_captures_bounded_result() {
    let root = TempDir::new().expect("test workspace must be created");
    let request = ProcessRequest {
        request_id: "version".to_string(),
        command: "rustc".to_string(),
        arguments: vec!["--version".to_string()],
        stdin: None,
        workspace_root: None,
        timeout_ms: Some(5_000),
    };
    let validated = validate_extension_process_request(
        "process-test",
        true,
        &["process.exec".to_string()],
        &["rustc".to_string()],
        &[root.path().to_path_buf()],
        &[root.path().to_path_buf()],
        &request,
    )
    .expect("trusted allowlisted process request must validate");

    let result = run_extension_process(validated, IndexingResourceGovernor::default()).await;

    assert_eq!(result.request_id, "version");
    assert_eq!(result.status, ProcessResultStatus::Exited);
    assert_eq!(result.exit_code, Some(0));
    assert!(result.stdout.starts_with("rustc "));
    assert!(!result.stdout_truncated);
    assert!(!result.stderr_truncated);
}

#[tokio::test(flavor = "current_thread")]
async fn extension_process_waits_for_weighted_admission_and_releases_exact_lease() {
    let governor = crate::utils::admission::IndexingResourceGovernor::new(
        crate::utils::admission::IndexingResourcePolicy::with_limits(1, 1, 128 * 1024 * 1024, 1),
    );
    let (holder_started_tx, holder_started_rx) = tokio::sync::oneshot::channel();
    let holder_release = Arc::new(tokio::sync::Notify::new());
    let holder_governor = governor.clone();
    let holder_release_task = holder_release.clone();
    let holder = tokio::spawn(async move {
        holder_governor
            .run_async_with_resources(
                "extension process contention holder",
                crate::utils::admission::IndexingWorkSpec::new(
                    Some(crate::test::harness::fixture_path("/workspace/background")),
                    crate::utils::admission::IndexingResourcePriority::Background,
                    1,
                    128 * 1024 * 1024,
                    1,
                ),
                None,
                async move {
                    holder_started_tx.send(()).unwrap();
                    holder_release_task.notified().await;
                },
            )
            .await
            .unwrap();
    });
    holder_started_rx.await.unwrap();

    let root = TempDir::new().expect("test workspace must be created");
    let request = ProcessRequest {
        request_id: "version-after-admission".to_string(),
        command: "rustc".to_string(),
        arguments: vec!["--version".to_string()],
        stdin: None,
        workspace_root: None,
        timeout_ms: Some(5_000),
    };
    let validated = validate_extension_process_request(
        "process-test",
        true,
        &["process.exec".to_string()],
        &["rustc".to_string()],
        &[root.path().to_path_buf()],
        &[root.path().to_path_buf()],
        &request,
    )
    .expect("trusted allowlisted process request must validate");
    let process_governor = governor.clone();
    let process =
        tokio::spawn(async move { run_extension_process(validated, process_governor).await });
    tokio::time::timeout(Duration::from_secs(1), async {
        while governor.snapshot().queued_tasks != 1 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("extension process must queue behind the complete weighted claim");
    assert_eq!(governor.snapshot().active_tasks, 1);

    holder_release.notify_one();
    holder.await.unwrap();
    let result = process.await.unwrap();
    assert_eq!(result.status, ProcessResultStatus::Exited);
    assert!(result.stdout.starts_with("rustc "));
    let complete = governor.snapshot();
    assert_eq!(complete.active_tasks, 0);
    assert_eq!(complete.queued_tasks, 0);
    assert_eq!(complete.completed_tasks, 2);
    assert_eq!(complete.peak_active_cpu_lanes, 1);
    assert_eq!(
        complete.peak_active_transient_memory_bytes,
        128 * 1024 * 1024
    );
    assert_eq!(complete.peak_active_io_slots, 1);
}

#[cfg(unix)]
#[tokio::test]
async fn extension_process_host_kills_timed_out_process() {
    let root = TempDir::new().expect("test workspace must be created");
    let request = ProcessRequest {
        request_id: "timeout".to_string(),
        command: "sh".to_string(),
        arguments: vec!["-c".to_string(), "sleep 5".to_string()],
        stdin: None,
        workspace_root: None,
        timeout_ms: Some(10),
    };
    let validated = validate_extension_process_request(
        "process-test",
        true,
        &["process.exec".to_string()],
        &["sh".to_string()],
        &[root.path().to_path_buf()],
        &[root.path().to_path_buf()],
        &request,
    )
    .expect("explicitly allowlisted shell process must validate");

    let result = run_extension_process(validated, IndexingResourceGovernor::default()).await;

    assert_eq!(result.status, ProcessResultStatus::TimedOut);
    assert_eq!(result.exit_code, None);
}
