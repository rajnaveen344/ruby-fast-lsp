use super::{SeededScript, SIM_GENERATOR_VERSION};
use crate::simulation::build_identity::write_build_identity;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

pub fn write_seed_artifact(script: &SeededScript) -> PathBuf {
    static NEXT_ARTIFACT: AtomicU64 = AtomicU64::new(0);
    let artifact_id = NEXT_ARTIFACT.fetch_add(1, Ordering::Relaxed);
    let parent = std::env::temp_dir().join("ruby-fast-lsp-sim");
    fs::create_dir_all(&parent).unwrap_or_else(|err| {
        panic!(
            "INVARIANT VIOLATED: failed to create simulation artifact parent `{}`: {}. This is a bug because replay evidence needs a writable temp directory. Fix: inspect temp dir permissions.",
            parent.display(), err
        )
    });
    let root = parent.join(format!(
        "seed-{}-process-{}-run-{artifact_id}",
        script.seed,
        std::process::id()
    ));
    fs::create_dir(&root).unwrap_or_else(|err| {
        panic!(
            "INVARIANT VIOLATED: failed to reserve unique simulation artifact `{}`: {}. This is a bug because one run must never replace another run's replay evidence. Fix: inspect artifact identity or temp dir permissions.",
            root.display(), err
        )
    });
    write_build_identity(&root);
    fs::create_dir(root.join("files")).unwrap_or_else(|err| {
        panic!(
            "INVARIANT VIOLATED: failed to create simulation artifact dir `{}`: {}. This is a bug because seed artifacts need a writable temp dir. Fix: inspect temp dir permissions.",
            root.display(),
            err
        )
    });

    let render = script.project.render();
    for (file, content) in &render.files {
        let path = root.join("files").join(file);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|err| {
                panic!(
                    "INVARIANT VIOLATED: failed to create simulation artifact subdir `{}`: {}. This is a bug because generated file paths must be writable under the artifact root. Fix: inspect file path generation.",
                    parent.display(),
                    err
                )
            });
        }
        fs::write(&path, content).unwrap_or_else(|err| {
            panic!(
                "INVARIANT VIOLATED: failed to write simulation artifact file `{}`: {}. This is a bug because generated Ruby files must be serializable. Fix: inspect temp dir permissions.",
                path.display(),
                err
            )
        });
    }

    fs::write(root.join("script.txt"), script_description(script)).unwrap_or_else(|err| {
        panic!(
            "INVARIANT VIOLATED: failed to write simulation script artifact for seed `{}`: {}. This is a bug because seeded scripts must be serializable. Fix: inspect temp dir permissions.",
            script.seed,
            err
        )
    });
    fs::write(
        root.join("README.txt"),
        format!(
            "Replay all seeded lifecycle and incremental/fresh checks:\nSIM_SEED={} cargo test test::simulation -- --nocapture\n\nBuild identity:\nbuild.json retains the compiled Rust workspace source manifest, Cargo build metadata, and SHA-256 of the running test executable. Compare these identities before interpreting a replay from another build. The source scope excludes external runtime assets and editor/platform evidence.\n\nPersist regression:\nAdd `{}` to src/test/simulation/support/regression_seeds.txt after reducing the failure.\n",
            script.seed,
            script.seed
        ),
    )
    .unwrap_or_else(|err| {
        panic!(
            "INVARIANT VIOLATED: failed to write simulation README artifact for seed `{}`: {}. This is a bug because seeded replay metadata must be serializable. Fix: inspect temp dir permissions.",
            script.seed,
            err
        )
    });

    root
}

fn script_description(script: &SeededScript) -> String {
    let mut out = String::new();
    let render = script.project.render();
    out.push_str(&format!("sim_generator_version: {SIM_GENERATOR_VERSION}\n"));
    out.push_str(&format!("seed: {}\n", script.seed));
    out.push_str(&format!("project: {}\n", script.project.name));
    out.push_str(&format!(
        "files: {}\nmethods: {}\nedges: {}\n",
        render.files.len(),
        script.project.enabled_method_count(),
        script.project.meaningful_edge_count()
    ));
    out.push_str(&format!(
        "initial_open_files: {:?}\n\n",
        script.initial_open_files
    ));
    out.push_str("edits:\n");
    for (idx, edit) in script.project.edits.iter().enumerate() {
        out.push_str(&format!("  {idx}: {}\n", edit.name));
    }
    out.push_str("\nsteps:\n");
    for (idx, step) in script.steps.iter().enumerate() {
        out.push_str(&format!("  {idx}: {step:?}\n"));
    }
    out
}
