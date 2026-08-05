//! Headless project checking over the shared analysis pipeline.
//!
//! This module owns filesystem discovery and terminal/JSON-ready projection.
//! Runtime/project loading, parsing, fact collection, method lookup, inference,
//! and semantic diagnostics remain in the same `IndexingCoordinator`,
//! `FileProcessor`, and `AnalysisEngine` used by the LSP.

use crate::capabilities::diagnostics::generate_diagnostics;
use crate::config::{IndexingConfig, RubyFastLspConfig};
use crate::indexer::coordinator::IndexingCoordinator;
use crate::indexer::file_processor::analysis_source;
use crate::server::RubyLanguageServer;
use crate::utils::should_index_file;
use anyhow::{anyhow, Context, Result};
use ruby_analysis::core::{
    DiagnosticSeverity, InferenceTelemetry, SourceKind, TextRange, TypeInferenceOutcome,
    TypeSubject, UnknownReason,
};
use ruby_analysis::engine::{AnalysisEngine, AnalysisQuery};
use ruby_analysis::indexer::RubyDocument;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tower_lsp::lsp_types::{Diagnostic as LspDiagnostic, DiagnosticSeverity as LspSeverity, Url};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CheckPosition {
    /// One-based line number for CLI consumers.
    pub line: u32,
    /// One-based UTF-16 column for parity with LSP position semantics.
    pub column: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CheckRange {
    pub start: CheckPosition,
    pub end: CheckPosition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckDiagnostic {
    pub path: PathBuf,
    pub range: CheckRange,
    pub severity: CheckSeverity,
    pub code: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CheckTypeOutcome {
    Proven { type_label: String },
    Unknown { reason: UnknownReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckTypeSubjectKind {
    MethodReturn,
    Constant,
    Local,
    InstanceVariable,
    ClassVariable,
    GlobalVariable,
    Parameter,
    Expression,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct CheckInferredType {
    pub path: PathBuf,
    pub range: CheckRange,
    pub kind: CheckTypeSubjectKind,
    pub subject: String,
    pub outcome: CheckTypeOutcome,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct CheckSummary {
    pub errors: usize,
    pub warnings: usize,
    pub information: usize,
    pub hints: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckReport {
    pub schema_version: u32,
    pub root: PathBuf,
    pub files_checked: usize,
    /// True only after every selected project completed the shared LSP cold
    /// indexing lifecycle. Loader failures abort the check instead of silently
    /// weakening the lookup universe.
    pub dependency_loading_complete: bool,
    /// Missing-symbol claims withheld because dependency loading is incomplete.
    pub suppressed_inconclusive_diagnostics: usize,
    pub diagnostics: Vec<CheckDiagnostic>,
    /// Stable domain type results. Method returns retain exact proof outcomes;
    /// expressions additionally retain exact engine-owned Unknown reasons,
    /// while other subjects are emitted only from concrete engine facts.
    pub inferred_types: Vec<CheckInferredType>,
    pub summary: CheckSummary,
    pub inference: InferenceTelemetry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckOutputFormat {
    Human,
    Json,
}

impl CheckOutputFormat {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "human" => Ok(Self::Human),
            "json" => Ok(Self::Json),
            "text" | "pretty" | "json-pretty" => Err(anyhow!(
                "unsupported check format `{value}`; expected `human` or `json`"
            )),
            value => Err(anyhow!(
                "unsupported check format `{value}`; expected `human` or `json`"
            )),
        }
    }
}

impl CheckReport {
    pub fn has_failures(&self) -> bool {
        self.summary.errors > 0 || self.summary.warnings > 0
    }
}

pub fn render_report(report: &CheckReport, format: CheckOutputFormat) -> Result<String> {
    match format {
        CheckOutputFormat::Json => serde_json::to_string_pretty(report)
            .context("failed to serialize the check report as JSON"),
        CheckOutputFormat::Human => {
            let mut output = String::new();
            for inferred in &report.inferred_types {
                let result = match &inferred.outcome {
                    CheckTypeOutcome::Proven { type_label } => type_label.clone(),
                    CheckTypeOutcome::Unknown { reason } => {
                        format!("Unknown[{}]: {}", reason.code(), reason.explanation())
                    }
                };
                output.push_str(&format!(
                    "{}:{}:{}: type[{}]: {}\n",
                    inferred.path.display(),
                    inferred.range.start.line,
                    inferred.range.start.column,
                    inferred.subject,
                    result
                ));
            }
            for diagnostic in &report.diagnostics {
                let code = diagnostic
                    .code
                    .as_deref()
                    .map(|code| format!("[{code}]"))
                    .unwrap_or_default();
                output.push_str(&format!(
                    "{}:{}:{}: {}{}: {}\n",
                    diagnostic.path.display(),
                    diagnostic.range.start.line,
                    diagnostic.range.start.column,
                    severity_name(diagnostic.severity),
                    code,
                    diagnostic.message
                ));
            }
            output.push_str(&format!(
                "checked {} file(s): {} error(s), {} warning(s), {} informational, {} hint(s); \
                 {} proven method return(s), {} Unknown; {} inconclusive dependency diagnostic(s) suppressed",
                report.files_checked,
                report.summary.errors,
                report.summary.warnings,
                report.summary.information,
                report.summary.hints,
                report.inference.proven_method_returns,
                report.inference.unknown_method_returns,
                report.suppressed_inconclusive_diagnostics
            ));
            if !report.inference.unknown_reasons.is_empty() {
                output.push_str("; Unknown reasons: ");
                output.push_str(
                    &report
                        .inference
                        .unknown_reasons
                        .iter()
                        .map(|(reason, count)| format!("{}={count}", reason.code()))
                        .collect::<Vec<_>>()
                        .join(", "),
                );
            }
            Ok(output)
        }
    }
}

#[derive(Debug, Clone)]
pub struct CheckSession {
    indexing_config: IndexingConfig,
}

impl Default for CheckSession {
    fn default() -> Self {
        Self {
            indexing_config: IndexingConfig::default(),
        }
    }
}

impl CheckSession {
    pub fn new(indexing_config: IndexingConfig) -> Self {
        Self { indexing_config }
    }

    /// Analyze one project directory or one Ruby/RBS source without starting an
    /// LSP service. Inputs and all output diagnostics are sorted deterministically.
    pub async fn check_path(&self, input: &Path) -> Result<CheckReport> {
        let input = input
            .canonicalize()
            .with_context(|| format!("failed to resolve check path {}", input.display()))?;
        if !input.is_dir() && !input.is_file() {
            return Err(anyhow!(
                "check path is neither a file nor a directory: {}",
                input.display()
            ));
        }
        if input.is_file()
            && !input
                .extension()
                .is_some_and(|extension| extension == "rbs")
            && !should_index_file(&input)
        {
            return Err(anyhow!(
                "check source is not a supported Ruby or RBS file: {}",
                input.display()
            ));
        }

        let selected_file = input.is_file().then(|| input.clone());
        let root = if input.is_dir() {
            input.clone()
        } else {
            nearest_project_root(&input)?
        };

        let mut config = RubyFastLspConfig::default();
        config.indexing = self.indexing_config.clone();
        if let Some(selected_file) = selected_file.as_deref() {
            let relative = selected_file.strip_prefix(&root).with_context(|| {
                format!(
                    "selected check source {} is outside its project root {}",
                    selected_file.display(),
                    root.display()
                )
            })?;
            let pattern = relative.to_string_lossy().replace('\\', "/");
            if !config.indexing.included_patterns.contains(&pattern) {
                config.indexing.included_patterns.push(pattern);
            }
        }
        let server = RubyLanguageServer::default();
        *server.config.lock() = config.clone();

        let workspaces = if input.is_dir() {
            let root_uri = Url::from_directory_path(&root)
                .map_err(|()| anyhow!("check root is not a valid file URI: {}", root.display()))?;
            server.add_workspace_folder(root_uri)?
        } else {
            let root_uri = Url::from_directory_path(&root)
                .map_err(|()| anyhow!("check root is not a valid file URI: {}", root.display()))?;
            vec![server.add_workspace(root_uri)]
        };

        for workspace in &workspaces {
            let mut coordinator =
                IndexingCoordinator::new(workspace.root_path.clone(), config.clone());
            coordinator.set_analysis_engine(workspace.analysis_engine.clone());
            coordinator
                .run_complete_indexing(&server)
                .await
                .with_context(|| {
                    format!(
                        "failed to complete shared project indexing for {}",
                        workspace.root_path.display()
                    )
                })?;
        }

        let mut diagnostics = Vec::new();
        let mut inferred_types = Vec::new();
        let mut files_checked = 0usize;
        let mut inference = InferenceTelemetry::default();
        for workspace in &workspaces {
            let engine = workspace.analysis_engine.read();
            for file in engine.files() {
                if !matches!(file.kind, SourceKind::Project | SourceKind::Signature)
                    || !source_is_selected(&file.path, selected_file.as_deref(), &root)
                {
                    continue;
                }
                files_checked = files_checked.checked_add(1).expect(
                    "INVARIANT VIOLATED: checked file count exhausted usize. This is a bug \
                     because one process cannot retain more files than addressable memory. \
                     Fix: bound project discovery below usize::MAX.",
                );
                if let Some(file_telemetry) = engine.inference_telemetry_in_file(file.id) {
                    inference.merge(file_telemetry);
                }
                inferred_types.extend(solved_types_in_file(&engine, &root, file.id)?);
                if file.kind == SourceKind::Project {
                    let source = match file.source_text() {
                        Some(source) => source.to_string(),
                        None => std::fs::read_to_string(&file.path).with_context(|| {
                            format!(
                                "failed to reread {} for syntax diagnostics after semantic indexing",
                                file.path.display()
                            )
                        })?,
                    };
                    if !engine.file_content_matches(file.id, &source) {
                        return Err(anyhow!(
                            "check source {} changed while analysis was running; rerun the check \
                             so syntax and semantic diagnostics use one byte-identical input",
                            file.path.display()
                        ));
                    }
                    let uri = Url::from_file_path(&file.path).map_err(|()| {
                        anyhow!(
                            "check source is not a valid file URI: {}",
                            file.path.display()
                        )
                    })?;
                    let projected_source = analysis_source(&uri, &source);
                    let parse = ruby_prism::parse(projected_source.as_bytes());
                    let document =
                        RubyDocument::with_analysis_file_id(uri, source.clone(), 0, file.id);
                    diagnostics.extend(
                        generate_diagnostics(&parse, &document)
                            .into_iter()
                            .map(|diagnostic| lsp_diagnostic(&root, &file.path, diagnostic)),
                    );
                }
            }
            diagnostics.extend(domain_diagnostics(
                &engine,
                &root,
                selected_file.as_deref(),
            )?);
        }
        if let Some(selected_file) = selected_file.as_deref() {
            if files_checked != 1 {
                return Err(anyhow!(
                    "selected check source {} was not indexed exactly once by the shared project policy \
                     (indexed {files_checked} times)",
                    selected_file.display()
                ));
            }
        }
        normalize_diagnostics(&mut diagnostics);
        inferred_types.sort();
        inferred_types.dedup();

        let summary = summarize(&diagnostics);
        Ok(CheckReport {
            schema_version: 4,
            root,
            files_checked,
            dependency_loading_complete: true,
            suppressed_inconclusive_diagnostics: 0,
            diagnostics,
            inferred_types,
            summary,
            inference,
        })
    }
}

fn solved_types_in_file(
    engine: &AnalysisEngine,
    root: &Path,
    file_id: ruby_analysis::core::SourceFileId,
) -> Result<Vec<CheckInferredType>> {
    let file = engine
        .file(file_id)
        .ok_or_else(|| anyhow!("inferred types reference unknown file id {file_id:?}"))?;
    let query = AnalysisQuery::new(engine);
    let exact_outcomes = engine.method_return_outcomes_in_file(file_id);
    let mut inferred = Vec::new();

    for method in query.method_facts_in_file(file_id) {
        let outcome = exact_outcomes
            .and_then(|outcomes| outcomes.get(&method.fqn))
            .cloned()
            .unwrap_or_else(|| {
                TypeInferenceOutcome::from_optional(
                    query.method_return_type(&method),
                    UnknownReason::UnresolvedMethodReturn,
                )
            });
        let outcome = match outcome.proven_type() {
            Some(ruby_type) => CheckTypeOutcome::Proven {
                type_label: ruby_type.to_string(),
            },
            None => CheckTypeOutcome::Unknown {
                reason: outcome.unknown_reason().expect(
                    "INVARIANT VIOLATED: a non-proven inference outcome has no Unknown reason. This is a bug because TypeInferenceOutcome must make unproven states explicit. Fix: construct every failed proof through TypeInferenceOutcome::unknown.",
                ),
            },
        };
        inferred.push(CheckInferredType {
            path: report_path(root, &file.path),
            range: check_range(file, method.range)?,
            kind: CheckTypeSubjectKind::MethodReturn,
            subject: method.fqn.to_string(),
            outcome,
        });
    }

    for fact in query.type_facts_in_file(file_id) {
        if fact.ruby_type == ruby_analysis::inference::RubyType::Unknown {
            let TypeSubject::Expression(range) = fact.subject else {
                continue;
            };
            let Some(reason) = query.expression_unknown_reason(range) else {
                continue;
            };
            inferred.push(CheckInferredType {
                path: report_path(root, &file.path),
                range: check_range(file, fact.range)?,
                kind: CheckTypeSubjectKind::Expression,
                subject: "expression".to_string(),
                outcome: CheckTypeOutcome::Unknown { reason },
            });
            continue;
        }
        let (kind, subject) = match fact.subject {
            TypeSubject::Constant(fqn) => (CheckTypeSubjectKind::Constant, fqn.to_string()),
            TypeSubject::Local { name, .. } => (CheckTypeSubjectKind::Local, name),
            TypeSubject::InstanceVariable { owner, name } => (
                CheckTypeSubjectKind::InstanceVariable,
                format!("{owner}::{name}"),
            ),
            TypeSubject::ClassVariable { owner, name } => (
                CheckTypeSubjectKind::ClassVariable,
                format!("{owner}::{name}"),
            ),
            TypeSubject::GlobalVariable(name) => (CheckTypeSubjectKind::GlobalVariable, name),
            TypeSubject::Parameter { method, name } => {
                (CheckTypeSubjectKind::Parameter, format!("{method}({name})"))
            }
            TypeSubject::Expression(_) => {
                (CheckTypeSubjectKind::Expression, "expression".to_string())
            }
            TypeSubject::MethodReturn(_) => continue,
        };
        inferred.push(CheckInferredType {
            path: report_path(root, &file.path),
            range: check_range(file, fact.range)?,
            kind,
            subject,
            outcome: CheckTypeOutcome::Proven {
                type_label: fact.ruby_type.to_string(),
            },
        });
    }
    if let Some(call_outcomes) = query.call_expression_outcomes_in_file(file_id) {
        for (range, outcome) in call_outcomes {
            let outcome = match outcome.proven_type() {
                Some(ruby_type) => CheckTypeOutcome::Proven {
                    type_label: ruby_type.to_string(),
                },
                None => CheckTypeOutcome::Unknown {
                    reason: outcome.unknown_reason().expect(
                        "INVARIANT VIOLATED: an unproven call expression has no Unknown reason. This is a bug because call outcomes must preserve why a concrete type was withheld. Fix: construct deferred failures through TypeInferenceOutcome::unknown.",
                    ),
                },
            };
            inferred.push(CheckInferredType {
                path: report_path(root, &file.path),
                range: check_range(file, *range)?,
                kind: CheckTypeSubjectKind::Expression,
                subject: "expression".to_string(),
                outcome,
            });
        }
    }
    Ok(inferred)
}

fn check_range(file: &ruby_analysis::engine::SourceFile, range: TextRange) -> Result<CheckRange> {
    assert_eq!(
        file.id, range.file_id,
        "INVARIANT VIOLATED: inferred-type range belongs to a different file. This is a bug because per-file method facts must retain their owning SourceFileId. Fix: reject or rehome the fact during file replacement."
    );
    let start = file
        .byte_offset_to_line_character(range.start_byte)
        .ok_or_else(|| {
            anyhow!(
                "inferred-type start offset is outside {}",
                file.path.display()
            )
        })?;
    let end = file
        .byte_offset_to_line_character(range.end_byte)
        .ok_or_else(|| {
            anyhow!(
                "inferred-type end offset is outside {}",
                file.path.display()
            )
        })?;
    Ok(CheckRange {
        start: one_based_position(start),
        end: one_based_position(end),
    })
}

fn nearest_project_root(source: &Path) -> Result<PathBuf> {
    let parent = source
        .parent()
        .ok_or_else(|| anyhow!("check source has no parent directory: {}", source.display()))?;
    for ancestor in parent.ancestors() {
        if ancestor.join("Gemfile").is_file() {
            return Ok(ancestor.to_path_buf());
        }
    }
    Ok(parent.to_path_buf())
}

fn source_is_selected(path: &Path, selected_file: Option<&Path>, root: &Path) -> bool {
    selected_file.map_or_else(|| path.starts_with(root), |selected| path == selected)
}

fn domain_diagnostics(
    engine: &AnalysisEngine,
    root: &Path,
    selected_file: Option<&Path>,
) -> Result<Vec<CheckDiagnostic>> {
    let mut diagnostics = Vec::new();
    for diagnostic in engine.all_diagnostic_facts() {
        let file = engine.file(diagnostic.range.file_id).ok_or_else(|| {
            anyhow!(
                "diagnostic references unknown file id {:?}",
                diagnostic.range.file_id
            )
        })?;
        if selected_file.is_some_and(|selected| file.path != selected) {
            continue;
        }
        let start = file
            .byte_offset_to_line_character(diagnostic.range.start_byte)
            .ok_or_else(|| anyhow!("diagnostic start offset is outside {}", file.path.display()))?;
        let end = file
            .byte_offset_to_line_character(diagnostic.range.end_byte)
            .ok_or_else(|| anyhow!("diagnostic end offset is outside {}", file.path.display()))?;
        diagnostics.push(CheckDiagnostic {
            path: report_path(root, &file.path),
            range: CheckRange {
                start: one_based_position(start),
                end: one_based_position(end),
            },
            severity: domain_severity(diagnostic.severity),
            code: Some(diagnostic.code),
            message: diagnostic.message,
        });
    }
    Ok(diagnostics)
}

fn lsp_diagnostic(root: &Path, path: &Path, diagnostic: LspDiagnostic) -> CheckDiagnostic {
    let code = diagnostic.code.map(|code| match code {
        tower_lsp::lsp_types::NumberOrString::Number(value) => value.to_string(),
        tower_lsp::lsp_types::NumberOrString::String(value) => value,
    });
    CheckDiagnostic {
        path: report_path(root, path),
        range: CheckRange {
            start: one_based_position((
                diagnostic.range.start.line,
                diagnostic.range.start.character,
            )),
            end: one_based_position((diagnostic.range.end.line, diagnostic.range.end.character)),
        },
        severity: lsp_severity(diagnostic.severity),
        code,
        message: diagnostic.message,
    }
}

fn report_path(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| path.to_path_buf())
}

fn one_based_position((line, character): (u32, u32)) -> CheckPosition {
    CheckPosition {
        line: line.checked_add(1).expect(
            "INVARIANT VIOLATED: check diagnostic line exhausted u32. This is a bug because \
             source line indexes must fit the domain range representation. Fix: widen check \
             positions before admitting a source with u32::MAX lines.",
        ),
        column: character.checked_add(1).expect(
            "INVARIANT VIOLATED: check diagnostic column exhausted u32. This is a bug because \
             source UTF-16 columns must fit the domain range representation. Fix: widen check \
             positions before admitting a line with u32::MAX UTF-16 code units.",
        ),
    }
}

fn domain_severity(severity: DiagnosticSeverity) -> CheckSeverity {
    match severity {
        DiagnosticSeverity::Error => CheckSeverity::Error,
        DiagnosticSeverity::Warning => CheckSeverity::Warning,
        DiagnosticSeverity::Information => CheckSeverity::Information,
        DiagnosticSeverity::Hint => CheckSeverity::Hint,
    }
}

fn lsp_severity(severity: Option<LspSeverity>) -> CheckSeverity {
    match severity {
        Some(LspSeverity::ERROR) => CheckSeverity::Error,
        Some(LspSeverity::WARNING) => CheckSeverity::Warning,
        Some(LspSeverity::INFORMATION) => CheckSeverity::Information,
        Some(LspSeverity::HINT) => CheckSeverity::Hint,
        None => CheckSeverity::Error,
        Some(value) => panic!(
            "INVARIANT VIOLATED: unsupported LSP diagnostic severity {value:?}. This is a bug \
             because every protocol severity must map to a stable check severity. Fix: add the \
             new protocol severity to lsp_severity."
        ),
    }
}

fn normalize_diagnostics(diagnostics: &mut Vec<CheckDiagnostic>) {
    diagnostics.sort_by(|left, right| {
        (
            &left.path,
            left.range,
            left.severity,
            left.code.as_deref(),
            left.message.as_str(),
        )
            .cmp(&(
                &right.path,
                right.range,
                right.severity,
                right.code.as_deref(),
                right.message.as_str(),
            ))
    });
    diagnostics.dedup();
}

fn summarize(diagnostics: &[CheckDiagnostic]) -> CheckSummary {
    let mut summary = CheckSummary::default();
    for diagnostic in diagnostics {
        match diagnostic.severity {
            CheckSeverity::Error => summary.errors += 1,
            CheckSeverity::Warning => summary.warnings += 1,
            CheckSeverity::Information => summary.information += 1,
            CheckSeverity::Hint => summary.hints += 1,
        }
    }
    summary
}

fn severity_name(severity: CheckSeverity) -> &'static str {
    match severity {
        CheckSeverity::Error => "error",
        CheckSeverity::Warning => "warning",
        CheckSeverity::Information => "information",
        CheckSeverity::Hint => "hint",
    }
}
