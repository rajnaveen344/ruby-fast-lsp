//! Method return, higher-order, and callable-body types shared by `check` and LSP projections.

use super::support::hover_text;
use crate::invariant::ExpectInvariant;
use crate::lsp::check::{CheckSession, CheckTypeOutcome, CheckTypeSubjectKind};
use crate::test::harness::{get_hint_label, FakeEditor};
use ruby_analysis::core::UnknownReason;

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
            let label = get_hint_label(&hint);
            label.starts_with(" -> ").then(|| {
                (
                    hint.position.line.checked_add(1).expect_invariant(
                        "LSP line exhausted u32 during parity normalization",
                        "source positions must fit u32",
                        "reject sources whose normalized line cannot be one-based",
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
    assert!(editor
        .inlay_hints("main.rb")
        .await
        .iter()
        .any(|hint| { get_hint_label(&hint) == ": Array<String>" }));
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
    assert!(editor
        .inlay_hints("main.rb")
        .await
        .iter()
        .any(|hint| { get_hint_label(&hint) == ": String" }));
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
