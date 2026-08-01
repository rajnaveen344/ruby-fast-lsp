use crate::config::{FormatterKind, LinterKind, RubyFastLspConfig};
use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};

use crate::indexing_resources::{
    IndexingResourceGovernor, IndexingResourcePriority, IndexingWorkSpec,
};

const EDITOR_TOOL_TRANSIENT_MEMORY_BYTES: usize = 128 * 1024 * 1024;

#[derive(Debug, Deserialize)]
struct LinterReport {
    files: Vec<LinterFile>,
}

#[derive(Debug, Deserialize)]
struct LinterFile {
    offenses: Vec<LinterOffense>,
}

#[derive(Debug, Deserialize)]
struct LinterOffense {
    severity: String,
    message: String,
    cop_name: String,
    #[serde(default)]
    correctable: bool,
    location: LinterLocation,
}

#[derive(Debug, Deserialize)]
struct LinterLocation {
    start_line: u32,
    start_column: u32,
    last_line: u32,
    last_column: u32,
}

pub async fn lint_document(
    config: &RubyFastLspConfig,
    indexing_resources: IndexingResourceGovernor,
    workspace_root: &Path,
    file_path: &Path,
    content: &str,
    timeout: Duration,
) -> Result<Vec<Diagnostic>> {
    if config.linter == LinterKind::None {
        return Ok(Vec::new());
    }
    let spec = editor_tool_work_spec(workspace_root);
    indexing_resources
        .run_async_with_resources(
            "external Ruby linter",
            spec,
            None,
            lint_document_admitted(config, workspace_root, file_path, content, timeout),
        )
        .await?
}

async fn lint_document_admitted(
    config: &RubyFastLspConfig,
    workspace_root: &Path,
    file_path: &Path,
    content: &str,
    timeout: Duration,
) -> Result<Vec<Diagnostic>> {
    let command_argv = resolved_command(config);
    let (program, initial_args) = command_argv.split_first().expect(
        "INVARIANT VIOLATED: linter command argv is empty after default command resolution. \
         This is a bug because enabled linters must always resolve to a program. \
         Fix: preserve the default command or validate configured argv before execution.",
    );

    let mut command = Command::new(program);
    let stdin_path = file_path.strip_prefix(workspace_root).unwrap_or(file_path);
    command
        .args(initial_args)
        .args(["--format", "json", "--force-exclusion", "--stdin"])
        .arg(stdin_path)
        .current_dir(workspace_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = command.spawn().with_context(|| {
        format!(
            "failed to start {} linter command `{}` in {}",
            config.linter.data_name().unwrap_or("configured"),
            command_argv.join(" "),
            workspace_root.display()
        )
    })?;
    let mut stdin = child.stdin.take().expect(
        "INVARIANT VIOLATED: spawned linter has no piped stdin. \
         This is a bug because the command is always configured with Stdio::piped(). \
         Fix: keep stdin piped before taking the child handle.",
    );
    stdin
        .write_all(content.as_bytes())
        .await
        .context("failed to write Ruby source to linter stdin")?;
    drop(stdin);

    let output = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| {
            anyhow!(
                "{} linter timed out after {} ms while checking {}",
                config.linter.data_name().unwrap_or("configured"),
                timeout.as_millis(),
                file_path.display()
            )
        })?
        .context("failed while waiting for linter process")?;

    let exit_code = output.status.code();
    if !matches!(exit_code, Some(0) | Some(1)) {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(anyhow!(
            "{} linter failed for {} with exit status {:?}: {}",
            config.linter.data_name().unwrap_or("configured"),
            file_path.display(),
            exit_code,
            if stderr.is_empty() {
                "no error output"
            } else {
                stderr.as_str()
            }
        ));
    }

    let stdout = std::str::from_utf8(&output.stdout).context("linter output was not UTF-8")?;
    parse_linter_json(stdout, config.linter, content)
}

pub async fn fix_document(
    config: &RubyFastLspConfig,
    indexing_resources: IndexingResourceGovernor,
    workspace_root: &Path,
    file_path: &Path,
    content: &str,
    timeout: Duration,
) -> Result<String> {
    if config.linter == LinterKind::None {
        return Err(anyhow!(
            "cannot request a fix while the external linter is disabled"
        ));
    }
    let command_argv = resolved_command(config);
    let fix_flag = match config.linter {
        LinterKind::RuboCop => "--autocorrect",
        LinterKind::Standard => "--fix",
        LinterKind::None => unreachable!(
            "INVARIANT VIOLATED: disabled linter reached safe fix flag selection. \
             This is a bug because fix_document rejects LinterKind::None first. \
             Fix: preserve the disabled-linter guard above."
        ),
    };
    run_correction_with_resources(
        indexing_resources,
        &command_argv,
        config.linter.data_name().expect(
            "INVARIANT VIOLATED: enabled linter has no data name. This is a bug because correction errors must identify their tool. Fix: add the LinterKind mapping.",
        ),
        fix_flag,
        workspace_root,
        file_path,
        content,
        timeout,
    )
    .await
}

pub async fn format_document(
    config: &RubyFastLspConfig,
    indexing_resources: IndexingResourceGovernor,
    workspace_root: &Path,
    file_path: &Path,
    content: &str,
    timeout: Duration,
) -> Result<String> {
    if config.formatter == FormatterKind::None {
        return Err(anyhow!(
            "cannot format while the external formatter is disabled"
        ));
    }
    let command_argv = if config.formatter_command.is_empty() {
        vec![
            "bundle".to_string(),
            "exec".to_string(),
            config
                .formatter
                .executable()
                .expect(
                    "INVARIANT VIOLATED: enabled formatter has no executable. This is a bug because every enabled formatter must resolve to a program. Fix: add the FormatterKind executable mapping.",
                )
                .to_string(),
        ]
    } else {
        config.formatter_command.clone()
    };
    let fix_flag = match config.formatter {
        FormatterKind::RuboCop => "--autocorrect",
        FormatterKind::Standard => "--fix",
        FormatterKind::None => unreachable!(
            "INVARIANT VIOLATED: disabled formatter reached flag selection. This is a bug because format_document rejects FormatterKind::None first. Fix: preserve that guard."
        ),
    };
    run_correction_with_resources(
        indexing_resources,
        &command_argv,
        config.formatter.data_name().expect(
            "INVARIANT VIOLATED: enabled formatter has no data name. This is a bug because formatter errors must identify their tool. Fix: add the FormatterKind mapping.",
        ),
        fix_flag,
        workspace_root,
        file_path,
        content,
        timeout,
    )
    .await
}

async fn run_correction_with_resources(
    indexing_resources: IndexingResourceGovernor,
    command_argv: &[String],
    tool_name: &str,
    fix_flag: &str,
    workspace_root: &Path,
    file_path: &Path,
    content: &str,
    timeout: Duration,
) -> Result<String> {
    let spec = editor_tool_work_spec(workspace_root);
    indexing_resources
        .run_async_with_resources(
            "external Ruby correction tool",
            spec,
            None,
            run_correction(
                command_argv,
                tool_name,
                fix_flag,
                workspace_root,
                file_path,
                content,
                timeout,
            ),
        )
        .await?
}

async fn run_correction(
    command_argv: &[String],
    tool_name: &str,
    fix_flag: &str,
    workspace_root: &Path,
    file_path: &Path,
    content: &str,
    timeout: Duration,
) -> Result<String> {
    let (program, initial_args) = command_argv.split_first().expect(
        "INVARIANT VIOLATED: correction command argv is empty after resolution. This is a bug because enabled tools must always resolve to a program. Fix: preserve default commands or validate configured argv before execution.",
    );
    let stdin_path = file_path.strip_prefix(workspace_root).unwrap_or(file_path);
    let mut child = Command::new(program)
        .args(initial_args)
        .args([fix_flag, "--stderr", "--force-exclusion", "--stdin"])
        .arg(stdin_path)
        .current_dir(workspace_root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .with_context(|| {
            format!(
                "failed to start safe {} fix command `{}` in {}",
                tool_name,
                command_argv.join(" "),
                workspace_root.display()
            )
        })?;
    let mut stdin = child.stdin.take().expect(
        "INVARIANT VIOLATED: spawned linter fix has no piped stdin. \
         This is a bug because the command is always configured with Stdio::piped(). \
         Fix: keep stdin piped before taking the child handle.",
    );
    stdin
        .write_all(content.as_bytes())
        .await
        .context("failed to write Ruby source to safe linter fix stdin")?;
    drop(stdin);

    let output = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| {
            anyhow!(
                "safe {} fix timed out after {} ms while checking {}",
                tool_name,
                timeout.as_millis(),
                file_path.display()
            )
        })?
        .context("failed while waiting for safe linter fix process")?;
    if !matches!(output.status.code(), Some(0) | Some(1)) {
        return Err(anyhow!(
            "safe {} fix failed for {} with exit status {:?}: {}",
            tool_name,
            file_path.display(),
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let fixed = String::from_utf8(output.stdout).context("safe linter fix output was not UTF-8")?;
    if fixed.is_empty() && !content.is_empty() {
        return Err(anyhow!(
            "safe {} fix returned empty source for non-empty document {}",
            tool_name,
            file_path.display()
        ));
    }
    Ok(fixed)
}

fn editor_tool_work_spec(workspace_root: &Path) -> IndexingWorkSpec {
    IndexingWorkSpec::new(
        Some(workspace_root.to_path_buf()),
        IndexingResourcePriority::OpenDocument,
        1,
        EDITOR_TOOL_TRANSIENT_MEMORY_BYTES,
        1,
    )
}

fn resolved_command(config: &RubyFastLspConfig) -> Vec<String> {
    if !config.linter_command.is_empty() {
        return config.linter_command.clone();
    }
    vec![
        "bundle".to_string(),
        "exec".to_string(),
        config
            .linter
            .executable()
            .expect(
                "INVARIANT VIOLATED: enabled linter has no executable. \
                 This is a bug because every enabled linter kind must map to an executable. \
                 Fix: add the executable mapping when adding a LinterKind variant.",
            )
            .to_string(),
    ]
}

pub fn parse_linter_json(
    output: &str,
    linter: LinterKind,
    content: &str,
) -> Result<Vec<Diagnostic>> {
    assert!(
        linter != LinterKind::None,
        "INVARIANT VIOLATED: linter JSON parsing was requested for LinterKind::None. \
         This is a bug because disabled linters cannot produce reports. \
         Fix: return before spawning or parsing when the linter is disabled."
    );
    let report: LinterReport =
        serde_json::from_str(output).context("linter returned malformed JSON")?;
    let source = linter.diagnostic_source().expect(
        "INVARIANT VIOLATED: enabled linter has no diagnostic source. \
         This is a bug because published diagnostics must identify their provider. \
         Fix: add the diagnostic source mapping for the enabled linter.",
    );
    let data_name = linter.data_name().expect(
        "INVARIANT VIOLATED: enabled linter has no stable data name. \
         This is a bug because code actions need to identify the diagnostic provider. \
         Fix: add the data name mapping for the enabled linter.",
    );

    report
        .files
        .into_iter()
        .flat_map(|file| file.offenses)
        .map(|offense| {
            let start = rubocop_position(
                content,
                offense.location.start_line,
                offense.location.start_column.saturating_sub(1),
            )?;
            let end = rubocop_position(
                content,
                offense.location.last_line,
                offense.location.last_column,
            )?;
            Ok(Diagnostic {
                range: Range::new(start, end),
                severity: Some(linter_severity(&offense.severity)),
                code: Some(NumberOrString::String(offense.cop_name)),
                code_description: None,
                source: Some(source.to_string()),
                message: offense.message,
                related_information: None,
                tags: None,
                data: Some(serde_json::json!({
                    "linter": data_name,
                    "correctable": offense.correctable
                })),
            })
        })
        .collect()
}

fn rubocop_position(content: &str, one_based_line: u32, byte_column: u32) -> Result<Position> {
    let line_index = one_based_line
        .checked_sub(1)
        .ok_or_else(|| anyhow!("linter returned invalid one-based line 0"))?;
    let line = content.lines().nth(line_index as usize).ok_or_else(|| {
        anyhow!(
            "linter returned line {} outside a {}-line document",
            one_based_line,
            content.lines().count()
        )
    })?;
    let byte_column = byte_column as usize;
    if byte_column > line.len() || !line.is_char_boundary(byte_column) {
        return Err(anyhow!(
            "linter returned byte column {} that is not a UTF-8 boundary in line {}",
            byte_column,
            one_based_line
        ));
    }
    Ok(Position::new(
        line_index,
        line[..byte_column].encode_utf16().count() as u32,
    ))
}

fn linter_severity(severity: &str) -> DiagnosticSeverity {
    match severity {
        "fatal" | "error" => DiagnosticSeverity::ERROR,
        "warning" | "convention" | "refactor" => DiagnosticSeverity::WARNING,
        "info" => DiagnosticSeverity::INFORMATION,
        _ => DiagnosticSeverity::WARNING,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{FormatterKind, LinterKind, RubyFastLspConfig};
    use std::fs;
    use std::time::Duration;
    use tempfile::TempDir;
    use tower_lsp::lsp_types::{DiagnosticSeverity, NumberOrString, Position};

    const RUBOCOP_JSON: &str = r#"{
      "metadata": {"rubocop_version": "1.75.0"},
      "files": [{
        "path": "sample.rb",
        "offenses": [{
          "severity": "convention",
          "message": "Style/StringLiterals: Prefer single-quoted strings.",
          "cop_name": "Style/StringLiterals",
          "corrected": false,
          "correctable": true,
          "location": {
            "start_line": 2,
            "start_column": 3,
            "last_line": 2,
            "last_column": 9,
            "length": 7,
            "line": 2,
            "column": 3
          }
        }]
      }],
      "summary": {"offense_count": 1, "target_file_count": 1, "inspected_file_count": 1}
    }"#;

    #[test]
    fn parses_rubocop_json_into_lsp_diagnostics() {
        let diagnostics =
            parse_linter_json(RUBOCOP_JSON, LinterKind::RuboCop, "first\n  example\n").unwrap();
        assert_eq!(diagnostics.len(), 1);
        let diagnostic = &diagnostics[0];
        assert_eq!(diagnostic.range.start, Position::new(1, 2));
        assert_eq!(diagnostic.range.end, Position::new(1, 9));
        assert_eq!(diagnostic.severity, Some(DiagnosticSeverity::WARNING));
        assert_eq!(
            diagnostic.code,
            Some(NumberOrString::String("Style/StringLiterals".to_string()))
        );
        assert_eq!(diagnostic.source.as_deref(), Some("RuboCop"));
        assert_eq!(
            diagnostic.data,
            Some(serde_json::json!({
                "linter": "rubocop",
                "correctable": true
            }))
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runs_configured_linter_with_stdin_and_accepts_offense_exit_status() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let executable = temp.path().join("fake-rubocop");
        let captured_stdin = temp.path().join("stdin.rb");
        let captured_args = temp.path().join("args.txt");
        let captured_pwd = temp.path().join("pwd.txt");
        let script = format!(
            "#!/bin/sh\nprintf '%s' \"$*\" > '{}'\npwd > '{}'\ncat > '{}'\nprintf '%s' '{}'\nexit 1\n",
            captured_args.display(),
            captured_pwd.display(),
            captured_stdin.display(),
            RUBOCOP_JSON.replace('\'', "'\\''")
        );
        fs::write(&executable, script).unwrap();
        let mut permissions = fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).unwrap();

        let config = RubyFastLspConfig {
            linter: LinterKind::RuboCop,
            linter_command: vec![executable.to_string_lossy().to_string()],
            ..RubyFastLspConfig::default()
        };
        let source = "puts \"hello\"\n  example\n";
        let indexing_resources = IndexingResourceGovernor::new(
            crate::indexing_resources::IndexingResourcePolicy::with_limits(
                1,
                1,
                EDITOR_TOOL_TRANSIENT_MEMORY_BYTES,
                1,
            ),
        );
        let diagnostics = lint_document(
            &config,
            indexing_resources.clone(),
            temp.path(),
            &temp.path().join("sample.rb"),
            source,
            Duration::from_secs(2),
        )
        .await
        .unwrap();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(fs::read_to_string(captured_stdin).unwrap(), source);
        let args = fs::read_to_string(captured_args).unwrap();
        assert!(args.contains("--format json --force-exclusion --stdin"));
        assert!(args.ends_with("sample.rb"), "actual argv: {args}");
        assert_eq!(
            fs::canonicalize(fs::read_to_string(captured_pwd).unwrap().trim()).unwrap(),
            fs::canonicalize(temp.path()).unwrap()
        );
        let resources = indexing_resources.snapshot();
        assert_eq!(resources.completed_tasks, 1);
        assert_eq!(resources.active_tasks, 0);
        assert_eq!(resources.queued_tasks, 0);
        assert_eq!(resources.peak_active_cpu_lanes, 1);
        assert_eq!(
            resources.peak_active_transient_memory_bytes,
            EDITOR_TOOL_TRANSIENT_MEMORY_BYTES
        );
        assert_eq!(resources.peak_active_io_slots, 1);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn times_out_a_hung_linter() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let executable = temp.path().join("hung-rubocop");
        fs::write(&executable, "#!/bin/sh\nsleep 5\n").unwrap();
        let mut permissions = fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).unwrap();
        let config = RubyFastLspConfig {
            linter: LinterKind::RuboCop,
            linter_command: vec![executable.to_string_lossy().to_string()],
            ..RubyFastLspConfig::default()
        };

        let error = lint_document(
            &config,
            IndexingResourceGovernor::default(),
            temp.path(),
            &temp.path().join("sample.rb"),
            "puts 1\n",
            Duration::from_millis(20),
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("timed out"), "{error:#}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn standard_safe_fix_uses_fix_flag_and_current_stdin() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let executable = temp.path().join("fake-standardrb");
        let captured_args = temp.path().join("fix-args.txt");
        let script = format!(
            "#!/bin/sh\nprintf '%s' \"$*\" > '{}'\ncat\n",
            captured_args.display()
        );
        fs::write(&executable, script).unwrap();
        let mut permissions = fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).unwrap();
        let config = RubyFastLspConfig {
            linter: LinterKind::Standard,
            linter_command: vec![executable.to_string_lossy().to_string()],
            ..RubyFastLspConfig::default()
        };

        let source = "puts 'already safe'\n";
        let fixed = fix_document(
            &config,
            IndexingResourceGovernor::default(),
            temp.path(),
            &temp.path().join("sample.rb"),
            source,
            Duration::from_secs(2),
        )
        .await
        .unwrap();

        assert_eq!(fixed, source);
        let args = fs::read_to_string(captured_args).unwrap();
        assert!(args.contains("--fix --stderr --force-exclusion --stdin"));
        assert!(!args.contains("--fix-unsafely"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn rubocop_formatter_uses_safe_autocorrect_and_current_stdin() {
        use std::os::unix::fs::PermissionsExt;

        let temp = TempDir::new().unwrap();
        let executable = temp.path().join("fake-rubocop");
        let captured_args = temp.path().join("format-args.txt");
        let script = format!(
            "#!/bin/sh\nprintf '%s' \"$*\" > '{}'\ncat\n",
            captured_args.display()
        );
        fs::write(&executable, script).unwrap();
        let mut permissions = fs::metadata(&executable).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).unwrap();
        let config = RubyFastLspConfig {
            formatter: FormatterKind::RuboCop,
            formatter_command: vec![executable.to_string_lossy().to_string()],
            ..RubyFastLspConfig::default()
        };

        let source = "puts \"current buffer\"\n";
        let formatted = format_document(
            &config,
            IndexingResourceGovernor::default(),
            temp.path(),
            &temp.path().join("sample.rb"),
            source,
            Duration::from_secs(2),
        )
        .await
        .unwrap();

        assert_eq!(formatted, source);
        let args = fs::read_to_string(captured_args).unwrap();
        assert!(args.contains("--autocorrect --stderr --force-exclusion --stdin"));
        assert!(!args.contains("--autocorrect-all"));
        assert!(args.ends_with("sample.rb"));
    }

    #[test]
    fn standard_diagnostics_identify_standard_as_the_source() {
        let diagnostics =
            parse_linter_json(RUBOCOP_JSON, LinterKind::Standard, "first\n  example\n").unwrap();
        assert_eq!(diagnostics[0].source.as_deref(), Some("Standard"));
        assert_eq!(
            diagnostics[0].data,
            Some(serde_json::json!({
                "linter": "standard",
                "correctable": true
            }))
        );
    }

    #[test]
    fn malformed_linter_output_is_an_actionable_error() {
        let error = parse_linter_json("not json", LinterKind::RuboCop, "puts 1\n").unwrap_err();
        assert!(
            error.to_string().contains("malformed JSON"),
            "actual error: {error:#}"
        );
    }

    #[test]
    fn converts_rubocop_utf8_byte_columns_to_lsp_utf16_columns() {
        let report = RUBOCOP_JSON
            .replace("\"start_line\": 2", "\"start_line\": 1")
            .replace("\"last_line\": 2", "\"last_line\": 1")
            .replace("\"start_column\": 3", "\"start_column\": 5")
            .replace("\"last_column\": 9", "\"last_column\": 7");
        let diagnostics = parse_linter_json(&report, LinterKind::RuboCop, "😀foo\n").unwrap();

        assert_eq!(diagnostics[0].range.start, Position::new(0, 2));
        assert_eq!(diagnostics[0].range.end, Position::new(0, 5));
    }
}
