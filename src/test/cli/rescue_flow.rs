//! Rescue-entry flow types consumed by every receiver feature and `check`.

use super::support::hover_text;
use crate::lsp::check::{CheckSession, CheckTypeOutcome, CheckTypeSubjectKind};
use crate::test::harness::{get_hint_label, FakeEditor};
use ruby_analysis::core::UnknownReason;
use tower_lsp::lsp_types::NumberOrString;

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
        editor
            .inlay_hints("main.rb")
            .await
            .into_iter()
            .any(|hint| { hint.position.line == 13 && get_hint_label(&hint) == " -> String" }),
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
            hint.position.line == 13 && get_hint_label(&hint) == " -> (Integer | String)"
        }),
        "the exhaustive rescue dispatch must supply Picker#choose's union return inlay"
    );

    editor.set("main.rb", unknown_return_union_source).await;
    let main_uri = crate::test::harness::fixture_uri("/main.rb");
    let analysis_engine = editor.server().analysis_engine_for_uri(&main_uri);
    let main_file_id = analysis_engine
        .read()
        .view()
        .file_id(&crate::test::harness::fixture_path("/main.rb"))
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
            hint.position.line != 13 || get_hint_label(&hint) != " -> (Integer | String)"
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
