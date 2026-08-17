use crate::check::{
    CheckDiagnostic, CheckReport, CheckSession, CheckTypeOutcome, CheckTypeSubjectKind,
};
use crate::indexer::file_processor::FileProcessor;
use crate::test::harness::FakeEditor;
use ruby_analysis::{RubyType, UnknownReason};
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
async fn higher_order_call_types_match_headless_and_lsp_projection() {
    let source = "strings = [1, 2].map { |value| value.to_s }\nstrings.first.upcase\n";
    let project = tempfile::tempdir().expect("temporary callable parity project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("callable parity fixture must be written");

    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the callable parity fixture");
    assert!(
        check_report.inferred_types.iter().any(|inferred| {
            inferred.subject == "strings"
                && inferred.outcome
                    == CheckTypeOutcome::Proven {
                        type_label: "Array<String>".to_string(),
                    }
        }),
        "the headless checker must consume the same higher-order result: {:#?}",
        check_report.inferred_types
    );

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    assert!(editor.inlay_hints("main.rb").await.iter().any(|hint| {
        matches!(&hint.label, InlayHintLabel::String(label) if label == ": Array<String>")
    }));
    let hover = editor
        .hover_at("main.rb", 1, 14)
        .await
        .expect("the chained String call must have hover proof");
    assert!(hover_text(hover).contains("String"));
}

#[tokio::test]
async fn callable_body_types_match_headless_and_lsp_projection() {
    let source =
        "stringify = ->(value) { value.to_s }\nresult = stringify.call(1)\nresult.upcase\n";
    let project = tempfile::tempdir().expect("temporary callable-body project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("callable-body parity fixture must be written");

    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the callable-body fixture");
    assert!(
        check_report.inferred_types.iter().any(|inferred| {
            inferred.subject == "result"
                && inferred.outcome
                    == CheckTypeOutcome::Proven {
                        type_label: "String".to_string(),
                    }
        }),
        "headless callable-body result diverged from engine facts: {:#?}",
        check_report.inferred_types
    );

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    assert!(editor.inlay_hints("main.rb").await.iter().any(|hint| {
        matches!(&hint.label, InlayHintLabel::String(label) if label == ": String")
    }));
    let hover = editor
        .hover_at("main.rb", 2, 9)
        .await
        .expect("chained callable result must have hover proof");
    assert!(hover_text(hover).contains("String"));
}

#[tokio::test]
async fn callable_body_failures_keep_stable_engine_owned_unknown_reasons() {
    let project = tempfile::tempdir().expect("temporary callable-body failure project");
    std::fs::write(
        project.path().join("main.rb"),
        "convert = ->(value) { value.to_s }\nconsume(convert)\nresult = convert.call(1)\n",
    )
    .expect("callable-body failure fixture must be written");

    let report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must retain callable-body failure reasons");
    assert!(
        report.inferred_types.iter().any(|inferred| {
            inferred.kind == CheckTypeSubjectKind::Expression
                && inferred.outcome
                    == CheckTypeOutcome::Unknown {
                        reason: UnknownReason::EscapedCallableValue,
                    }
        }),
        "missing escaped callable outcome: {:#?}",
        report.inferred_types
    );
}

#[tokio::test]
async fn callable_body_capture_and_recursion_reasons_are_stable_in_check_output() {
    let project = tempfile::tempdir().expect("temporary callable-body reason project");
    std::fs::write(
        project.path().join("main.rb"),
        r#"prefix = "item"
captured = -> { prefix }
prefix = dynamic_value
capture_result = captured.call

recursive = ->(value) { recursive.call(value) }
recursive_result = recursive.call(1)
"#,
    )
    .expect("callable-body reason fixture must be written");

    let report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must retain callable-body reasons");
    for expected in [
        UnknownReason::IncompleteCallableCapture,
        UnknownReason::CallableRecursionUnsupported,
    ] {
        assert!(
            report.inferred_types.iter().any(|inferred| {
                inferred.kind == CheckTypeSubjectKind::Expression
                    && inferred.outcome == CheckTypeOutcome::Unknown { reason: expected }
            }),
            "missing callable-body reason {}: {:#?}",
            expected.code(),
            report.inferred_types
        );
    }
}

#[tokio::test]
async fn higher_order_failures_keep_stable_engine_owned_unknown_reasons() {
    let project = tempfile::tempdir().expect("temporary callable failure project must be created");
    std::fs::write(
        project.path().join("main.rb"),
        r#"
callable = dynamic_value
dynamic_result = [1, 2].map(&callable)
flow_result = [1, 2].map { |value| break value.to_s }
"#,
    )
    .expect("callable failure fixture must be written");

    let report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must retain higher-order failure reasons");
    assert!(
        report.inferred_types.iter().any(|inferred| {
            inferred.kind == CheckTypeSubjectKind::Expression
                && inferred.outcome
                    == CheckTypeOutcome::Unknown {
                        reason: UnknownReason::UnsupportedCallable,
                    }
        }),
        "missing unsupported callable outcome: {:#?}",
        report.inferred_types
    );
    assert!(report.inferred_types.iter().any(|inferred| {
        inferred.kind == CheckTypeSubjectKind::Expression
            && inferred.outcome
                == CheckTypeOutcome::Unknown {
                    reason: UnknownReason::UnsupportedBlockFlow,
                }
    }));
}

#[tokio::test]
async fn conflicting_callable_overloads_fail_closed_with_a_stable_reason() {
    let project = tempfile::tempdir().expect("temporary overload project must be created");
    let signature_dir = project.path().join("sig");
    std::fs::create_dir_all(&signature_dir).expect("signature directory must be created");
    std::fs::write(
        signature_dir.join("converter.rbs"),
        r#"class Converter
  def apply: [Input, Output] (Input value) { (Input) -> Output } -> Output
           | [Input, Output] (Input value) { (Input) -> Output } -> Array[Output]
end
"#,
    )
    .expect("conflicting callable signatures must be written");
    std::fs::write(
        project.path().join("main.rb"),
        "result = Converter.new.apply(1) { |value| value.to_s }\n",
    )
    .expect("overload consumer must be written");

    let report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the conflicting callable overloads");
    assert!(
        report.inferred_types.iter().any(|inferred| {
            inferred.kind == CheckTypeSubjectKind::Expression
                && inferred.outcome
                    == CheckTypeOutcome::Unknown {
                        reason: UnknownReason::AmbiguousCallableOverload,
                    }
        }),
        "missing ambiguous callable outcome: {:#?}",
        report.inferred_types
    );
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
async fn structural_shape_types_match_cli_hover_and_inlay_projection() {
    let source =
        "payload = { id: 1, profile: { name: \"Ada\", active: true } }\npayload[:profile][:name]\n";
    let expected_shape = "{ id: Integer, profile: { active: TrueClass, name: String } }";
    let project = tempfile::tempdir().expect("temporary shape-parity project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("shape-parity fixture must be written");

    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze structural shapes");
    let payload = check_report
        .inferred_types
        .iter()
        .find(|inferred| {
            inferred.kind == CheckTypeSubjectKind::Local && inferred.subject == "payload"
        })
        .expect("the CLI must publish the shape-valued local assignment");
    assert_eq!(
        payload.outcome,
        CheckTypeOutcome::Proven {
            type_label: expected_shape.to_string(),
        }
    );
    assert!(
        check_report.inferred_types.iter().any(|inferred| {
            inferred.kind == CheckTypeSubjectKind::Expression
                && inferred.range.start.line == 2
                && inferred.outcome
                    == CheckTypeOutcome::Proven {
                        type_label: "String".to_string(),
                    }
        }),
        "the CLI must publish the final keyed-read String proof: {:?}",
        check_report.inferred_types
    );
    assert!(check_report.inference.retained_shape_occurrences > 0);
    assert!(check_report.inference.retained_shape_fields > 0);
    assert_eq!(check_report.inference.max_retained_shape_fields, 2);
    assert_eq!(check_report.inference.max_retained_shape_depth, 2);

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    assert!(
        editor.inlay_hints("main.rb").await.into_iter().any(|hint| {
            matches!(
                hint.label,
                InlayHintLabel::String(ref label) if label == &format!(": {expected_shape}")
            )
        }),
        "LSP inlay hints must render the same canonical shape as the CLI"
    );
    let payload_hover = hover_text(
        editor
            .hover_at("main.rb", 0, 2)
            .await
            .expect("the shape local must have hover output"),
    );
    assert!(
        payload_hover.contains(expected_shape),
        "LSP hover must render the same canonical shape as the CLI, got `{payload_hover}`"
    );
    let read_hover = hover_text(
        editor
            .hover_at("main.rb", 1, 24)
            .await
            .expect("the nested keyed read must have hover output"),
    );
    assert!(
        read_hover.contains("String"),
        "LSP hover must consume the same keyed-read proof as CLI, got `{read_hover}`"
    );
}

#[tokio::test]
async fn invalidated_shape_reason_matches_cli_and_lsp() {
    let source = "payload = { count: 1 }\ndynamic_sink(payload)\npayload[:count]\n";
    let project = tempfile::tempdir().expect("temporary shape-parity project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("shape-parity fixture must be written");

    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must retain invalidated shape evidence");
    let read = check_report
        .inferred_types
        .iter()
        .find(|inferred| {
            inferred.kind == CheckTypeSubjectKind::Expression && inferred.range.start.line == 3
        })
        .expect("the CLI must retain the invalidated keyed-read outcome");
    assert_eq!(
        read.outcome,
        CheckTypeOutcome::Unknown {
            reason: UnknownReason::MutableShapeInvalidated,
        }
    );
    assert!(
        check_report.inference.shape_invalidated_outcomes > 0,
        "the headless report must count retained mutable-shape invalidations"
    );

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let hover = hover_text(
        editor
            .hover_at("main.rb", 2, 15)
            .await
            .expect("the invalidated keyed read must retain hover context"),
    );
    assert!(
        hover.contains("Unknown[mutable_shape_invalidated]"),
        "LSP hover must project the same invalidation reason as CLI, got `{hover}`"
    );
}

#[tokio::test]
async fn shape_telemetry_reports_alias_and_bound_observations() {
    let fields = (0..33)
        .map(|index| format!("field_{index}: {index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let source = format!(
        r#"class PayloadFactory
  def build
    payload = {{ count: 1 }}
    copy = payload
    copy[:count] = "many"
    payload
  end

  def too_wide
    payload = {{ {fields} }}
    payload
  end
end
"#
    );
    let project = tempfile::tempdir().expect("temporary shape-telemetry project must be created");
    std::fs::write(project.path().join("payload_factory.rb"), source)
        .expect("shape-telemetry fixture must be written");

    let report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must aggregate file-owned shape telemetry");

    assert_eq!(report.inference.max_live_shape_aliases, 2);
    assert!(
        report.inference.shape_bound_exceeded_outcomes > 0,
        "the rejected 33-field shape must remain measurable as a bound-triggered Unknown; telemetry={:#?}, types={:#?}",
        report.inference,
        report.inferred_types
    );
}

#[tokio::test]
async fn structural_return_diagnostics_match_cli_and_lsp_and_fail_closed() {
    let signature = "class PayloadFactory\n  def build: () -> { id: Integer }\nend\n";
    let mismatched = "class PayloadFactory\n  def build\n    { id: \"wrong\" }\n  end\nend\n";
    let incomplete = "class PayloadFactory\n  def build\n    { id: dynamic_value }\n  end\nend\n";
    let project = tempfile::tempdir().expect("temporary structural-parity project must be created");
    let sig_dir = project.path().join("sig");
    std::fs::create_dir(&sig_dir).expect("the synthetic sig directory must be created");
    std::fs::write(sig_dir.join("payload_factory.rbs"), signature)
        .expect("the synthetic RBS contract must be written");
    let implementation = project.path().join("payload_factory.rb");
    std::fs::write(&implementation, mismatched)
        .expect("the mismatched Ruby implementation must be written");

    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must evaluate the structural return contract");
    let mut editor = FakeEditor::new().await;
    let signature_uri = Url::parse("file:///sig/payload_factory.rbs")
        .expect("the synthetic signature URI must be valid");
    FileProcessor::default()
        .collect_rbs_facts(&signature_uri, signature, editor.server())
        .expect("the RBS contract must enter the LSP engine");
    editor.open("payload_factory.rb", mismatched).await;
    let lsp_diagnostics = editor.diagnostics("payload_factory.rb").await;
    assert_diagnostic_parity(
        find_cli_diagnostic(&check_report, "declared-return-type-mismatch"),
        find_lsp_diagnostic(&lsp_diagnostics, "declared-return-type-mismatch"),
    );

    std::fs::write(&implementation, incomplete)
        .expect("the incomplete Ruby implementation must replace the mismatch");
    let incomplete_check = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must evaluate incomplete structural evidence");
    editor.set("payload_factory.rb", incomplete).await;
    let incomplete_lsp = editor.diagnostics("payload_factory.rb").await;
    assert!(
        incomplete_check.diagnostics.iter().all(|diagnostic| {
            diagnostic.code.as_deref() != Some("declared-return-type-mismatch")
        }),
        "CLI must not diagnose structural incompatibility from incomplete evidence: {:?}",
        incomplete_check.diagnostics
    );
    assert!(
        incomplete_lsp.iter().all(|diagnostic| {
            !matches!(
                &diagnostic.code,
                Some(NumberOrString::String(code)) if code == "declared-return-type-mismatch"
            )
        }),
        "LSP must not diagnose structural incompatibility from incomplete evidence: {incomplete_lsp:?}"
    );
}

#[tokio::test]
async fn embedded_core_value_constant_chain_matches_cli_and_lsp() {
    let source = "ARGV.first.upcase\n";
    let project = tempfile::tempdir().expect("temporary core-constant project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("core-constant parity fixture must be written");

    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must load embedded core runtime constants");
    assert!(
        check_report.diagnostics.iter().all(|diagnostic| {
            !matches!(
                diagnostic.code.as_deref(),
                Some("unresolved-constant" | "unresolved-method")
            )
        }),
        "a proven core runtime value chain must not produce absence diagnostics: {:?}",
        check_report.diagnostics
    );
    let cli_type = check_report
        .inferred_types
        .iter()
        .find(|inferred| {
            inferred.kind == CheckTypeSubjectKind::Expression
                && inferred.range.start.line == 1
                && inferred.range.start.column == 1
                && inferred.range.end.column == 18
        })
        .unwrap_or_else(|| {
            panic!(
                "CLI must retain the outer ARGV chain outcome, got {:#?}",
                check_report.inferred_types
            )
        });
    assert_eq!(
        cli_type.outcome,
        CheckTypeOutcome::Proven {
            type_label: "String".to_string(),
        }
    );

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let hover = editor
        .hover_at("main.rb", 0, 14)
        .await
        .expect("proven ARGV chain must have LSP hover output");
    let hover = hover_text(hover);
    assert!(
        hover.contains("String"),
        "LSP hover must project the same proven String type as CLI, got `{hover}`"
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
async fn unresolved_local_binding_reason_matches_cli_and_lsp() {
    let source = "value = dynamic_value\nvalue\n";
    let project = tempfile::tempdir().expect("temporary type-parity project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("type-parity fixture must be written");

    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must explain the unresolved local binding");
    let read = check_report
        .inferred_types
        .iter()
        .find(|inferred| {
            inferred.kind == CheckTypeSubjectKind::Expression
                && inferred.range.start.line == 2
                && inferred.range.start.column == 1
                && inferred.range.end.column == 6
        })
        .expect("the CLI must retain the exact Unknown outcome for the local read");
    assert_eq!(
        read.outcome,
        CheckTypeOutcome::Unknown {
            reason: UnknownReason::UnresolvedAssignmentValue,
        }
    );

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let hover = editor
        .hover_at("main.rb", 1, 2)
        .await
        .expect("the unresolved local read must retain hover context");
    let actual = hover_text(hover);
    assert!(
        actual.contains("Unknown[unresolved_assignment_value]"),
        "LSP hover must project the same local-binding failure as CLI, got `{actual}`"
    );

    let proven_source = "value = \"ready\"\nvalue.missing\n";
    editor.set("main.rb", proven_source).await;
    let proven_hover = editor
        .hover_at("main.rb", 1, 2)
        .await
        .expect("the proven local read must have hover output after replacement");
    assert!(
        hover_text(proven_hover).contains("String"),
        "replacing the Unknown binding must remove its stale reason and expose the proven local type even when its enclosing call is unresolved"
    );

    editor.set("main.rb", source).await;
    let unknown_again = editor
        .hover_at("main.rb", 1, 2)
        .await
        .expect("the unresolved local read must regain its exact reason after replacement");
    assert!(
        hover_text(unknown_again).contains("Unknown[unresolved_assignment_value]"),
        "restoring the unresolved binding must not reuse the stale concrete type"
    );
}

#[tokio::test]
async fn unmatched_case_path_preserves_cli_and_lsp_flow_type_parity() {
    let source = r#"class Picker
  def choose(flag)
    value = 1
    case flag
    when true
      value = "ready"
    end
    value
  end
end
"#;
    let project = tempfile::tempdir().expect("temporary case-flow project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("case-flow parity fixture must be written");

    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must retain the unmatched case path");
    let method_return = check_report
        .inferred_types
        .iter()
        .find(|inferred| {
            inferred.kind == CheckTypeSubjectKind::MethodReturn
                && inferred.subject == "Picker#choose"
        })
        .unwrap_or_else(|| {
            panic!(
                "the CLI must publish Picker#choose's solved return, got {:#?}",
                check_report.inferred_types
            )
        });
    assert_eq!(
        method_return.outcome,
        CheckTypeOutcome::Proven {
            type_label: "(Integer | String)".to_string(),
        },
        "the unmatched path must keep the Integer binding that reaches the case"
    );

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let hover = editor
        .hover_at("main.rb", 7, 6)
        .await
        .expect("the joined local read must have hover output");
    let actual = hover_text(hover);
    assert!(
        actual.contains("Integer | String"),
        "LSP hover must consume the same exhaustive join as the CLI, got `{actual}`"
    );
    assert!(
        editor.inlay_hints("main.rb").await.into_iter().any(|hint| {
            matches!(
                hint.label,
                InlayHintLabel::String(ref label) if label == " -> (Integer | String)"
            )
        }),
        "the method-return inlay must project the same joined engine type"
    );
}

#[tokio::test]
async fn unmatched_case_join_blocks_unsound_local_chained_call_resolution() {
    let source = r#"class Picker
  def normalize(flag)
    value = 1
    case flag
    when true
      value = "ready"
    end
    value.upcase
  end
end
"#;
    let project = tempfile::tempdir().expect("temporary case-chain project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("case-chain parity fixture must be written");

    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must fail closed on the joined local receiver");
    let call = check_report
        .inferred_types
        .iter()
        .find(|inferred| {
            inferred.kind == CheckTypeSubjectKind::Expression
                && inferred.range.start.line == 8
                && inferred.range.start.column == 5
                && inferred.range.end.column == 17
        })
        .unwrap_or_else(|| {
            panic!(
                "the CLI must retain the local union call outcome, got {:#?}",
                check_report.inferred_types
            )
        });
    assert_eq!(
        call.outcome,
        CheckTypeOutcome::Unknown {
            reason: UnknownReason::IncompleteUnionMember,
        },
        "String#upcase cannot be selected while Integer remains a reachable receiver"
    );

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let hover = editor
        .hover_at("main.rb", 7, 12)
        .await
        .expect("the incomplete local-union call must retain hover context");
    let actual = hover_text(hover);
    assert!(
        actual.contains("Unknown[incomplete_union_member]"),
        "LSP hover must fail closed with the same reason as CLI, got `{actual}`"
    );
}

#[tokio::test]
async fn unmatched_pattern_case_uses_only_reaching_flow_types_across_cli_and_lsp() {
    let source = r#"class Picker
  def normalize
    case { name: "Ada" }
    in { name: value }
      value
    end
    value.upcase
  end
end
"#;
    let project = tempfile::tempdir().expect("temporary pattern-flow project must be created");
    std::fs::write(project.path().join("main.rb"), source)
        .expect("pattern-flow parity fixture must be written");

    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must retain the only reaching pattern-match path");
    let method_return = check_report
        .inferred_types
        .iter()
        .find(|inferred| {
            inferred.kind == CheckTypeSubjectKind::MethodReturn
                && inferred.subject == "Picker#normalize"
        })
        .unwrap_or_else(|| {
            panic!(
                "the CLI must publish Picker#normalize's solved return, got {:#?}",
                check_report.inferred_types
            )
        });
    assert_eq!(
        method_return.outcome,
        CheckTypeOutcome::Proven {
            type_label: "String".to_string(),
        },
        "the unmatched pattern path raises and cannot add NilClass to the receiver"
    );

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let local_hover = editor
        .hover_at("main.rb", 6, 6)
        .await
        .expect("the post-pattern local read must have hover output");
    assert!(
        hover_text(local_hover).contains("String"),
        "LSP hover must consume the same reaching-path proof as the CLI"
    );
    let call_hover = editor
        .hover_at("main.rb", 6, 12)
        .await
        .expect("the resolved chained call must have hover output");
    assert!(
        hover_text(call_hover).contains("String"),
        "the proven local receiver must resolve String#upcase"
    );
    assert!(
        editor
            .complete_at("main.rb", 6, 12)
            .await
            .iter()
            .any(|item| item.label == "upcase"),
        "completion must use the same proven post-pattern receiver type"
    );
    assert!(
        editor.inlay_hints("main.rb").await.into_iter().any(|hint| {
            matches!(
                hint.label,
                InlayHintLabel::String(ref label) if label == " -> String"
            )
        }),
        "the method-return inlay must publish the shared solved type"
    );

    let explicit_else_source = r#"class Picker
  def normalize
    case { name: "Ada" }
    in { name: value }
      value
    else
      nil
    end
    value.upcase
  end
end
"#;
    editor.set("main.rb", explicit_else_source).await;
    let incomplete_hover = editor
        .hover_at("main.rb", 8, 14)
        .await
        .expect("the explicit-else call must retain its proof-failure hover");
    assert!(
        hover_text(incomplete_hover).contains("Unknown[incomplete_union_member]"),
        "a reachable else without the capture must invalidate String#upcase"
    );
    assert!(
        editor
            .complete_at("main.rb", 8, 12)
            .await
            .iter()
            .all(|item| item.label != "upcase"),
        "completion must not reuse the stale no-else receiver proof"
    );

    editor.set("main.rb", source).await;
    let restored_hover = editor
        .hover_at("main.rb", 6, 12)
        .await
        .expect("restoring the raising unmatched path must restore call hover");
    assert!(
        hover_text(restored_hover).contains("String"),
        "the no-else proof must be reproducible after invalidation"
    );
}

#[tokio::test]
async fn unresolved_flow_join_blocks_every_receiver_consumer_after_edit() {
    let proven_source = r#"class Product
  def label(prefix)
    "label"
  end
end

class Picker
  def normalize
    value = Product.new
    value.label("x")
  end
end
"#;
    let unresolved_source = r#"class Product
  def label(prefix)
    "label"
  end
end

class Picker
  def normalize(flag)
    if flag
      value = dynamic_value
    else
      value = Product.new
    end
    value.label("x")
  end
end
"#;
    let project = tempfile::tempdir().expect("temporary flow-proof project must be created");
    std::fs::write(project.path().join("main.rb"), unresolved_source)
        .expect("flow-proof parity fixture must be written");
    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must retain the unresolved flow join");
    let local_read = check_report
        .inferred_types
        .iter()
        .find(|inferred| {
            inferred.kind == CheckTypeSubjectKind::Expression
                && inferred.range.start.line == 14
                && inferred.range.start.column == 5
                && inferred.range.end.column == 10
        })
        .unwrap_or_else(|| {
            panic!(
                "the CLI must publish the post-join local proof failure, got {:#?}",
                check_report.inferred_types
            )
        });
    assert_eq!(
        local_read.outcome,
        CheckTypeOutcome::Unknown {
            reason: UnknownReason::UnresolvedAssignmentValue,
        },
        "one unresolved reaching branch must block the later concrete syntactic assignment"
    );

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", proven_source).await;
    assert_eq!(
        editor.goto_def_at("main.rb", 9, 12).await.len(),
        1,
        "the initial concrete receiver must resolve Product#label"
    );
    let initial_signature = editor
        .signature_help_at("main.rb", 9, 18)
        .await
        .expect("the initial concrete receiver must provide signature help");
    assert!(
        initial_signature.signatures[0]
            .label
            .starts_with("label(prefix)"),
        "the initial signature must belong to Product#label"
    );

    editor.set("main.rb", unresolved_source).await;
    let document = editor
        .server()
        .get_doc(&Url::parse("file:///main.rb").expect("test URI must be valid"))
        .expect("edited document must remain open");
    let read_position = tower_lsp::lsp_types::Position::new(13, 6);
    let read_source_position = crate::utils::lsp::source_position(read_position);
    let scope_id = document
        .find_scope_for_variable_at("value", read_source_position)
        .expect("the post-join read must retain its lexical owner");
    assert_eq!(
        document.variable_scopes().get_flow_read_type_at_position(
            "value",
            scope_id,
            document.analysis_file_id(),
            document.position_to_analysis_offset(read_source_position),
        ),
        Some(&RubyType::Unknown),
        "the document must retain the exact flow Unknown as an internal proof barrier"
    );
    let local_hover = editor
        .hover_at("main.rb", 13, 6)
        .await
        .expect("the unresolved post-join local must retain hover context");
    let local_hover_text = hover_text(local_hover);
    assert!(
        local_hover_text.contains("Unknown[unresolved_assignment_value]"),
        "local hover must expose the exact flow proof failure, got `{local_hover_text}`"
    );
    let call_hover = editor
        .hover_at("main.rb", 13, 12)
        .await
        .expect("the unresolved receiver call must retain hover context");
    assert!(
        hover_text(call_hover).contains("Unknown[unknown_receiver]"),
        "call hover must not reuse the concrete assignment from the else branch"
    );
    assert!(
        editor
            .complete_at("main.rb", 13, 12)
            .await
            .iter()
            .all(|item| item.label != "label"),
        "completion must fail closed for the unresolved exhaustive flow join"
    );
    assert!(
        editor.goto_def_at("main.rb", 13, 12).await.is_empty(),
        "navigation must not resolve Product#label through a stale syntactic assignment"
    );
    assert!(
        editor.signature_help_at("main.rb", 13, 18).await.is_none(),
        "signature help must not resolve Product#label through a stale syntactic assignment"
    );
    assert!(
        editor.inlay_hints("main.rb").await.into_iter().all(|hint| {
            hint.position.line != 7
                || !matches!(
                    hint.label,
                    InlayHintLabel::String(ref label) if label == " -> String"
                )
        }),
        "Picker#normalize must not publish the result of an unproven dispatch"
    );

    editor.set("main.rb", proven_source).await;
    assert_eq!(
        editor.goto_def_at("main.rb", 9, 12).await.len(),
        1,
        "restoring the concrete receiver must restore navigation"
    );
    assert!(
        editor.signature_help_at("main.rb", 9, 18).await.is_some(),
        "restoring the concrete receiver must restore signature help"
    );
}

#[tokio::test]
async fn short_circuit_assignment_never_becomes_an_unconditional_receiver_proof() {
    let proven_source = r#"class Product
end

class Text
  def upcase(prefix)
    "fallback"
  end
end

class Picker
  def normalize
    value = Text.new
    value.upcase("x")
  end
end
"#;
    let conditional_source = r#"class Product
end

class Text
  def upcase(prefix)
    "fallback"
  end
end

class Picker
  def normalize(flag)
    value = Product.new
    flag && (value = Text.new)
    value.upcase("x")
  end
end
"#;

    let project = tempfile::tempdir().expect("temporary short-circuit project must be created");
    std::fs::write(project.path().join("main.rb"), conditional_source)
        .expect("short-circuit parity fixture must be written");
    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must retain both short-circuit receiver paths");
    assert!(
        check_report.inferred_types.iter().any(|inferred| {
            inferred.kind == CheckTypeSubjectKind::Expression
                && inferred.outcome
                    == CheckTypeOutcome::Proven {
                        type_label: "(Product | Text)".to_string(),
                    }
        }),
        "the CLI must publish the exhaustive pre-write/right-write receiver union, got {:#?}",
        check_report.inferred_types
    );
    assert!(
        check_report.inferred_types.iter().any(|inferred| {
            inferred.kind == CheckTypeSubjectKind::Expression
                && inferred.outcome
                    == CheckTypeOutcome::Unknown {
                        reason: UnknownReason::IncompleteUnionMember,
                    }
        }),
        "the CLI must fail closed when Product does not prove Text#upcase dispatch, got {:#?}",
        check_report.inferred_types
    );
    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", proven_source).await;
    assert!(
        editor
            .complete_at("main.rb", 12, 11)
            .await
            .iter()
            .any(|item| item.label == "upcase"),
        "the initial Text receiver must offer Text#upcase"
    );
    assert!(
        !editor.goto_def_at("main.rb", 12, 11).await.is_empty(),
        "the initial Text receiver must navigate to Text#upcase"
    );
    assert!(
        editor.signature_help_at("main.rb", 12, 18).await.is_some(),
        "the initial Text receiver must provide Text#upcase signature help"
    );
    assert!(
        editor.inlay_hints("main.rb").await.into_iter().any(|hint| {
            hint.position.line == 10
                && matches!(
                    hint.label,
                    InlayHintLabel::String(ref label) if label == " -> String"
                )
        }),
        "the initial proven Text#upcase call must supply Picker#normalize's return inlay"
    );

    editor.set("main.rb", conditional_source).await;
    let receiver_hover = editor
        .hover_at("main.rb", 13, 6)
        .await
        .expect("the joined short-circuit receiver must retain hover context");
    assert!(
        hover_text(receiver_hover).contains("(Product | Text)"),
        "hover must expose the exhaustive short-circuit receiver union"
    );
    let call_hover = editor
        .hover_at("main.rb", 13, 11)
        .await
        .expect("the partial-union call must retain Unknown hover context");
    assert!(
        hover_text(call_hover).contains("Unknown[incomplete_union_member]"),
        "call hover must explain that one reachable receiver does not prove upcase"
    );
    let conditional_completions = editor.complete_at("main.rb", 13, 11).await;
    assert!(
        conditional_completions
            .iter()
            .all(|item| item.label != "upcase"),
        "completion must require upcase on every reachable receiver, got {:?}",
        conditional_completions
            .iter()
            .map(|item| item.label.as_str())
            .collect::<Vec<_>>()
    );
    assert!(
        editor.goto_def_at("main.rb", 13, 11).await.is_empty(),
        "navigation must not select Text#upcase from a partial receiver union"
    );
    assert!(
        editor.signature_help_at("main.rb", 13, 18).await.is_none(),
        "signature help must not select Text#upcase from a partial receiver union"
    );
    assert!(
        editor.inlay_hints("main.rb").await.into_iter().all(|hint| {
            hint.position.line != 10
                || !matches!(
                    hint.label,
                    InlayHintLabel::String(ref label) if label == " -> String"
                )
        }),
        "Picker#normalize must not publish Text#upcase's String result from a partial-union dispatch"
    );

    editor.set("main.rb", proven_source).await;
    assert!(
        editor
            .complete_at("main.rb", 12, 11)
            .await
            .iter()
            .any(|item| item.label == "upcase"),
        "restoring the unconditional Text assignment must restore completion"
    );
    assert!(
        !editor.goto_def_at("main.rb", 12, 11).await.is_empty(),
        "restoring the unconditional Text assignment must restore navigation"
    );
    assert!(
        editor.inlay_hints("main.rb").await.into_iter().any(|hint| {
            hint.position.line == 10
                && matches!(
                    hint.label,
                    InlayHintLabel::String(ref label) if label == " -> String"
                )
        }),
        "restoring the unconditional Text assignment must restore the method-return inlay"
    );
}

#[tokio::test]
async fn rescue_entry_types_drive_every_receiver_consumer_and_cli() {
    let proven_source = r#"class Product
  def normalize(prefix)
    1
  end
end

class Text
  def normalize(prefix)
    "text"
  end
end

class Picker
  def choose
    value = Text.new
    value.normalize("x")
  end
end
"#;
    let rescued_union_source = r#"class Product
  def normalize(prefix)
    1
  end
end

class Text
  def normalize(prefix)
    "text"
  end
end

class Picker
  def choose
    value = Product.new
    begin
      value = Text.new
      raise
    rescue
      value.normalize("x")
    end
  end
end
"#;
    let unresolved_source = r#"class Product
  def normalize(prefix)
    1
  end
end

class Text
  def normalize(prefix)
    "text"
  end
end

class Picker
  def choose
    value = Product.new
    begin
      value = dynamic_value
      raise
    rescue
      value.normalize("x")
    end
  end
end
"#;
    let unknown_return_union_source = r#"class Product
  def normalize(prefix)
    dynamic_value
  end
end

class Text
  def normalize(prefix)
    dynamic_value
  end
end

class Picker
  def choose
    value = Product.new
    begin
      value = Text.new
      raise
    rescue
      value.normalize("x")
    end
  end
end
"#;
    let private_union_source = r#"class Product
  def normalize(prefix)
    1
  end
end

class Text
  def normalize(prefix)
    "text"
  end
  private :normalize
end

class Picker
  def choose
    value = Product.new
    begin
      value = Text.new
      raise
    rescue
      value.normalize("x")
    end
  end
end
"#;

    let project = tempfile::tempdir().expect("temporary rescue-flow project must be created");
    std::fs::write(project.path().join("main.rb"), rescued_union_source)
        .expect("rescue-flow parity fixture must be written");
    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must retain every protected assignment prefix");
    assert!(
        check_report.inferred_types.iter().any(|inferred| {
            inferred.kind == CheckTypeSubjectKind::Expression
                && inferred.outcome
                    == CheckTypeOutcome::Proven {
                        type_label: "(Product | Text)".to_string(),
                    }
        }),
        "the CLI must publish the exhaustive rescue receiver union, got {:#?}",
        check_report.inferred_types
    );
    assert!(
        check_report.inferred_types.iter().any(|inferred| {
            inferred.kind == CheckTypeSubjectKind::Expression
                && inferred.outcome
                    == CheckTypeOutcome::Proven {
                        type_label: "(Integer | String)".to_string(),
                    }
        }),
        "the CLI must publish the exhaustive common-call return union, got {:#?}",
        check_report.inferred_types
    );
    assert!(
        check_report
            .diagnostics
            .iter()
            .all(|diagnostic| {
                diagnostic.code.as_deref() != Some("unresolved-method")
                    || !diagnostic.message.contains("`normalize`")
            }),
        "a call proven for every rescue receiver member must not produce an unresolved-method diagnostic: {:#?}",
        check_report.diagnostics
    );

    std::fs::write(project.path().join("main.rb"), unresolved_source)
        .expect("unresolved rescue-flow parity fixture must be written");
    let unresolved_check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must fail closed for an unproven protected assignment");
    assert!(
        unresolved_check_report
            .inferred_types
            .iter()
            .any(|inferred| {
                inferred.kind == CheckTypeSubjectKind::Expression
                    && inferred.range.start.line == 20
                    && inferred.range.start.column == 7
                    && inferred.range.end.column == 12
                    && inferred.outcome
                        == CheckTypeOutcome::Unknown {
                            reason: UnknownReason::UnresolvedAssignmentValue,
                        }
            }),
        "the CLI must retain the exact unresolved protected-assignment reason, got {:#?}",
        unresolved_check_report.inferred_types
    );
    assert!(
        unresolved_check_report
            .inferred_types
            .iter()
            .any(|inferred| {
                inferred.kind == CheckTypeSubjectKind::Expression
                    && inferred.range.start.line == 20
                    && inferred.range.start.column == 7
                    && inferred.range.end.column == 27
                    && inferred.outcome
                        == CheckTypeOutcome::Unknown {
                            reason: UnknownReason::UnknownReceiver,
                        }
            }),
        "the CLI must project the unresolved rescue receiver into the shared call proof, got {:#?}",
        unresolved_check_report.inferred_types
    );

    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", proven_source).await;
    assert!(
        editor
            .complete_at("main.rb", 15, 12)
            .await
            .iter()
            .any(|item| item.label == "normalize"),
        "the initial Text receiver must offer Text#normalize"
    );
    assert_eq!(
        editor.goto_def_at("main.rb", 15, 13).await.len(),
        1,
        "the initial Text receiver must navigate to Text#normalize"
    );
    assert!(
        editor.signature_help_at("main.rb", 15, 22).await.is_some(),
        "the initial Text receiver must provide Text#normalize signature help"
    );
    assert!(
        editor.inlay_hints("main.rb").await.into_iter().any(|hint| {
            hint.position.line == 13
                && matches!(
                    hint.label,
                    InlayHintLabel::String(ref label) if label == " -> String"
                )
        }),
        "the initial proven Text#normalize call must supply Picker#choose's return inlay"
    );

    editor.set("main.rb", rescued_union_source).await;
    let receiver_hover = editor
        .hover_at("main.rb", 19, 8)
        .await
        .expect("the rescue receiver union must retain hover context");
    assert!(
        hover_text(receiver_hover).contains("(Product | Text)"),
        "hover must expose every protected assignment prefix"
    );
    let call_hover = editor
        .hover_at("main.rb", 19, 14)
        .await
        .expect("the complete union call must retain hover context");
    assert!(
        hover_text(call_hover).contains("(Integer | String)"),
        "call hover must union the proven Product#normalize and Text#normalize returns"
    );
    assert!(
        editor
            .complete_at("main.rb", 19, 13)
            .await
            .iter()
            .any(|item| item.label == "normalize"),
        "completion must retain a method proven on every rescue receiver member"
    );
    assert_eq!(
        editor.goto_def_at("main.rb", 19, 14).await.len(),
        2,
        "navigation must return both proven union receiver definitions"
    );
    assert!(
        editor.prepare_rename_at("main.rb", 19, 14).await.is_none(),
        "a call that can dispatch to two independent method identities must not be renameable"
    );
    for (definition_line, owner) in [(1, "Product"), (7, "Text")] {
        let references = editor.references_at("main.rb", definition_line, 7).await;
        assert!(
            references.iter().any(|location| {
                location.uri.path().ends_with("/main.rb")
                    && location.range.start.line == 19
                    && location.range.start.character == 12
            }),
            "the proven union call must be indexed as a reference to {owner}#normalize, got {references:#?}"
        );
    }
    let union_signature = editor
        .signature_help_at("main.rb", 19, 23)
        .await
        .expect("the complete union receiver must provide signature help");
    assert!(
        union_signature
            .signatures
            .iter()
            .all(|signature| signature.label.starts_with("normalize(prefix)")),
        "every union signature must preserve the common parameter contract"
    );
    assert!(
        editor.inlay_hints("main.rb").await.into_iter().any(|hint| {
            hint.position.line == 13
                && matches!(
                    hint.label,
                    InlayHintLabel::String(ref label) if label == " -> (Integer | String)"
                )
        }),
        "the exhaustive rescue dispatch must supply Picker#choose's union return inlay"
    );

    editor.set("main.rb", unknown_return_union_source).await;
    let main_uri = Url::parse("file:///main.rb").expect("rescue fixture URI must be valid");
    let analysis_engine = editor.server().analysis_engine_for_uri(&main_uri);
    let main_file_id = analysis_engine
        .read()
        .file_id(std::path::Path::new("/main.rb"))
        .expect("rescue fixture must be registered in the analysis engine");
    let method_return_outcomes = analysis_engine
        .read()
        .method_return_outcomes_in_file(main_file_id)
        .expect("rescue fixture must retain method-return outcomes")
        .clone();
    assert!(
        method_return_outcomes
            .values()
            .all(|outcome| outcome.proven_type().is_none()),
        "the edited file must atomically invalidate every caller derived from the two unknown returns: {method_return_outcomes:#?}"
    );
    let unknown_return_hover = editor
        .hover_at("main.rb", 19, 14)
        .await
        .expect("the exact union dispatch with unknown returns must retain hover context");
    let unknown_return_hover = hover_text(unknown_return_hover);
    assert!(
        unknown_return_hover.contains("Unknown[incomplete_union_member]"),
        "a union call must remain unknown until every member return type is proven, got `{unknown_return_hover}` with method outcomes {method_return_outcomes:#?}"
    );
    assert_eq!(
        editor.goto_def_at("main.rb", 19, 14).await.len(),
        2,
        "unknown return types must not erase exact union dispatch definitions"
    );
    assert!(
        editor
            .diagnostics("main.rb")
            .await
            .iter()
            .all(|diagnostic| {
                !matches!(
                    &diagnostic.code,
                    Some(NumberOrString::String(code)) if code == "unresolved-method"
                ) || !diagnostic.message.contains("`normalize`")
            }),
        "an exact dispatch must not become unresolved merely because its return type is unknown"
    );
    for (definition_line, owner) in [(1, "Product"), (7, "Text")] {
        let references = editor.references_at("main.rb", definition_line, 7).await;
        assert!(
            references.iter().any(|location| {
                location.uri.path().ends_with("/main.rb")
                    && location.range.start.line == 19
                    && location.range.start.character == 12
            }),
            "the exact union call with unknown returns must remain a reference to {owner}#normalize, got {references:#?}"
        );
    }

    editor.set("main.rb", private_union_source).await;
    let private_call_hover = editor
        .hover_at("main.rb", 20, 14)
        .await
        .expect("the visibility-incomplete union call must retain hover context");
    assert!(
        hover_text(private_call_hover).contains("Unknown[incomplete_union_member]"),
        "one private explicit-receiver member must invalidate the complete union dispatch"
    );
    assert!(
        editor.goto_def_at("main.rb", 20, 14).await.is_empty(),
        "navigation must fail closed when one union member is private"
    );
    assert!(
        editor.signature_help_at("main.rb", 20, 23).await.is_none(),
        "signature help must fail closed when one union member is private"
    );
    for definition_line in [1, 7] {
        assert!(
            editor
                .references_at("main.rb", definition_line, 7)
                .await
                .iter()
                .all(|location| location.range.start.line != 20),
            "reindexing must remove the prior grouped call when one union member becomes inaccessible"
        );
    }

    editor.set("main.rb", unresolved_source).await;
    let unresolved_receiver = editor
        .hover_at("main.rb", 19, 8)
        .await
        .expect("the unresolved rescue receiver must retain hover context");
    assert!(
        hover_text(unresolved_receiver).contains("Unknown[unresolved_assignment_value]"),
        "one unproven protected assignment must absorb the rescue receiver union"
    );
    assert!(
        editor
            .complete_at("main.rb", 19, 13)
            .await
            .iter()
            .all(|item| item.label != "normalize"),
        "completion must fail closed after an unresolved protected assignment"
    );
    assert!(
        editor.goto_def_at("main.rb", 19, 14).await.is_empty(),
        "navigation must fail closed after an unresolved protected assignment"
    );
    assert!(
        editor.signature_help_at("main.rb", 19, 23).await.is_none(),
        "signature help must fail closed after an unresolved protected assignment"
    );
    assert!(
        editor.inlay_hints("main.rb").await.into_iter().all(|hint| {
            hint.position.line != 13
                || !matches!(
                    hint.label,
                    InlayHintLabel::String(ref label) if label == " -> (Integer | String)"
                )
        }),
        "an unresolved protected assignment must remove the previously proven return inlay"
    );

    editor.set("main.rb", proven_source).await;
    assert_eq!(
        editor.goto_def_at("main.rb", 15, 13).await.len(),
        1,
        "restoring the unconditional Text receiver must restore navigation"
    );
    assert!(
        editor.signature_help_at("main.rb", 15, 22).await.is_some(),
        "restoring the unconditional Text receiver must restore signature help"
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
async fn cross_file_module_constructor_chain_remains_unknown() {
    let declaration_source = "module FactoryLike\nend\n";
    let call_source = "FactoryLike.new.to_s\n";
    let project = tempfile::tempdir().expect("temporary module-chain project must be created");
    std::fs::write(project.path().join("factory_like.rb"), declaration_source)
        .expect("module declaration fixture must be written");
    std::fs::write(project.path().join("main.rb"), call_source)
        .expect("module call fixture must be written");

    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must retain the unproven module constructor chain");
    let outer_call = check_report
        .inferred_types
        .iter()
        .find(|inferred| {
            inferred.path == std::path::Path::new("main.rb")
                && inferred.kind == CheckTypeSubjectKind::Expression
                && inferred.range.start.line == 1
                && inferred.range.start.column == 1
                && inferred.range.end.column == 21
        })
        .unwrap_or_else(|| {
            panic!(
                "the CLI must retain an exact outcome for the module chain, got {:#?}",
                check_report.inferred_types
            )
        });
    assert!(
        matches!(outer_call.outcome, CheckTypeOutcome::Unknown { .. }),
        "a module declaration must never be reclassified as a class merely because it receives `new`: {:?}",
        outer_call.outcome
    );

    let mut editor = FakeEditor::new().await;
    editor.open("factory_like.rb", declaration_source).await;
    editor.open("main.rb", call_source).await;
    let hover = editor
        .hover_at("main.rb", 0, 18)
        .await
        .expect("the unproven module chain must retain hover context");
    let actual = hover_text(hover);
    assert!(
        actual.contains("Unknown["),
        "LSP hover must fail closed for the same module chain as CLI, got `{actual}`"
    );
}

#[tokio::test]
async fn cross_file_constructor_distinguishes_initialize_from_explicit_new() {
    let project = tempfile::tempdir().expect("temporary constructor-proof project must be created");
    std::fs::write(
        project.path().join("widget.rb"),
        "class Widget\n  def initialize(value = nil)\n  end\n  def label\n    \"widget\"\n  end\nend\n",
    )
    .expect("normalized initialize fixture must be written");
    std::fs::write(
        project.path().join("factory.rb"),
        "class Factory\n  def self.new\n    dynamic_factory\n  end\nend\n",
    )
    .expect("explicit new fixture must be written");
    std::fs::write(
        project.path().join("main.rb"),
        "Widget.new.label\nFactory.new.to_s\n",
    )
    .expect("constructor call fixture must be written");

    let report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must distinguish constructor origins");
    let widget_call = report
        .inferred_types
        .iter()
        .find(|inferred| {
            inferred.path == std::path::Path::new("main.rb")
                && inferred.kind == CheckTypeSubjectKind::Expression
                && inferred.range.start.line == 1
                && inferred.range.start.column == 1
                && inferred.range.end.column == 17
        })
        .expect("the normalized initialize chain must retain its outer expression");
    assert_eq!(
        widget_call.outcome,
        CheckTypeOutcome::Proven {
            type_label: "String".to_string(),
        },
        "Ruby initialize normalization must prove the constructed Widget before resolving the chain"
    );
    let factory_call = report
        .inferred_types
        .iter()
        .find(|inferred| {
            inferred.path == std::path::Path::new("main.rb")
                && inferred.kind == CheckTypeSubjectKind::Expression
                && inferred.range.start.line == 2
                && inferred.range.start.column == 1
                && inferred.range.end.column == 17
        })
        .expect("the explicit new chain must retain its outer expression");
    assert!(
        matches!(factory_call.outcome, CheckTypeOutcome::Unknown { .. }),
        "an explicit self.new with an unproven body must not inherit builtin constructor semantics: {:?}",
        factory_call.outcome
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
            diagnostic.code.as_ref().is_some_and(
                |code_value| matches!(code_value, NumberOrString::String(value) if value == code),
            )
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
    std::fs::write(project.path().join("main.rb"), source).expect("parity fixture must be written");
    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the unresolved-method fixture");
    let mut editor = FakeEditor::new().await;
    editor.open("main.rb", source).await;
    let published = editor.diagnostics("main.rb").await;
    assert!(
        check_report.diagnostics.iter().all(|diagnostic| {
            diagnostic.code.as_deref() != Some("unresolved-method")
                || !diagnostic.message.contains("`new`")
        }) && published.iter().all(|diagnostic| {
            !matches!(&diagnostic.code, Some(NumberOrString::String(code)) if code == "unresolved-method")
                || !diagnostic.message.contains("`new`")
        }),
        "Class#new must resolve through the shared Class object lookup chain: CLI={:?}, LSP={published:?}",
        check_report.diagnostics
    );

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
    std::fs::write(project.path().join("main.rb"), source).expect("parity fixture must be written");
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
    std::fs::write(project.path().join("main.rb"), source).expect("parity fixture must be written");
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
    std::fs::write(project.path().join("main.rb"), source).expect("parity fixture must be written");
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
    std::fs::write(project.path().join("main.rb"), source).expect("parity fixture must be written");
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
    editor
        .open("project/main.rb", "require \"missing\"\n")
        .await;
    let published = editor.diagnostics("project/main.rb").await;
    let lsp_diagnostic = find_lsp_diagnostic(&published, "unresolved-require");
    assert_diagnostic_parity(check_diagnostic, lsp_diagnostic);
}

#[tokio::test]
async fn syntax_diagnostic_matches_cli_and_lsp() {
    let source = "def broken(\n";
    let project = tempfile::tempdir().expect("temporary parity project must be created");
    std::fs::write(project.path().join("main.rb"), source).expect("parity fixture must be written");
    let check_report = CheckSession::default()
        .check_path(project.path())
        .await
        .expect("headless check must analyze the syntax-error fixture");
    let mut check_syntax = check_report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code.is_none())
        .map(|diagnostic| (diagnostic.range, diagnostic.message.clone()))
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
    std::fs::write(project.path().join("main.rb"), source).expect("parity fixture must be written");
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
