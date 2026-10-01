//! Variable, expression, structural shape, constant, and nonlocal types shared by `check` and LSP projections.

use super::support::hover_text;
use crate::invariant::ExpectInvariant;
use crate::lsp::check::{CheckSession, CheckTypeOutcome, CheckTypeSubjectKind};
use crate::test::harness::{get_hint_label, get_hint_tooltip, FakeEditor};
use ruby_analysis::core::UnknownReason;

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
            let label = get_hint_label(&hint);
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
            get_hint_label(&hint).starts_with(": Hash<Symbol, ")
                && get_hint_tooltip(&hint) == Some("```ruby\n{\n  id: Integer,\n  profile: {\n    active: TrueClass,\n    name: String\n  }\n}\n```")
        }),
        "LSP inlay tooltips must format every field of the CLI shape behind the compact Hash label"
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
            let label = get_hint_label(&hint);
            matches!(
                (hint.position.line, hint.position.character),
                (2, 18) | (3, 12) | (4, 10) | (5, 11) | (8, 5)
            )
            .then(|| {
                (
                    hint.position.line.checked_add(1).expect_invariant(
                        "LSP line exhausted u32 during parity normalization",
                        "source positions must fit u32",
                        "reject sources whose normalized line cannot be one-based",
                    ),
                    hint.position.character.checked_add(1).expect_invariant(
                        "LSP column exhausted u32 during parity normalization",
                        "source positions must fit u32",
                        "reject sources whose normalized column cannot be one-based",
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
