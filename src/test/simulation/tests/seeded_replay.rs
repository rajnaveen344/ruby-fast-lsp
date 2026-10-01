use super::phase1_project;
use crate::test::simulation::{
    seeded_script, simulation_seeds_from_env, write_seed_artifact, SimulationRunner,
};

#[test]
fn generated_project_seeded_generation_is_replayable() {
    let seed = 20_260_524;
    let first = seeded_script(seed);
    let second = seeded_script(seed);
    assert_eq!(first.project.render().files, second.project.render().files);
    assert_eq!(first.initial_open_files, second.initial_open_files);
    assert_eq!(first.steps, second.steps);
    assert_eq!(
        first
            .project
            .edits
            .iter()
            .map(|step| step.name.as_str())
            .collect::<Vec<_>>(),
        second
            .project
            .edits
            .iter()
            .map(|step| step.name.as_str())
            .collect::<Vec<_>>()
    );

    let render = first.project.render();
    let generated_source = render
        .files
        .values()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    for hardcoded_name in [
        "Billing",
        "Payments",
        "Catalog",
        "Audit",
        "Reporting",
        "Synthetic::Generated",
        "account",
        "audit",
        "capture",
        "charge",
        "gateway",
        "invoice",
        "item",
        "publish",
        "refund",
        "render",
        "sku",
        "summary",
    ] {
        assert!(
            !generated_source.contains(hardcoded_name),
            "seed {seed}: seeded source must not contain hardcoded fixture name `{hardcoded_name}`"
        );
    }
    let edit_names = first
        .project
        .edits
        .iter()
        .map(|step| step.name.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    for hardcoded_name in ["gateway", "capture", "invoice", "currency"] {
        assert!(
            !edit_names.contains(hardcoded_name),
            "seed {seed}: seeded edit labels must not contain hardcoded fixture name `{hardcoded_name}`"
        );
    }
    assert!(
        (46..=66).contains(&render.files.len()),
        "seed {seed}: expected 46-66 files, got {}",
        render.files.len()
    );
    assert!(
        (107..=157).contains(&first.project.enabled_method_count()),
        "seed {seed}: expected 107-157 methods, got {}",
        first.project.enabled_method_count()
    );
    assert!(
        (36..=96).contains(&first.project.meaningful_edge_count()),
        "seed {seed}: expected 36-96 meaningful graph edges, got {}",
        first.project.meaningful_edge_count()
    );
    for shape in ["method-object", "instance-method-object"] {
        assert!(
            render.map.calls.iter().any(|call| call.shape_name == shape),
            "seed {seed}: expected seeded generated call shape `{shape}`"
        );
    }
}

#[test]
fn generated_project_seeded_generation_has_stable_fingerprint() {
    let script = seeded_script(20_260_524);

    assert_eq!(
        seeded_script_fingerprint(&script),
        "fnv1a64:f034d6936aef2c68"
    );
}

#[tokio::test]
async fn generated_project_runs_deterministic_edit_scenario() {
    let project = phase1_project();
    let mut runner = SimulationRunner::start(project.clone()).await;
    runner.check_initial().await;

    for step in &project.edits {
        runner.apply_step(step).await;
    }

    runner.close_and_reopen("billing/invoice.rb").await;
}

#[tokio::test]
async fn generated_project_runs_seeded_edit_sequence() {
    let seeds = simulation_seeds_from_env();

    for seed in seeds {
        let script = seeded_script(seed);
        let artifact = write_seed_artifact(&script);
        eprintln!(
            "running simulation seed {seed}; replay with SIM_SEED={seed}; artifact {}",
            artifact.display()
        );
        let initial_open_files = script
            .initial_open_files
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let mut runner =
            SimulationRunner::start_with_open_files(script.project.clone(), &initial_open_files)
                .await;
        runner.check_initial().await;

        for (step_index, step) in script.steps.iter().enumerate() {
            eprintln!("simulation seed {seed} step {step_index}: {step:?}");
            runner.run_edit_script_step(step).await;
        }
    }
}

fn seeded_script_fingerprint(script: &super::seeded::SeededScript) -> String {
    let mut hasher = StableHasher::new();
    let render = script.project.render();

    hasher.write_str("seed:");
    hasher.write_str(&script.seed.to_string());
    hasher.write_str("\nproject:");
    hasher.write_str(&script.project.name);
    hasher.write_str("\nfiles:\n");
    for (file, content) in &render.files {
        hasher.write_str(file);
        hasher.write_str("\0");
        hasher.write_str(content);
        hasher.write_str("\0");
    }
    hasher.write_str("\ninitial_open_files:\n");
    for file in &script.initial_open_files {
        hasher.write_str(file);
        hasher.write_str("\0");
    }
    hasher.write_str("\nedits:\n");
    for edit in &script.project.edits {
        hasher.write_str(&edit.name);
        hasher.write_str("\0");
        hasher.write_str(&format!("{:?}", edit.ops));
        hasher.write_str("\0");
        hasher.write_str(&format!("{:?}", edit.expected));
        hasher.write_str("\0");
    }
    hasher.write_str("\nsteps:\n");
    for step in &script.steps {
        hasher.write_str(&format!("{step:?}"));
        hasher.write_str("\0");
    }

    format!("fnv1a64:{:016x}", hasher.finish())
}

struct StableHasher {
    hash: u64,
}

impl StableHasher {
    fn new() -> Self {
        Self {
            hash: 0xcbf2_9ce4_8422_2325,
        }
    }

    fn write_str(&mut self, value: &str) {
        for byte in value.as_bytes() {
            self.hash ^= u64::from(*byte);
            self.hash = self.hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn finish(&self) -> u64 {
        self.hash
    }
}
