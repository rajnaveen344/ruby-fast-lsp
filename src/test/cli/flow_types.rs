//! Flow-sensitive local types across case paths, joins, and short-circuit assignments in `check` and LSP.

use super::support::hover_text;
use crate::lsp::check::{CheckSession, CheckTypeOutcome, CheckTypeSubjectKind};
use crate::test::harness::{get_hint_label, FakeEditor};
use ruby_analysis::core::{RubyType, UnknownReason};

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
        editor
            .inlay_hints("main.rb")
            .await
            .into_iter()
            .any(|hint| { get_hint_label(&hint) == " -> (Integer | String)" }),
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
        editor
            .inlay_hints("main.rb")
            .await
            .into_iter()
            .any(|hint| { get_hint_label(&hint) == " -> String" }),
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
        .get_doc(&crate::test::harness::fixture_uri("/main.rb"))
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
        editor
            .inlay_hints("main.rb")
            .await
            .into_iter()
            .all(|hint| { hint.position.line != 7 || get_hint_label(&hint) != " -> String" }),
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
        editor
            .inlay_hints("main.rb")
            .await
            .into_iter()
            .any(|hint| { hint.position.line == 10 && get_hint_label(&hint) == " -> String" }),
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
                || get_hint_label(&hint) != " -> String"
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
        editor
            .inlay_hints("main.rb")
            .await
            .into_iter()
            .any(|hint| { hint.position.line == 10 && get_hint_label(&hint) == " -> String" }),
        "restoring the unconditional Text assignment must restore the method-return inlay"
    );
}
