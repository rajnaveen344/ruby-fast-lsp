//! Call-resolution proofs and Unknown reasons shared by `check` and LSP: chained, deferred, lambda, cross-file, reopened, and union calls.

use super::support::hover_text;
use crate::lsp::check::{CheckSession, CheckTypeOutcome, CheckTypeSubjectKind};
use crate::test::harness::FakeEditor;
use ruby_analysis::core::UnknownReason;
use ruby_analysis::engine::ResolveStat;

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
    let analysis_engine = editor
        .server()
        .analysis_engine_for_uri(&crate::test::harness::fixture_uri("/cycle_even.rb"));
    let even_file_id = analysis_engine
        .read()
        .file_id(&crate::test::harness::fixture_path("/cycle_even.rb"))
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
            .get(ResolveStat::MethodReturnEquationSolveRuns),
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
            .get(ResolveStat::MethodReturnEquationSolveRuns),
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
