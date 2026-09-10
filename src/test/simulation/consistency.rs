//! Incremental lifecycle results must agree with a fresh public-LSP analysis.

use super::tests::{phase1_project, superclass_switch_project};
use super::{
    seeded_script, simulation_seeds_from_env, write_seed_artifact, EditStep, SimulationRunner,
};

#[tokio::test]
async fn fresh_equivalence_initial_generated_project() {
    let runner = SimulationRunner::start(phase1_project()).await;
    runner.check_fresh_equivalence().await;
}

#[tokio::test]
async fn fresh_equivalence_after_method_restoration_and_superclass_switch() {
    let project = phase1_project();
    let mut runner = SimulationRunner::start(project.clone()).await;
    for step in &project.edits {
        eprintln!("fresh-equivalence checkpoint: {}", step.name);
        runner.apply_step_with_fresh_equivalence(step).await;
    }

    let mut runner = SimulationRunner::start(superclass_switch_project()).await;
    let step = EditStep::new("switch superclass", |edit| {
        edit.change_superclass("SimSuper::Child", "SimSuper::NewBase")
            .expect_no_method_definition_target(
                "SimSuper::OldBase#resolve_token",
                "SimSuper::OldBase#resolve_token",
            );
    });
    eprintln!("fresh-equivalence checkpoint: {}", step.name);
    runner.apply_step_with_fresh_equivalence(&step).await;
}

#[tokio::test]
async fn fresh_equivalence_seeded_edit_open_close_sequences() {
    for seed in simulation_seeds_from_env() {
        let script = seeded_script(seed);
        let artifact = write_seed_artifact(&script);
        eprintln!(
            "fresh-equivalence seed {seed}; replay with SIM_SEED={seed} cargo test fresh_equivalence_seeded_edit_open_close_sequences -- --nocapture; artifact {}",
            artifact.display()
        );
        let initial_files = script
            .initial_open_files
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let mut runner =
            SimulationRunner::start_with_open_files(script.project.clone(), &initial_files).await;
        runner.check_initial().await;
        for (checkpoint, step) in script.steps.iter().enumerate() {
            eprintln!("fresh-equivalence seed {seed} checkpoint {checkpoint}: {step:?}");
            runner
                .run_edit_script_step_with_fresh_equivalence(step)
                .await;
        }
        eprintln!("fresh-equivalence seed {seed} final checkpoint");
        runner.check_fresh_equivalence().await;
    }
}

#[test]
fn seed_artifact_writes_preserve_each_run() {
    let script = seeded_script(42);
    let first = write_seed_artifact(&script);
    let second = write_seed_artifact(&script);
    assert_ne!(
        first, second,
        "each simulation run must retain its own artifact"
    );
    for root in [first, second] {
        let readme = std::fs::read_to_string(root.join("README.txt"))
            .expect("simulation artifact must retain replay instructions");
        assert!(readme.contains("SIM_SEED=42"));
        let description = std::fs::read_to_string(root.join("script.txt"))
            .expect("simulation artifact must retain its script");
        assert!(!description.is_empty());
        for (file, expected) in script.project.render().files {
            let actual = std::fs::read_to_string(root.join("files").join(file))
                .expect("simulation artifact must retain every generated source file");
            assert_eq!(actual, expected);
        }
    }
}
