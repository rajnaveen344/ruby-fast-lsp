//! Generated-project simulation tests grouped by modeled Ruby feature and observation.

mod coverage;
mod diagnostics;
mod hierarchy_projects;
mod metaprogramming_projects;
mod navigation;
mod oracle_model;
mod seeded_replay;

pub(super) use hierarchy_projects::{phase1_project, superclass_switch_project};

use super::ruby_gen::SourcePos;
use super::seeded;
use super::{
    CallShape, EngineSimulationRunner, MethodTarget, OracleState, SimulationRunner,
    SyntheticProject,
};

fn location_matches_pos(location: &tower_lsp::lsp_types::Location, pos: &SourcePos) -> bool {
    location.uri.path().ends_with(&pos.file) && location.range.start.line == pos.line
}

#[tokio::test]
async fn generated_module_dispatch_tracks_exact_targets_after_edits() {
    let mut project = SyntheticProject::new("module_dispatch");
    super::seeded::add_module_dispatch_scenario(&mut project, "Dispatch");
    let steps = project.edits.clone();
    let mut engine = EngineSimulationRunner::start(project.clone());
    engine.check_definitions();
    let mut runner = SimulationRunner::start(project).await;
    runner.check_initial().await;
    for step in &steps {
        engine.apply_step(step);
        engine.check_definitions();
        runner.apply_step_with_fresh_equivalence(step).await;
        runner.check_initial().await;
    }
}

#[test]
fn generated_module_dispatch_model_has_reviewed_edit_expectations() {
    let mut project = SyntheticProject::new("dispatch_model_controls");
    super::seeded::add_module_dispatch_scenario(&mut project, "Dispatch");
    let steps = project.edits.clone();
    let expected: &[&[&str]] = &[
        &["Dispatch::FirstHost#label", "Dispatch::Wrapper#label"],
        &["Dispatch::Defaults#label", "Dispatch::Wrapper#label"],
        &["Dispatch::Defaults#label", "Dispatch::SecondHost#label"],
        &["Dispatch::Defaults#label"],
        &["Dispatch::FirstHost#label"],
        &["Dispatch::FirstHost#label", "Dispatch::SecondHost#label"],
        &["Dispatch::FirstHost#label", "Dispatch::Wrapper#label"],
    ];
    assert_eq!(
        steps.len() + 1,
        expected.len(),
        "every edited state needs a reviewed expectation"
    );
    for (index, expected) in expected.iter().enumerate() {
        if index > 0 {
            for op in &steps[index - 1].ops {
                project.apply_op(op);
            }
        }
        let render = project.render();
        let oracle = OracleState::all_files(&project, &render.map);
        let call = render
            .map
            .calls
            .iter()
            .find(|call| matches!(call.shape, CallShape::Bare))
            .expect("dispatch control must contain an ordinary internal call");
        let targets = oracle.resolve_call_targets(call);
        assert_eq!(
            targets
                .iter()
                .map(MethodTarget::signature)
                .collect::<Vec<_>>(),
            *expected,
            "reviewed module dispatch state {index}"
        );
        assert_eq!(
            oracle.resolve_unique_call(call).is_some(),
            expected.len() == 1,
            "references require one agreed receiver identity at state {index}"
        );
        let expected_priority = match index {
            1 => vec![(
                "Dispatch::Wrapper#label".to_owned(),
                "Dispatch::Defaults#label".to_owned(),
            )],
            2 => vec![(
                "Dispatch::SecondHost#label".to_owned(),
                "Dispatch::Defaults#label".to_owned(),
            )],
            0 | 3 | 4 | 5 | 6 => Vec::new(),
            unexpected => panic!(
                "dispatch priority control needs an explicit expectation for state {unexpected}"
            ),
        };
        assert_eq!(
            oracle
                .definition_order_constraints(call)
                .into_iter()
                .map(|(before, after)| (before.signature(), after.signature()))
                .collect::<Vec<_>>(),
            expected_priority
        );
        let reflection = render
            .map
            .calls
            .iter()
            .find(|call| matches!(call.shape, CallShape::InstanceMethodObject))
            .expect("dispatch control must include independent reflection");
        assert_eq!(
            oracle
                .resolve_unique_call(reflection)
                .map(|target| target.signature()),
            Some("Dispatch::Defaults#label".to_owned()),
            "reflection at state {index}"
        );
    }
}
