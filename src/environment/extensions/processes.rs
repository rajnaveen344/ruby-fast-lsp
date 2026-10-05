use crate::invariant::ExpectInvariant;
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use log::warn;
use ruby_fast_lsp_extension_api::{
    ExtensionEvent, ProcessRequest, ProcessResult, ProcessResultStatus, WatchedFileChange,
    WatchedFileChangeKind,
};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Command as ProcessCommand;
use tower_lsp::lsp_types::{FileChangeType, FileEvent, Url};

use crate::environment::extensions::loading::config::ExtensionLoadError;
use crate::environment::extensions::registry::handle::ExtensionRegistryHandle;
use crate::environment::extensions::registry::loaded::LoadedWasmExtension;
use crate::environment::extensions::{
    DEFAULT_PROCESS_TIMEOUT, EXTENSION_PROCESS_TRANSIENT_MEMORY_BYTES, MAX_PROCESS_ARGUMENTS,
    MAX_PROCESS_ARGUMENT_BYTES, MAX_PROCESS_OUTPUT_BYTES, MAX_PROCESS_REQUESTS_PER_EVENT,
    MAX_PROCESS_STDIN_BYTES, MAX_PROCESS_TIMEOUT,
};
use crate::utils::admission::{
    IndexingResourceGovernor, IndexingResourcePriority, IndexingWorkSpec,
};

pub(super) fn handle_watched_file_changes_with_registry(
    registry: &ExtensionRegistryHandle,
    workspace_roots: &[PathBuf],
    changes: &[FileEvent],
) -> Vec<PendingExtensionProcessRequest> {
    let mut pending = Vec::new();
    let candidates = watched_file_candidates(workspace_roots, changes);
    if candidates.is_empty() {
        return pending;
    }

    for loaded in registry.extensions() {
        if !loaded.is_loaded() || loaded.metadata.watched_files.is_empty() {
            continue;
        }
        let matched = candidates
            .iter()
            .filter(|change| loaded.watched_file_matcher.is_match(&change.path))
            .cloned()
            .collect::<Vec<_>>();
        if matched.is_empty() {
            continue;
        }
        let event_roots = matched
            .iter()
            .map(|change| PathBuf::from(&change.workspace_root))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        let event = ExtensionEvent {
            event: "files.changed".to_string(),
            call: None,
            document: None,
            project: None,
            settings: None,
            files: Some(matched),
            process_results: None,
        };
        match loaded.handle_event_for_project(&event, None) {
            Ok(output)
                if output.index_patches.is_empty()
                    && output.execution_contexts.is_empty()
                    && output.response_patches.is_empty()
                    && output.command_patches.is_empty() =>
            {
                if output.process_requests.len() > MAX_PROCESS_REQUESTS_PER_EVENT {
                    loaded.reject(format!(
                        "extension `{}` returned {} process requests from `files.changed`, exceeding the limit of {MAX_PROCESS_REQUESTS_PER_EVENT}",
                        loaded.metadata.id,
                        output.process_requests.len()
                    ));
                    continue;
                }
                let mut request_ids = BTreeSet::new();
                if output
                    .process_requests
                    .iter()
                    .any(|request| !request_ids.insert(request.request_id.clone()))
                {
                    loaded.reject(format!(
                        "extension `{}` returned duplicate process request ids from `files.changed`",
                        loaded.metadata.id
                    ));
                    continue;
                }
                pending.extend(output.process_requests.into_iter().map(|request| {
                    PendingExtensionProcessRequest {
                        loaded: Arc::clone(&loaded),
                        event_roots: event_roots.clone(),
                        request,
                    }
                }));
            }
            Ok(_) => {
                loaded.reject(format!(
                    "extension `{}` returned patches from `files.changed`; watched-file events may update private extension state only",
                    loaded.metadata.id
                ));
            }
            Err(err) => {
                loaded.fail(format!(
                    "extension `{}` files.changed failed: {err}",
                    loaded.metadata.id
                ));
            }
        }
    }
    pending
}

pub(super) struct PendingExtensionProcessRequest {
    pub(super) loaded: Arc<LoadedWasmExtension>,
    pub(super) event_roots: Vec<PathBuf>,
    pub(super) request: ProcessRequest,
}

pub(super) fn watched_file_candidates(
    workspace_roots: &[PathBuf],
    changes: &[FileEvent],
) -> Vec<WatchedFileChange> {
    let mut roots = workspace_roots.to_vec();
    roots.sort_by(|left, right| {
        right
            .as_os_str()
            .len()
            .cmp(&left.as_os_str().len())
            .then_with(|| left.cmp(right))
    });
    roots.dedup();

    let mut candidates = BTreeSet::new();
    for change in changes {
        let Ok(file_path) = change.uri.to_file_path() else {
            warn!(
                "Ignoring extension watched-file event with non-file URI `{}`",
                change.uri
            );
            continue;
        };
        let Some(root) = roots.iter().find(|root| file_path.starts_with(root)) else {
            continue;
        };
        let relative = file_path.strip_prefix(root).expect_invariant(
            "watched file selected a workspace root that is not its prefix",
            "the root was chosen with starts_with",
            "keep root selection and strip_prefix adjacent",
        );
        let kind = if change.typ == FileChangeType::CREATED {
            WatchedFileChangeKind::Created
        } else if change.typ == FileChangeType::CHANGED {
            WatchedFileChangeKind::Changed
        } else if change.typ == FileChangeType::DELETED {
            WatchedFileChangeKind::Deleted
        } else {
            warn!(
                "Ignoring extension watched-file event with unsupported change type for `{}`",
                change.uri
            );
            continue;
        };
        candidates.insert(WatchedFileChange {
            workspace_root: root.to_string_lossy().replace('\\', "/"),
            path: relative.to_string_lossy().replace('\\', "/"),
            uri: change.uri.to_string(),
            kind,
        });
    }
    candidates.into_iter().collect()
}

#[derive(Debug)]
pub(super) struct ValidatedExtensionProcessRequest {
    request_id: String,
    program: PathBuf,
    arguments: Vec<String>,
    stdin: Option<String>,
    workspace_root: PathBuf,
    timeout: Duration,
}

pub(super) fn validate_extension_process_request(
    extension_id: &str,
    workspace_trusted: bool,
    permissions: &[String],
    allowed_commands: &[String],
    workspace_roots: &[PathBuf],
    event_roots: &[PathBuf],
    request: &ProcessRequest,
) -> Result<ValidatedExtensionProcessRequest, ExtensionLoadError> {
    if !workspace_trusted {
        return Err(ExtensionLoadError::new(format!(
            "extension `{extension_id}` requested a process outside a trusted workspace"
        )));
    }
    if !permissions
        .iter()
        .any(|permission| permission == "process.exec")
    {
        return Err(ExtensionLoadError::new(format!(
            "extension `{extension_id}` requested a process without `process.exec` permission"
        )));
    }
    if !allowed_commands
        .iter()
        .any(|command| command == &request.command)
    {
        return Err(ExtensionLoadError::new(format!(
            "extension `{extension_id}` requested command `{}` which is not allowlisted by its manifest",
            request.command
        )));
    }
    if request.request_id.is_empty() || request.request_id.len() > 128 {
        return Err(ExtensionLoadError::new(format!(
            "extension `{extension_id}` process request id must contain 1..=128 bytes"
        )));
    }
    if request.arguments.len() > MAX_PROCESS_ARGUMENTS
        || request
            .arguments
            .iter()
            .any(|argument| argument.len() > MAX_PROCESS_ARGUMENT_BYTES)
    {
        return Err(ExtensionLoadError::new(format!(
            "extension `{extension_id}` process request exceeds argument count or size limits"
        )));
    }
    if request
        .stdin
        .as_ref()
        .is_some_and(|stdin| stdin.len() > MAX_PROCESS_STDIN_BYTES)
    {
        return Err(ExtensionLoadError::new(format!(
            "extension `{extension_id}` process stdin exceeds {MAX_PROCESS_STDIN_BYTES} bytes"
        )));
    }

    let mut roots = workspace_roots.to_vec();
    roots.sort();
    roots.dedup();
    let mut allowed_event_roots = event_roots.to_vec();
    allowed_event_roots.sort();
    allowed_event_roots.dedup();
    let workspace_root = match &request.workspace_root {
        Some(requested) => roots
            .iter()
            .find(|root| normalized_path(root) == *requested)
            .cloned()
            .filter(|root| allowed_event_roots.contains(root))
            .ok_or_else(|| {
                ExtensionLoadError::new(format!(
                    "extension `{extension_id}` requested unregistered or unrelated workspace root `{requested}`"
                ))
            })?,
        None if allowed_event_roots.len() == 1 => allowed_event_roots[0].clone(),
        None => {
            return Err(ExtensionLoadError::new(format!(
                "extension `{extension_id}` process request must select one workspace root when an event spans multiple roots"
            )))
        }
    };
    if !roots.contains(&workspace_root) {
        return Err(ExtensionLoadError::new(format!(
            "extension `{extension_id}` process request resolved outside registered workspace roots"
        )));
    }

    let command_path = Path::new(&request.command);
    let program = if command_path.components().count() == 1 {
        command_path.to_path_buf()
    } else {
        if command_path.is_absolute()
            || command_path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(ExtensionLoadError::new(format!(
                "extension `{extension_id}` process command `{}` must be a bare executable or workspace-relative path without traversal",
                request.command
            )));
        }
        workspace_root.join(command_path)
    };
    let requested_timeout = Duration::from_millis(
        request
            .timeout_ms
            .unwrap_or(DEFAULT_PROCESS_TIMEOUT.as_millis() as u64),
    );
    let timeout = requested_timeout.min(MAX_PROCESS_TIMEOUT);
    if timeout.is_zero() {
        return Err(ExtensionLoadError::new(format!(
            "extension `{extension_id}` process timeout must be greater than zero"
        )));
    }

    Ok(ValidatedExtensionProcessRequest {
        request_id: request.request_id.clone(),
        program,
        arguments: request.arguments.clone(),
        stdin: request.stdin.clone(),
        workspace_root,
        timeout,
    })
}

const MAX_RUNTIME_REINDEX_FILES: usize = 256;

pub(super) fn validate_extension_reindex_files(
    extension_id: &str,
    workspace_roots: &[PathBuf],
    event_roots: &[PathBuf],
    requests: &[ruby_fast_lsp_extension_api::ReindexFile],
) -> Result<Vec<Url>, ExtensionLoadError> {
    if requests.len() > MAX_RUNTIME_REINDEX_FILES {
        return Err(ExtensionLoadError::new(format!(
            "extension `{extension_id}` requested {} runtime reindex files, exceeding the limit of {MAX_RUNTIME_REINDEX_FILES}",
            requests.len()
        )));
    }
    let roots = workspace_roots.iter().cloned().collect::<BTreeSet<_>>();
    let event_roots = event_roots.iter().cloned().collect::<BTreeSet<_>>();
    let mut uris = BTreeSet::new();
    for request in requests {
        let root = roots
            .iter()
            .find(|root| normalized_path(root) == request.workspace_root)
            .filter(|root| event_roots.contains(*root))
            .ok_or_else(|| {
                ExtensionLoadError::new(format!(
                    "extension `{extension_id}` requested runtime reindex outside event-related workspace root `{}`",
                    request.workspace_root
                ))
            })?;
        let relative = Path::new(&request.path);
        if relative.as_os_str().is_empty()
            || relative.is_absolute()
            || relative.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(ExtensionLoadError::new(format!(
                "extension `{extension_id}` runtime reindex path `{}` must be workspace-relative without traversal",
                request.path
            )));
        }
        let canonical_root = dunce::canonicalize(root).map_err(|err| {
            ExtensionLoadError::new(format!(
                "extension `{extension_id}` runtime reindex workspace root `{}` could not be canonicalized: {err}",
                request.workspace_root
            ))
        })?;
        let requested_path = root.join(relative);
        let canonical_path = dunce::canonicalize(&requested_path).map_err(|err| {
            ExtensionLoadError::new(format!(
                "extension `{extension_id}` runtime reindex path `{}` is not an existing file: {err}",
                request.path
            ))
        })?;
        if !canonical_path.starts_with(&canonical_root) || !canonical_path.is_file() {
            return Err(ExtensionLoadError::new(format!(
                "extension `{extension_id}` runtime reindex path `{}` resolves outside its workspace root or is not a file",
                request.path
            )));
        }
        let uri = Url::from_file_path(canonical_path).map_err(|_| {
            ExtensionLoadError::new(format!(
                "extension `{extension_id}` runtime reindex path `{}` could not convert to a file URI",
                request.path
            ))
        })?;
        uris.insert(uri);
    }
    Ok(uris.into_iter().collect())
}

pub(super) fn normalized_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

pub(super) async fn run_extension_process(
    request: ValidatedExtensionProcessRequest,
    indexing_resources: IndexingResourceGovernor,
) -> ProcessResult {
    let spec = IndexingWorkSpec::new(
        Some(request.workspace_root.clone()),
        IndexingResourcePriority::Background,
        1,
        EXTENSION_PROCESS_TRANSIENT_MEMORY_BYTES,
        1,
    );
    indexing_resources
        .run_async_with_resources(
            "extension child process",
            spec,
            None,
            run_extension_process_admitted(request),
        )
        .await
        .expect_invariant(
            "a non-cancellable extension process failed resource admission",
            "its fixed positive claim must fit the server-owned policy",
            "keep the extension process claim within the configured production budget",
        )
}

async fn run_extension_process_admitted(
    request: ValidatedExtensionProcessRequest,
) -> ProcessResult {
    let mut command = ProcessCommand::new(&request.program);
    command
        .args(&request.arguments)
        .current_dir(&request.workspace_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(err) => {
            return ProcessResult {
                request_id: request.request_id,
                status: ProcessResultStatus::Failed,
                exit_code: None,
                stdout: String::new(),
                stderr: format!(
                    "failed to start extension process `{}` in {}: {err}",
                    request.program.display(),
                    request.workspace_root.display()
                ),
                stdout_truncated: false,
                stderr_truncated: false,
            }
        }
    };
    let stdout = child.stdout.take().expect_invariant(
        "extension process has no piped stdout",
        "stdout is configured before spawning",
        "keep stdout piped before taking the child handle",
    );
    let stderr = child.stderr.take().expect_invariant(
        "extension process has no piped stderr",
        "stderr is configured before spawning",
        "keep stderr piped before taking the child handle",
    );
    let stdout_task = tokio::spawn(read_bounded_process_output(stdout));
    let stderr_task = tokio::spawn(read_bounded_process_output(stderr));
    let mut stdin = child.stdin.take().expect_invariant(
        "extension process has no piped stdin",
        "stdin is configured before spawning",
        "keep stdin piped before taking the child handle",
    );
    let stdin_content = request.stdin.unwrap_or_default();
    let stdin_task = tokio::spawn(async move {
        let result = stdin.write_all(stdin_content.as_bytes()).await;
        drop(stdin);
        result
    });

    let (status, exit_code, wait_error) =
        match tokio::time::timeout(request.timeout, child.wait()).await {
            Ok(Ok(status)) => (ProcessResultStatus::Exited, status.code(), None),
            Ok(Err(err)) => (ProcessResultStatus::Failed, None, Some(err.to_string())),
            Err(_) => {
                let kill_error = child.kill().await.err().map(|err| err.to_string());
                let _ = child.wait().await;
                (ProcessResultStatus::TimedOut, None, kill_error)
            }
        };
    let stdin_error = stdin_task
        .await
        .expect_invariant(
            "extension process stdin task panicked",
            "the task only writes bounded bytes",
            "keep panicking work out of the stdin task",
        )
        .err()
        .map(|err| err.to_string());
    let (stdout, stdout_truncated) = stdout_task.await.expect_invariant(
        "extension process stdout task panicked",
        "the task only drains bounded process output",
        "keep panicking work out of the output task",
    );
    let (mut stderr, stderr_truncated) = stderr_task.await.expect_invariant(
        "extension process stderr task panicked",
        "the task only drains bounded process output",
        "keep panicking work out of the output task",
    );
    for error in [wait_error, stdin_error].into_iter().flatten() {
        if !stderr.is_empty() {
            stderr.push(b'\n');
        }
        stderr.extend_from_slice(error.as_bytes());
    }

    ProcessResult {
        request_id: request.request_id,
        status,
        exit_code,
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        stdout_truncated,
        stderr_truncated,
    }
}

async fn read_bounded_process_output(mut reader: impl AsyncRead + Unpin) -> (Vec<u8>, bool) {
    let mut retained = Vec::new();
    let mut buffer = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let read = match reader.read(&mut buffer).await {
            Ok(0) => break,
            Ok(read) => read,
            Err(err) => {
                let message = format!("failed to read extension process output: {err}");
                let remaining = MAX_PROCESS_OUTPUT_BYTES.saturating_sub(retained.len());
                retained.extend_from_slice(&message.as_bytes()[..message.len().min(remaining)]);
                truncated |= message.len() > remaining;
                break;
            }
        };
        let remaining = MAX_PROCESS_OUTPUT_BYTES.saturating_sub(retained.len());
        let keep = read.min(remaining);
        retained.extend_from_slice(&buffer[..keep]);
        truncated |= keep < read;
    }
    (retained, truncated)
}
