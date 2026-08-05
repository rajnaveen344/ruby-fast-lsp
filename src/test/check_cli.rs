use crate::check::{CheckDiagnostic, CheckReport, CheckSession, CheckTypeOutcome, CheckTypeSubjectKind};
use crate::test::harness::FakeEditor;
use ruby_analysis::UnknownReason;
use tower_lsp::lsp_types::{InlayHintLabel, NumberOrString, Url};

fn hover_text(hover: tower_lsp::lsp_types::Hover) -> String {
    match hover.contents {
        tower_lsp::lsp_types::HoverContents::Scalar(marked) => match marked {
            tower_lsp::lsp_types::MarkedString::String(value) => value,
            tower_lsp::lsp_types::MarkedString::LanguageString(value) => value.value,
        },
        tower_lsp::lsp_types::HoverContents::Array(values) => values
            .into_iter()
            .map(|marked| match marked {
                tower_lsp::lsp_types::MarkedString::String(value) => value,
                tower_lsp::lsp_types::MarkedString::LanguageString(value) => value.value,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        tower_lsp::lsp_types::HoverContents::Markup(value) => value.value,
    }
}

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
    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.as_deref() == Some("wrong-arity")),
        "INVARIANT VIOLATED: the headless check session did not report the engine's \
         wrong-arity diagnostic. This is a bug because CLI and LSP must consume the same \
         semantic facts without starting an LSP client. Fix: route check inputs through the \
         shared FileProcessor and AnalysisEngine lifecycle."
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

    assert!(
        report.diagnostics.iter().any(|diagnostic| {
            matches!(
                diagnostic.code.as_deref(),
                Some("unresolved-method" | "unresolved-constant" | "unresolved-require")
            )
        }),
        "INVARIANT VIOLATED: the complete check session withheld every dependency diagnostic. \
         This is a bug because a standalone project without a Gemfile has a deliberately closed \
         dependency universe after core/project indexing. Fix: suppress absence claims only when \
         the shared project loader itself reports incomplete state."
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

    assert!(
        report.dependency_loading_complete,
        "INVARIANT VIOLATED: a successful project check did not complete the same runtime, core, \
         stdlib, gem, signature, and project loading lifecycle as LSP cold indexing. This is a bug \
         because the CLI cannot prove absence against a partial semantic universe. Fix: route the \
         check session through the shared IndexingCoordinator before emitting diagnostics."
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

#[tokio::test]
async fn local_semantic_diagnostic_matches_lsp_projection() {
    let source = "def greet(name)\n  name\nend\n\ngreet\n";
    let project = tempfile::tempdir().expect("temporary parity project must be created");
    std::fs::write(project.path().join("main.rb"), source).expect("parity fixture must be written");
    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the parity fixture");
    let check_diagnostic = check_report
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.as_deref() == Some("wrong-arity"))
        .expect("check must return the proven wrong-arity diagnostic");

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let lsp_diagnostic = editor
        .diagnostics("main.rb")
        .await
        .into_iter()
        .find(|diagnostic| {
            diagnostic.code.as_ref().is_some_and(
                |code| matches!(code, NumberOrString::String(code) if code == "wrong-arity"),
            )
        })
        .expect("LSP must return the proven wrong-arity diagnostic");

    assert_eq!(check_diagnostic.message, lsp_diagnostic.message);
    assert_eq!(
        (
            check_diagnostic.range.start.line,
            check_diagnostic.range.start.column,
            check_diagnostic.range.end.line,
            check_diagnostic.range.end.column,
        ),
        (
            lsp_diagnostic.range.start.line + 1,
            lsp_diagnostic.range.start.character + 1,
            lsp_diagnostic.range.end.line + 1,
            lsp_diagnostic.range.end.character + 1,
        ),
        "INVARIANT VIOLATED: CLI and LSP projected different ranges for one engine-owned \
         diagnostic. This is a bug because adapters may change indexing conventions but not \
         semantic locations. Fix: keep check range conversion aligned with LSP UTF-16 positions."
    );
}

#[tokio::test]
async fn normalized_method_return_types_match_lsp_inlay_projection() {
    let source = r#"class Types
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
"#;
    let project = tempfile::tempdir().expect("temporary type-parity project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("type-parity fixture must be written");

    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the type-parity fixture");
    let check_types = check_report
        .inferred_types
        .iter()
        .filter(|inferred| inferred.kind == CheckTypeSubjectKind::MethodReturn)
        .map(|inferred| {
            let type_label = match &inferred.outcome {
                CheckTypeOutcome::Proven { type_label } => type_label.as_str(),
                CheckTypeOutcome::Unknown { reason } => {
                    assert_eq!(*reason, UnknownReason::UnprovenRecursiveCycle);
                    "?"
                }
            };
            (inferred.range.start.line, format!(" -> {type_label}"))
        })
        .collect::<Vec<_>>();

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let lsp_types = editor
        .inlay_hints("main.rb")
        .await
        .into_iter()
        .filter_map(|hint| {
            let InlayHintLabel::String(label) = hint.label else {
                return None;
            };
            label.starts_with(" -> ").then(|| {
                (
                    hint.position.line.checked_add(1).expect(
                        "INVARIANT VIOLATED: LSP line exhausted u32 during parity normalization. This is a bug because source positions must fit u32. Fix: reject sources whose normalized line cannot be one-based.",
                    ),
                    label,
                )
            })
        })
        .collect::<Vec<_>>();

    assert_eq!(check_types, lsp_types);
}

#[tokio::test]
async fn normalized_variable_and_expression_types_match_lsp_inlay_projection() {
    let source = "class User\nend\nuser = User.new\nUser.new\n  .to_s\n";
    let project = tempfile::tempdir().expect("temporary type-parity project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("type-parity fixture must be written");

    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze variable and expression types");
    let projected = check_report
        .inferred_types
        .iter()
        .filter(|inferred| {
            matches!(
                (inferred.kind, inferred.subject.as_str()),
                (CheckTypeSubjectKind::Local, "user")
            ) || (inferred.kind == CheckTypeSubjectKind::Expression
                && inferred.subject == "expression"
                && inferred.range.end.line == 4
                && inferred.range.end.column == 9)
        })
        .filter_map(|inferred| {
            let type_label = match &inferred.outcome {
                CheckTypeOutcome::Proven { type_label } => type_label,
                CheckTypeOutcome::Unknown { reason } => {
                    assert_eq!(
                        *reason,
                        UnknownReason::UnresolvedMethodReturn,
                        "every withheld call type must retain its exact proof failure"
                    );
                    return None;
                }
            };
            Some((
                inferred.subject.as_str(),
                inferred.range.end.line,
                inferred.range.end.column,
                format!(": {type_label}"),
            ))
        })
        .collect::<Vec<_>>();

    assert_eq!(
        projected,
        vec![
            ("user", 3, 5, ": User".to_string()),
            ("expression", 4, 9, ": User".to_string()),
        ]
    );

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let lsp_types = editor
        .inlay_hints("main.rb")
        .await
        .into_iter()
        .filter_map(|hint| {
            let InlayHintLabel::String(label) = hint.label else {
                return None;
            };
            matches!(
                (hint.position.line, hint.position.character),
                (2, 4) | (3, 8)
            )
            .then(|| (hint.position.line + 1, hint.position.character + 1, label))
        })
        .collect::<Vec<_>>();

    assert_eq!(
        lsp_types,
        vec![(3, 5, ": User".to_string()), (4, 9, ": User".to_string())]
    );
}

#[tokio::test]
async fn normalized_parameter_and_nonlocal_variable_types_match_lsp_inlay_projection() {
    let source = r#"class Types
  # @param value [String]
  def record(value)
    @current = 1
    @@last = "last"
    $global = :symbol
  end
end
VALUE = 1.0
"#;
    let project = tempfile::tempdir().expect("temporary type-parity project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("type-parity fixture must be written");

    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze parameter and nonlocal variable types");
    let projected = check_report
        .inferred_types
        .iter()
        .filter(|inferred| {
            matches!(
                inferred.kind,
                CheckTypeSubjectKind::Parameter
                    | CheckTypeSubjectKind::InstanceVariable
                    | CheckTypeSubjectKind::ClassVariable
                    | CheckTypeSubjectKind::GlobalVariable
            ) || (inferred.kind == CheckTypeSubjectKind::Constant && inferred.subject == "VALUE")
        })
        .map(|inferred| {
            let CheckTypeOutcome::Proven { type_label } = &inferred.outcome else {
                panic!("parameter/nonlocal parity must not invent an unexplained Unknown")
            };
            (
                inferred.range.end.line,
                inferred.range.end.column,
                format!(": {type_label}"),
            )
        })
        .collect::<Vec<_>>();

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let mut lsp_types = editor
        .inlay_hints("main.rb")
        .await
        .into_iter()
        .filter_map(|hint| {
            let InlayHintLabel::String(label) = hint.label else {
                return None;
            };
            matches!(
                (hint.position.line, hint.position.character),
                (2, 18) | (3, 12) | (4, 10) | (5, 11) | (8, 5)
            )
            .then(|| {
                (
                    hint.position.line.checked_add(1).expect(
                        "INVARIANT VIOLATED: LSP line exhausted u32 during parity normalization. This is a bug because source positions must fit u32. Fix: reject sources whose normalized line cannot be one-based.",
                    ),
                    hint.position.character.checked_add(1).expect(
                        "INVARIANT VIOLATED: LSP column exhausted u32 during parity normalization. This is a bug because source positions must fit u32. Fix: reject sources whose normalized column cannot be one-based.",
                    ),
                    label,
                )
            })
        })
        .collect::<Vec<_>>();
    lsp_types.sort();

    assert_eq!(projected, lsp_types);
}

#[tokio::test]
async fn normalized_nonlocal_variable_reads_match_lsp_hover_projection() {
    let source = r#"class Types
  def record
    @current = 1
    @current
    @@last = "last"
    @@last
    $global = :symbol
    $global
  end
end
"#;
    let project = tempfile::tempdir().expect("temporary type-parity project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("type-parity fixture must be written");

    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze nonlocal variable reads");
    let projected = check_report
        .inferred_types
        .iter()
        .filter(|inferred| inferred.kind == CheckTypeSubjectKind::Expression)
        .map(|inferred| {
            let CheckTypeOutcome::Proven { type_label } = &inferred.outcome else {
                panic!("a concrete nonlocal read must not become an unexplained Unknown")
            };
            (
                inferred.range.start.line,
                inferred.range.start.column,
                inferred.range.end.column,
                type_label.clone(),
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        projected,
        vec![
            (4, 5, 13, "Integer".to_string()),
            (6, 5, 11, "String".to_string()),
            (8, 5, 12, "Symbol".to_string()),
        ],
        "the CLI must project each exact engine-owned nonlocal read"
    );

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    for (line, character, expected) in [
        (3, 5, "@current: Integer"),
        (5, 5, "@@last: String"),
        (7, 5, "$global: Symbol"),
    ] {
        let hover = editor
            .hover_at("main.rb", line, character)
            .await
            .expect("a proven nonlocal read must have hover output");
        let actual = hover_text(hover);
        assert!(
            actual.contains(expected),
            "CLI/LSP parity expected hover `{expected}`, got `{actual}`"
        );
    }
}

#[tokio::test]
async fn unknown_nonlocal_read_reason_matches_cli_and_lsp() {
    let source = "class Types\n  def read\n    @missing\n  end\nend\n";
    let project = tempfile::tempdir().expect("temporary type-parity project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("type-parity fixture must be written");

    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must explain the unproven nonlocal read");
    let read = check_report
        .inferred_types
        .iter()
        .find(|inferred| {
            inferred.kind == CheckTypeSubjectKind::Expression
                && inferred.range.start.line == 3
                && inferred.range.start.column == 5
        })
        .expect("the CLI must retain an exact Unknown outcome for the read");
    assert_eq!(
        read.outcome,
        CheckTypeOutcome::Unknown {
            reason: UnknownReason::NoReachingAssignment,
        }
    );

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let hover = editor
        .hover_at("main.rb", 2, 5)
        .await
        .expect("the unproven read must retain hover context");
    let actual = hover_text(hover);
    assert!(
        actual.contains("Unknown[no_reaching_assignment]"),
        "LSP hover must project the same machine-readable reason as CLI, got `{actual}`"
    );
}

#[tokio::test]
async fn unresolved_nonlocal_assignment_reason_matches_cli_and_lsp() {
    let source = "class Types\n  def read\n    @value = dynamic_value\n    @value\n  end\nend\n";
    let project = tempfile::tempdir().expect("temporary type-parity project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("type-parity fixture must be written");

    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must explain the unresolved reaching assignment");
    let read = check_report
        .inferred_types
        .iter()
        .find(|inferred| {
            inferred.kind == CheckTypeSubjectKind::Expression
                && inferred.range.start.line == 4
                && inferred.range.start.column == 5
        })
        .expect("the CLI must retain the Unknown outcome for the assigned nonlocal read");
    assert_eq!(
        read.outcome,
        CheckTypeOutcome::Unknown {
            reason: UnknownReason::UnresolvedAssignmentValue,
        }
    );

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let hover = editor
        .hover_at("main.rb", 3, 5)
        .await
        .expect("the unresolved assigned read must retain hover context");
    let actual = hover_text(hover);
    assert!(
        actual.contains("Unknown[unresolved_assignment_value]"),
        "LSP hover must project the same reaching-assignment failure as CLI, got `{actual}`"
    );
}

#[tokio::test]
async fn unmatched_yard_parameter_never_becomes_a_concrete_type_subject() {
    let source = "# @param ghost [String]\ndef actual\nend\n";
    let project = tempfile::tempdir().expect("temporary proof-safety project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("proof-safety fixture must be written");

    let report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the unmatched YARD parameter");

    assert!(
        report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code.as_deref() == Some("yard-unknown-param")),
        "the invalid declaration must remain visible as an engine-owned diagnostic"
    );
    assert!(
        report
            .inferred_types
            .iter()
            .all(|inferred| inferred.kind != CheckTypeSubjectKind::Parameter),
        "an annotation for a nonexistent parameter is not static proof of a program entity"
    );
}

#[tokio::test]
async fn unresolved_chained_call_reason_matches_cli_and_lsp_without_an_inlay() {
    let source = "dynamic_user.fetch\n  .profile\n";
    let project = tempfile::tempdir().expect("temporary type-parity project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("type-parity fixture must be written");

    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the unresolved expression");
    let chained_call = check_report
        .inferred_types
        .iter()
        .find(|inferred| {
            inferred.kind == CheckTypeSubjectKind::Expression
                && inferred.range.start.line == 1
                && inferred.range.start.column == 1
                && inferred.range.end.line == 2
        })
        .expect("the CLI must retain the exact Unknown outcome for the chained call");
    assert_eq!(
        chained_call.outcome,
        CheckTypeOutcome::Unknown {
            reason: UnknownReason::UnknownReceiver,
        },
        "the outer call is unproven because its receiver call has no proven result"
    );

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let hover = editor
        .hover_at("main.rb", 1, 3)
        .await
        .expect("the unresolved chained call must retain hover context");
    let actual = hover_text(hover);
    assert!(
        actual.contains("Unknown[unknown_receiver]"),
        "LSP hover must project the same proof failure as the CLI, got `{actual}`"
    );
    assert!(
        editor
            .inlay_hints("main.rb")
            .await
            .into_iter()
            .all(|hint| hint.position != tower_lsp::lsp_types::Position::new(0, 18)),
        "the LSP must stay silent at an unresolved chain boundary"
    );
}

#[tokio::test]
async fn unresolved_method_return_reason_matches_cli_and_lsp() {
    let source = "class User\n  def profile\n    dynamic_profile\n  end\nend\n\nUser.new.profile\n";
    let project = tempfile::tempdir().expect("temporary type-parity project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("type-parity fixture must be written");

    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must explain the unproven method return");
    let call = check_report
        .inferred_types
        .iter()
        .find(|inferred| {
            inferred.kind == CheckTypeSubjectKind::Expression
                && inferred.range.start.line == 7
                && inferred.range.start.column == 1
                && inferred.range.end.column == 17
        })
        .expect("the CLI must retain the exact Unknown outcome for User.new.profile");
    assert_eq!(
        call.outcome,
        CheckTypeOutcome::Unknown {
            reason: UnknownReason::UnresolvedMethodReturn,
        }
    );

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let hover = editor
        .hover_at("main.rb", 6, 10)
        .await
        .expect("the unproven method call must retain hover context");
    let actual = hover_text(hover);
    assert!(
        actual.contains("Unknown[unresolved_method_return]"),
        "LSP hover must project the same method-return failure as CLI, got `{actual}`"
    );
}

#[tokio::test]
async fn deferred_method_resolution_proof_matches_cli_and_lsp() {
    let source = "module FeatureFlags\n  def self.included(base)\n    base.extend(ClassMethods)\n  end\n  module ClassMethods\n    def status\n      \"on\"\n    end\n  end\nend\n\nclass Worker\n  include FeatureFlags\nend\n\nWorker.status\n";
    let project = tempfile::tempdir().expect("temporary type-parity project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("type-parity fixture must be written");

    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must finalize the included-hook call proof");
    let call = check_report
        .inferred_types
        .iter()
        .find(|inferred| {
            inferred.kind == CheckTypeSubjectKind::Expression
                && inferred.range.start.line == 16
                && inferred.range.start.column == 1
                && inferred.range.end.column == 14
        })
        .expect("the CLI must retain the finalized Worker.status expression type");
    assert_eq!(
        call.outcome,
        CheckTypeOutcome::Proven {
            type_label: "String".to_string(),
        },
        "the complete engine graph must upgrade the first-pass Unknown rather than leaking it to the CLI"
    );

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let hover = editor
        .hover_at("main.rb", 15, 8)
        .await
        .expect("the finalized call must have hover output");
    let actual = hover_text(hover);
    assert!(
        actual.contains("String"),
        "LSP hover must consume the same finalized proof as CLI, got `{actual}`"
    );
}

#[tokio::test]
async fn lambda_call_proof_matches_cli_and_lsp() {
    let source = "builder = -> { \"ready\" }\nbuilder.call\n";
    let project = tempfile::tempdir().expect("temporary type-parity project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("type-parity fixture must be written");

    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must retain the lambda-call proof");
    let call = check_report
        .inferred_types
        .iter()
        .find(|inferred| {
            inferred.kind == CheckTypeSubjectKind::Expression
                && inferred.range.start.line == 2
                && inferred.range.start.column == 1
                && inferred.range.end.column == 13
        })
        .unwrap_or_else(|| {
            panic!(
                "the CLI must retain the lambda-call expression outcome, got {:#?}",
                check_report.inferred_types
            )
        });
    assert_eq!(
        call.outcome,
        CheckTypeOutcome::Proven {
            type_label: "String".to_string(),
        },
        "the CLI must use the same lambda-body proof as LSP hover"
    );

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let hover = editor
        .hover_at("main.rb", 1, 9)
        .await
        .expect("the lambda call must have hover output");
    let actual = hover_text(hover);
    assert!(
        actual.contains("String"),
        "LSP hover must consume the same lambda-body proof as CLI, got `{actual}`"
    );
}

#[tokio::test]
async fn cross_file_recursive_return_proof_matches_cli_and_lsp() {
    let even_source =
        "class Cycle\n  def even(n)\n    return \"done\" if n.zero?\n    odd(n - 1)\n  end\nend\n";
    let odd_source = "class Cycle\n  def odd(n)\n    even(n - 1)\n  end\nend\n";
    let call_source = "Cycle.new.odd(2)\n";
    let project = tempfile::tempdir().expect("temporary type-parity project must be created");
    std::fs::write(project.path().join("cycle_even.rb"), even_source)
        .expect("even recursion fixture must be written");
    std::fs::write(project.path().join("cycle_odd.rb"), odd_source)
        .expect("odd recursion fixture must be written");
    std::fs::write(project.path().join("main.rb"), call_source)
        .expect("recursive call fixture must be written");

    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must solve the complete cross-file return component");
    let call = check_report
        .inferred_types
        .iter()
        .find(|inferred| {
            inferred.path == std::path::Path::new("main.rb")
                && inferred.kind == CheckTypeSubjectKind::Expression
                && inferred.range.start.line == 1
                && inferred.range.start.column == 1
                && inferred.range.end.column == 17
        })
        .unwrap_or_else(|| {
            panic!(
                "the CLI must retain the cross-file recursive call outcome, got {:#?}",
                check_report.inferred_types
            )
        });
    assert_eq!(
        call.outcome,
        CheckTypeOutcome::Proven {
            type_label: "String".to_string(),
        },
        "the CLI must solve mutually recursive returns across file boundaries"
    );
    let recursive_methods = check_report
        .inferred_types
        .iter()
        .filter(|inferred| {
            inferred.kind == CheckTypeSubjectKind::MethodReturn
                && matches!(inferred.subject.as_str(), "Cycle#even" | "Cycle#odd")
        })
        .collect::<Vec<_>>();
    assert_eq!(recursive_methods.len(), 2);
    assert!(recursive_methods.iter().all(|method| {
        method.outcome
            == CheckTypeOutcome::Proven {
                type_label: "String".to_string(),
            }
    }));
    assert_eq!(check_report.inference.recursive_components, 1);
    assert_eq!(check_report.inference.recursive_methods, 2);
    assert_eq!(check_report.inference.proven_method_returns, 2);
    assert_eq!(check_report.inference.unknown_method_returns, 0);

    let mut editor = FakeEditor::new().await;
    editor.open("cycle_even.rb", even_source).await;
    editor.open("cycle_odd.rb", odd_source).await;
    editor.open("main.rb", call_source).await;
    let hover = editor
        .hover_at("main.rb", 0, 12)
        .await
        .expect("the cross-file recursive call must have hover output");
    let actual = hover_text(hover);
    assert!(
        actual.contains("String"),
        "LSP hover must consume the same cross-file recursive proof as CLI, got `{actual}`"
    );

    let unchanged_equation = format!("{even_source}# body-independent edit\n");
    editor.set("cycle_even.rb", &unchanged_equation).await;
    let analysis_engine = editor.server().analysis_engine_for_uri(
        &Url::parse("file:///cycle_even.rb").expect("cycle fixture URI must be valid"),
    );
    let even_file_id = analysis_engine
        .read()
        .file_id(std::path::Path::new("/cycle_even.rb"))
        .expect("cycle fixture must be registered in the analysis engine");
    let equations_before_unchanged_edit = analysis_engine
        .read()
        .method_return_equations_in_file(even_file_id)
        .expect("cycle fixture must retain its return equations")
        .to_vec();
    editor.set("cycle_even.rb", &unchanged_equation).await;
    assert_eq!(
        analysis_engine
            .read()
            .method_return_equations_in_file(even_file_id)
            .expect("unchanged cycle fixture must retain its return equations"),
        equations_before_unchanged_edit,
        "a body-independent edit must not alter the method-return equation IR"
    );
    assert_eq!(
        analysis_engine
            .read()
            .last_resolve_stats()
            .method_return_equation_solve_runs,
        0,
        "an unchanged equation edit must reuse the existing project solution"
    );
    let hover = editor
        .hover_at("main.rb", 0, 12)
        .await
        .expect("an unchanged equation replacement must retain hover output");
    let actual = hover_text(hover);
    assert!(
        actual.contains("String"),
        "replacing a file with the same equation must retain the cached project proof, got `{actual}`"
    );

    let integer_even_source =
        "class Cycle\n  def even(n)\n    return 1 if n.zero?\n    odd(n - 1)\n  end\nend\n";
    editor.set("cycle_even.rb", integer_even_source).await;
    assert_eq!(
        analysis_engine
            .read()
            .last_resolve_stats()
            .method_return_equation_solve_runs,
        1,
        "a changed recursive base must run exactly one project equation solve"
    );
    let hover = editor
        .hover_at("main.rb", 0, 12)
        .await
        .expect("a changed recursive base must refresh dependent hover output");
    let actual = hover_text(hover);
    assert!(
        actual.contains("Integer") && !actual.contains("String"),
        "changing one file's recursive base must re-solve the project component, got `{actual}`"
    );

    let base_free_even_source = "class Cycle\n  def even(n)\n    odd(n - 1)\n  end\nend\n";
    editor.set("cycle_even.rb", base_free_even_source).await;
    let hover = editor
        .hover_at("main.rb", 0, 12)
        .await
        .expect("a base-free recursive call must retain explanatory hover output");
    let actual = hover_text(hover);
    assert!(
        actual.contains("Unknown[unresolved_method_return]")
            && !actual.contains("Integer")
            && !actual.contains("String"),
        "removing the final recursive base must invalidate every stale concrete type, got `{actual}`"
    );
}

#[tokio::test]
async fn reopened_implicit_call_proof_matches_cli_and_lsp() {
    let source = "module M\n  # @return [String]\n  def foo; end\n\n  # @return [Integer]\n  def foo; end\nend\n\ninclude M\nfoo\n";
    let project = tempfile::tempdir().expect("temporary type-parity project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("type-parity fixture must be written");

    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must finalize the reopened implicit-call proof");
    let call = check_report
        .inferred_types
        .iter()
        .find(|inferred| {
            inferred.kind == CheckTypeSubjectKind::Expression
                && inferred.range.start.line == 10
                && inferred.range.start.column == 1
                && inferred.range.end.column == 4
        })
        .unwrap_or_else(|| {
            panic!(
                "the CLI must retain the finalized reopened implicit-call type, got {:#?}",
                check_report.inferred_types
            )
        });
    assert_eq!(
        call.outcome,
        CheckTypeOutcome::Proven {
            type_label: "(Integer | String)".to_string(),
        },
        "the CLI must use the same exhaustive reopened-method proof as LSP hover"
    );

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let hover = editor
        .hover_at("main.rb", 9, 1)
        .await
        .expect("the reopened implicit call must have hover output");
    let actual = hover_text(hover);
    assert!(
        actual.contains("Integer | String"),
        "LSP hover must consume the same finalized proof as CLI, got `{actual}`"
    );
}

#[tokio::test]
async fn reopened_explicit_call_proof_matches_cli_and_lsp() {
    let source = "class Choice\n  # @return [String]\n  def value; end\n\n  # @return [Integer]\n  def value; end\nend\n\nChoice.new.value\n";
    let project = tempfile::tempdir().expect("temporary type-parity project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("type-parity fixture must be written");

    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must finalize the reopened explicit-call proof");
    let call = check_report
        .inferred_types
        .iter()
        .find(|inferred| {
            inferred.kind == CheckTypeSubjectKind::Expression
                && inferred.range.start.line == 9
                && inferred.range.start.column == 1
                && inferred.range.end.column == 17
        })
        .unwrap_or_else(|| {
            panic!(
                "the CLI must retain the finalized reopened explicit-call type, got {:#?}",
                check_report.inferred_types
            )
        });
    assert_eq!(
        call.outcome,
        CheckTypeOutcome::Proven {
            type_label: "(Integer | String)".to_string(),
        },
        "the CLI must use the same exhaustive visible-method proof as LSP hover"
    );

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let hover = editor
        .hover_at("main.rb", 8, 12)
        .await
        .expect("the reopened explicit call must have hover output");
    let actual = hover_text(hover);
    assert!(
        actual.contains("Integer | String"),
        "LSP hover must consume the same finalized proof as CLI, got `{actual}`"
    );
}

#[tokio::test]
async fn reopened_private_explicit_call_remains_unknown_in_cli_and_lsp() {
    let source = "class Secret\n  private\n\n  # @return [String]\n  def value; end\n\n  # @return [Integer]\n  def value; end\nend\n\nSecret.new.value\n";
    let project = tempfile::tempdir().expect("temporary type-parity project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("type-parity fixture must be written");

    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must retain the inaccessible-call proof failure");
    let call = check_report
        .inferred_types
        .iter()
        .find(|inferred| {
            inferred.kind == CheckTypeSubjectKind::Expression
                && inferred.range.start.line == 11
                && inferred.range.start.column == 1
                && inferred.range.end.column == 17
        })
        .unwrap_or_else(|| {
            panic!(
                "the CLI must retain the exact inaccessible-call outcome, got {:#?}",
                check_report.inferred_types
            )
        });
    assert_eq!(
        call.outcome,
        CheckTypeOutcome::Unknown {
            reason: UnknownReason::UnresolvedMethodReturn,
        },
        "an explicit call must not publish a union after visibility removes candidates"
    );

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let hover = editor
        .hover_at("main.rb", 10, 12)
        .await
        .expect("the inaccessible call must retain hover context");
    let actual = hover_text(hover);
    assert!(
        actual.contains("Unknown[unresolved_method_return]"),
        "LSP hover must project the same visibility proof failure as CLI, got `{actual}`"
    );
}

#[tokio::test]
async fn incomplete_union_call_reason_matches_cli_and_lsp() {
    let source = "class Choice\n  def value(flag)\n    if flag\n      \"text\"\n    else\n      1\n    end\n  end\nend\n\nChoice.new.value(true).length\n";
    let project = tempfile::tempdir().expect("temporary type-parity project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("type-parity fixture must be written");

    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must explain the incomplete union call");
    let call = check_report
        .inferred_types
        .iter()
        .find(|inferred| {
            inferred.kind == CheckTypeSubjectKind::Expression
                && inferred.range.start.line == 11
                && inferred.range.start.column == 1
                && inferred.range.end.column == 30
        })
        .unwrap_or_else(|| {
            panic!(
                "the CLI must retain the exact Unknown outcome for the union call, got {:#?}",
                check_report.inferred_types
            )
        });
    assert_eq!(
        call.outcome,
        CheckTypeOutcome::Unknown {
            reason: UnknownReason::IncompleteUnionMember,
        }
    );

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let hover = editor
        .hover_at("main.rb", 10, 24)
        .await
        .expect("the incomplete union call must retain hover context");
    let actual = hover_text(hover);
    assert!(
        actual.contains("Unknown[incomplete_union_member]"),
        "LSP hover must project the same union proof failure as CLI, got `{actual}`"
    );
}

fn find_cli_diagnostic<'a>(report: &'a CheckReport, code: &str) -> &'a CheckDiagnostic {
    report
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.as_deref() == Some(code))
        .unwrap_or_else(|| {
            panic!(
                "check must return the proven `{code}` diagnostic; got {:?}",
                report.diagnostics
            )
        })
}

fn find_lsp_diagnostic<'a>(
    diagnostics: &'a [tower_lsp::lsp_types::Diagnostic],
    code: &str,
) -> &'a tower_lsp::lsp_types::Diagnostic {
    diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code.as_ref().is_some_and(|code_value| {
                matches!(code_value, NumberOrString::String(value) if value == code)
            })
        })
        .unwrap_or_else(|| {
            panic!("LSP must return the proven `{code}` diagnostic; got {diagnostics:?}")
        })
}

fn assert_diagnostic_parity(
    check_diagnostic: &CheckDiagnostic,
    lsp_diagnostic: &tower_lsp::lsp_types::Diagnostic,
) {
    assert_eq!(check_diagnostic.message, lsp_diagnostic.message);
    assert_eq!(
        (
            check_diagnostic.range.start.line,
            check_diagnostic.range.start.column,
            check_diagnostic.range.end.line,
            check_diagnostic.range.end.column,
        ),
        (
            lsp_diagnostic.range.start.line + 1,
            lsp_diagnostic.range.start.character + 1,
            lsp_diagnostic.range.end.line + 1,
            lsp_diagnostic.range.end.character + 1,
        ),
        "INVARIANT VIOLATED: CLI and LSP projected different ranges for one engine-owned \
         diagnostic. This is a bug because adapters may change indexing conventions but not \
         semantic locations. Fix: keep check range conversion aligned with LSP UTF-16 positions."
    );
}

#[tokio::test]
async fn unresolved_method_diagnostic_matches_cli_and_lsp() {
    let source = "class User\n  def name\n    \"x\"\n  end\nend\n\nu = User.new\nu.naem\n";
    let project = tempfile::tempdir().expect("temporary parity project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("parity fixture must be written");
    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the unresolved-method fixture");
    let check_diagnostic = find_cli_diagnostic(&check_report, "unresolved-method");

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let published = editor.diagnostics("main.rb").await;
    let lsp_diagnostic = find_lsp_diagnostic(&published, "unresolved-method");
    assert!(
        check_diagnostic.message.contains("Unresolved method `new` on `User`"),
        "CLI and LSP must flag the unresolved `new` call identically, got `{}`",
        check_diagnostic.message
    );
    assert_diagnostic_parity(check_diagnostic, lsp_diagnostic);

    let check_naem = check_report
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code.as_deref() == Some("unresolved-method")
                && diagnostic.message.contains("Did you mean `name`?")
        })
        .unwrap_or_else(|| {
            panic!(
                "check must suggest `name` for `naem`, got {:?}",
                check_report.diagnostics
            )
        });
    let lsp_naem = published
        .iter()
        .find(|diagnostic| {
            matches!(&diagnostic.code, Some(NumberOrString::String(code)) if code == "unresolved-method")
                && diagnostic.message.contains("Did you mean `name`?")
        })
        .unwrap_or_else(|| panic!("LSP must suggest `name` for `naem`, got {published:?}"));
    assert_diagnostic_parity(check_naem, lsp_naem);
}

#[tokio::test]
async fn unresolved_constant_diagnostic_matches_cli_and_lsp() {
    let source = "UnknownThing.new\n";
    let project = tempfile::tempdir().expect("temporary parity project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("parity fixture must be written");
    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the unresolved-constant fixture");
    let check_diagnostic = find_cli_diagnostic(&check_report, "unresolved-constant");

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let published = editor.diagnostics("main.rb").await;
    let lsp_diagnostic = find_lsp_diagnostic(&published, "unresolved-constant");
    assert_diagnostic_parity(check_diagnostic, lsp_diagnostic);
}

#[tokio::test]
async fn missing_kwarg_diagnostic_matches_cli_and_lsp() {
    let source = "def greet(name:, age: 0)\n  name\nend\n\ngreet(age: 30)\n";
    let project = tempfile::tempdir().expect("temporary parity project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("parity fixture must be written");
    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the missing-kwarg fixture");
    let check_diagnostic = find_cli_diagnostic(&check_report, "missing-kwarg");

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let published = editor.diagnostics("main.rb").await;
    let lsp_diagnostic = find_lsp_diagnostic(&published, "missing-kwarg");
    assert!(
        check_diagnostic.message.contains("`name:`"),
        "the shared engine must name the missing keyword argument, got `{}`",
        check_diagnostic.message
    );
    assert_diagnostic_parity(check_diagnostic, lsp_diagnostic);
}

#[tokio::test]
async fn yard_unknown_param_diagnostic_matches_cli_and_lsp() {
    let source = "# @param ghost [String]\ndef actual\nend\n";
    let project = tempfile::tempdir().expect("temporary parity project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("parity fixture must be written");
    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the unmatched YARD parameter fixture");
    let check_diagnostic = find_cli_diagnostic(&check_report, "yard-unknown-param");

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let published = editor.diagnostics("main.rb").await;
    let lsp_diagnostic = find_lsp_diagnostic(&published, "yard-unknown-param");
    assert_diagnostic_parity(check_diagnostic, lsp_diagnostic);
}

#[tokio::test]
async fn yard_rbs_mismatch_diagnostic_matches_cli_and_lsp() {
    let source = "class String\n  # @return [String]\n  def length\n    1\n  end\nend\n";
    let project = tempfile::tempdir().expect("temporary parity project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("parity fixture must be written");
    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the YARD/RBS mismatch fixture");
    let check_diagnostic = find_cli_diagnostic(&check_report, "yard-rbs-mismatch");
    assert!(
        check_diagnostic.message.contains("conflicts with RBS type"),
        "the shared engine must explain the conflicting RBS type, got `{}`",
        check_diagnostic.message
    );

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let published = editor.diagnostics("main.rb").await;
    let lsp_diagnostic = find_lsp_diagnostic(&published, "yard-rbs-mismatch");
    assert_diagnostic_parity(check_diagnostic, lsp_diagnostic);
}

#[tokio::test]
async fn unresolved_require_diagnostic_matches_cli_and_lsp() {
    let project = tempfile::tempdir().expect("temporary parity project must be created");
    std::fs::write(project.path().join("main.rb"), "require \"missing\"\n")
        .expect("parity fixture must be written");
    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the unresolved-require fixture");
    let check_diagnostic = find_cli_diagnostic(&check_report, "unresolved-require");

    let mut editor = FakeEditor::new().await;
    editor.add_workspace("project");
    editor.open("project/main.rb", "require \"missing\"\n").await;
    let published = editor.diagnostics("project/main.rb").await;
    let lsp_diagnostic = find_lsp_diagnostic(&published, "unresolved-require");
    assert_diagnostic_parity(check_diagnostic, lsp_diagnostic);
}

#[tokio::test]
async fn syntax_diagnostic_matches_cli_and_lsp() {
    let source = "def broken(\n";
    let project = tempfile::tempdir().expect("temporary parity project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("parity fixture must be written");
    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the syntax-error fixture");
    let mut check_syntax = check_report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.is_none())
        .map(|diagnostic| {
            (
                diagnostic.range,
                diagnostic.message.clone(),
            )
        })
        .collect::<Vec<_>>();
    assert!(
        check_syntax.len() >= 1,
        "check must return the parser syntax diagnostics, got {:?}",
        check_report.diagnostics
    );

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let mut lsp_syntax = editor
        .diagnostics("main.rb")
        .await
        .into_iter()
        .filter(|diagnostic| diagnostic.code.is_none())
        .map(|diagnostic| {
            (
                crate::check::CheckRange {
                    start: crate::check::CheckPosition {
                        line: diagnostic.range.start.line + 1,
                        column: diagnostic.range.start.character + 1,
                    },
                    end: crate::check::CheckPosition {
                        line: diagnostic.range.end.line + 1,
                        column: diagnostic.range.end.character + 1,
                    },
                },
                diagnostic.message,
            )
        })
        .collect::<Vec<_>>();
    check_syntax.sort();
    lsp_syntax.sort();
    assert_eq!(
        check_syntax, lsp_syntax,
        "CLI and LSP must project the same parser syntax diagnostics for one byte-identical file"
    );
}

#[tokio::test]
async fn multi_diagnostic_file_keeps_deterministic_cli_lsp_parity() {
    let source = "def greet(name:, age: 0)\n  name\nend\n\nUnknownThing.new\ngreet(age: 30)\n";
    let project = tempfile::tempdir().expect("temporary parity project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("parity fixture must be written");
    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the multi-diagnostic fixture");
    let mut check_codes = check_report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.is_some())
        .map(|diagnostic| {
            (
                diagnostic.range,
                diagnostic.code.as_deref().unwrap().to_string(),
                diagnostic.message.clone(),
            )
        })
        .collect::<Vec<_>>();

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let mut lsp_codes = editor
        .diagnostics("main.rb")
        .await
        .into_iter()
        .filter_map(|diagnostic| {
            let code = match diagnostic.code {
                Some(NumberOrString::String(code)) => code,
                _ => return None,
            };
            Some((
                crate::check::CheckRange {
                    start: crate::check::CheckPosition {
                        line: diagnostic.range.start.line + 1,
                        column: diagnostic.range.start.character + 1,
                    },
                    end: crate::check::CheckPosition {
                        line: diagnostic.range.end.line + 1,
                        column: diagnostic.range.end.character + 1,
                    },
                },
                code,
                diagnostic.message,
            ))
        })
        .collect::<Vec<_>>();

    check_codes.sort();
    lsp_codes.sort();
    assert!(
        check_codes.len() >= 2,
        "the fixture must produce multiple engine-owned diagnostics, got {check_codes:?}"
    );
    assert_eq!(
        check_codes, lsp_codes,
        "CLI and LSP must project the same deterministic diagnostic set for one file"
    );
}
