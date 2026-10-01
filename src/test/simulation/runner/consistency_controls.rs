use super::protocol::{assert_definition_precedence, definition_observation};
use super::*;
use crate::test::simulation::graph::CallShape;

#[tokio::test]
async fn closed_source_changes_remain_invisible_until_reopened() {
    let mut project = SyntheticProject::new("delivery_control");
    project
        .class("Provider", |class| {
            class.method("value").returns("String");
            class.constant("TOKEN", "1");
        })
        .class("Reader", |class| {
            class
                .method("read")
                .calls("Provider#value", CallShape::ConstructorSend)
                .ref_const("Provider::TOKEN");
        });
    let mut runner = SimulationRunner::start(project).await;
    let delivered = runner.indexed_contents["provider.rb"].clone();
    runner.check_initial().await;
    runner.close_file("provider.rb").await;
    // This models an external source edit with no watcher notification.
    // Complete oracle checks must still observe the delivered definitions.
    let delete = EditStep::new("change undelivered source", |edit| {
        edit.delete_method("Provider#value")
            .delete_constant("Provider::TOKEN");
    });
    runner.apply_step_with_fresh_equivalence(&delete).await;
    assert_eq!(runner.indexed_contents["provider.rb"], delivered);
    runner.assert_call_resolves_to("Provider#value").await;
    assert!(runner.render.map.constants.contains_key("Provider::TOKEN"));

    runner.open_file("provider.rb").await;
    runner.check_fresh_equivalence().await;
    assert_ne!(runner.indexed_contents["provider.rb"], delivered);
    assert!(!runner
        .render
        .map
        .defs
        .contains_key(&MethodTarget::parse("Provider#value")));
    assert!(!runner.render.map.constants.contains_key("Provider::TOKEN"));

    let restore = EditStep::new("restore delivered source", |edit| {
        edit.restore_method("Provider#value")
            .restore_constant("Provider::TOKEN");
    });
    runner.apply_step_with_fresh_equivalence(&restore).await;
    assert_eq!(runner.indexed_contents["provider.rb"], delivered);
    runner.assert_call_resolves_to("Provider#value").await;
}

#[tokio::test]
async fn definition_precedence_guard_rejects_filename_order() {
    let mut project = SyntheticProject::new("definition_priority_control");
    crate::test::simulation::seeded::add_module_dispatch_scenario(&mut project, "Dispatch");
    for op in project.edits[0].ops.clone() {
        project.apply_op(&op);
    }
    let runner = SimulationRunner::start(project).await;
    let call = runner
        .render
        .map
        .calls
        .iter()
        .find(|call| matches!(call.shape, CallShape::Bare))
        .unwrap();
    let oracle = OracleState::all_files(&runner.observed_project, &runner.render.map);
    let constraints = oracle.definition_order_constraints(call);
    assert_eq!(
        constraints.len(),
        1,
        "the reviewed control must have one nontrivial precedence constraint"
    );
    let (before, after) = (&constraints[0].0, &constraints[0].1);
    let mut locations = runner
        .editor
        .goto_def_at(&call.pos.file, call.pos.line, call.pos.character)
        .await;
    assert_definition_precedence(&locations, runner.def_pos(before), runner.def_pos(after));
    locations.sort_by(|left, right| left.uri.cmp(&right.uri));
    let before = runner.def_pos(before).clone();
    let after = runner.def_pos(after).clone();
    assert!(
        std::panic::catch_unwind(|| assert_definition_precedence(&locations, &before, &after))
            .is_err(),
        "the simulator must reject the old filename order even when every destination is valid"
    );
}

#[test]
fn definition_observation_normalizes_order_without_hiding_faults() {
    use tower_lsp::lsp_types::Range;
    let first = Location::new(
        crate::test::harness::fixture_uri("/first.rb"),
        Range::new(Position::new(1, 2), Position::new(3, 4)),
    );
    let second = Location::new(crate::test::harness::fixture_uri("/second.rb"), first.range);
    let observe =
        |locations| definition_observation(Some(GotoDefinitionResponse::Array(locations)));
    let expected = observe(vec![first.clone(), second.clone()]);
    assert_eq!(expected, observe(vec![second.clone(), first.clone()]));
    assert_ne!(expected, observe(vec![first.clone()]));
    assert_ne!(
        expected,
        observe(vec![first.clone(), first.clone(), second.clone()])
    );
    let mut wrong_range = second.clone();
    wrong_range.range.end.character += 1;
    assert_ne!(expected, observe(vec![first, wrong_range]));
    assert_ne!(definition_observation(None), observe(Vec::new()));
}

#[tokio::test]
async fn module_dispatch_oracle_advances_only_when_closed_edits_are_delivered() {
    let mut project = SyntheticProject::new("closed_dispatch");
    crate::test::simulation::seeded::add_module_dispatch_scenario(&mut project, "Dispatch");
    let edit = project.edits[0].clone();
    let mut runner = SimulationRunner::start(project).await;
    runner.close_file("dispatch/first_host.rb").await;
    runner.apply_step_with_fresh_equivalence(&edit).await;
    runner.open_file("dispatch/first_host.rb").await;
    runner.check_fresh_equivalence().await;
}

#[tokio::test]
async fn fresh_equivalence_uses_only_delivered_content_across_close_and_reopen() {
    let mut project = SyntheticProject::new("fresh_equivalence_closed_content");
    project
        .raw_file("definition.rb", "VALUE = 1\n")
        .raw_file("caller.rb", "value = VALUE\n")
        .raw_file("unopened.rb", "UNOPENED = true\n");
    let mut runner =
        SimulationRunner::start_with_open_files(project, &["definition.rb", "caller.rb"]).await;
    assert!(!runner.indexed_contents.contains_key("unopened.rb"));
    runner.close_file("definition.rb").await;
    runner
        .project
        .raw_file("definition.rb", "VALUE = \"updated\"\n");
    runner
        .apply_step_with_fresh_equivalence(&EditStep::new("render closed edit", |_| {}))
        .await;
    assert_eq!(runner.indexed_contents["definition.rb"], "VALUE = 1\n");
    runner.check_fresh_equivalence().await;

    runner.open_file("definition.rb").await;
    assert_eq!(
        runner.indexed_contents["definition.rb"],
        "VALUE = \"updated\"\n"
    );
    runner.check_fresh_equivalence().await;
}

#[tokio::test]
#[should_panic(expected = "fresh-equivalence mismatch")]
async fn fresh_equivalence_detects_stale_editor_content() {
    let mut project = SyntheticProject::new("fresh_equivalence_negative_control");
    project.raw_file("value.rb", "value = 1\n");
    let mut runner = SimulationRunner::start(project.clone()).await;
    runner.check_fresh_equivalence().await;

    // Emulate a delayed writer replacing newer content without updating the
    // runner's authoritative delivered-content snapshot.
    let mut stale = SimulationRunner::start(project).await;
    stale.editor.set("value.rb", "value = \"stale\"\n").await;
    runner.editor = stale.editor;
    runner.check_fresh_equivalence().await;
}
