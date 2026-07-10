use crate::config::{LinterKind, RubyFastLspConfig};
use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString, Position, Range};

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
    workspace_root: &Path,
    file_path: &Path,
    content: &str,
    timeout: Duration,
) -> Result<Vec<Diagnostic>> {
    if config.linter == LinterKind::None {
        return Ok(Vec::new());
    }

    let command_argv = if config.linter_command.is_empty() {
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
    } else {
        config.linter_command.clone()
    };
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
    use crate::config::{LinterKind, RubyFastLspConfig};
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
        let diagnostics = lint_document(
            &config,
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
            temp.path(),
            &temp.path().join("sample.rb"),
            "puts 1\n",
            Duration::from_millis(20),
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("timed out"), "{error:#}");
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
