use super::rng::SeededRng;
use super::scenario::seeded_project;
use super::{SeededNames, SeededScript, SeededStep};
use crate::simulation::graph::CallShape;
use crate::simulation::project::{EditStep, SyntheticProject};

pub fn seeded_script(seed: u64) -> SeededScript {
    let project = seeded_project(seed);
    let mut rng = SeededRng::new(seed ^ 0xA11C_E5E5_D15C_A11D);
    let lifecycle_files = lifecycle_files(&project);
    let initial_open_files = initial_open_files(&project);
    let mut open_files = initial_open_files.clone();
    let mut steps = Vec::new();
    let mut edited_project = project.clone();

    steps.push(SeededStep::CheckDefinitions);
    steps.push(SeededStep::CheckReferences);
    steps.push(SeededStep::CheckHover);
    steps.push(SeededStep::CheckTypes);

    for index in 0..project.edits.len() {
        steps.push(random_check_step(&mut rng));
        // These are editor edits: every changed file must receive didChange.
        // A closed file retains its last delivered facts until it is reopened.
        let before = edited_project.render();
        for op in &project.edits[index].ops {
            edited_project.apply_op(op);
        }
        for (file, content) in edited_project.render().files {
            if before.files.get(&file) != Some(&content) && !open_files.contains(&file) {
                open_files.push(file.clone());
                steps.push(SeededStep::OpenFile { file });
            }
        }
        steps.push(SeededStep::ApplyEdit { index });
        steps.push(random_check_step(&mut rng));

        match rng.range_usize(4) {
            0 => {
                if let Some(file) = pick_closed_file(&mut rng, &lifecycle_files, &open_files) {
                    open_files.push(file.clone());
                    steps.push(SeededStep::OpenFile { file });
                }
            }
            1 => {
                if open_files.len() > 1 {
                    let file = remove_random_open_file(&mut rng, &mut open_files);
                    steps.push(SeededStep::CloseFile { file });
                }
            }
            2 => {
                let file = pick_open_file(&mut rng, &open_files);
                steps.push(SeededStep::CloseReopen { file });
            }
            3 => {}
            value => panic!(
                "INVARIANT VIOLATED: seeded lifecycle step index `{}` is impossible. This is a bug because range_usize(4) must return 0..=3. Fix: inspect SeededRng::range_usize.",
                value
            ),
        }
    }

    for file in lifecycle_files {
        if !open_files.contains(&file) {
            steps.push(SeededStep::OpenFile { file: file.clone() });
            open_files.push(file);
        }
    }
    steps.push(SeededStep::CloseReopen {
        file: initial_open_files
            .first()
            .expect("INVARIANT VIOLATED: seeded initial open file list is empty. This is a bug because seeded lifecycle must have at least one open file. Fix: inspect initial_open_files.")
            .clone(),
    });
    steps.push(SeededStep::CheckDefinitions);
    steps.push(SeededStep::CheckReferences);
    steps.push(SeededStep::CheckHover);
    steps.push(SeededStep::CheckTypes);

    SeededScript {
        seed,
        project,
        initial_open_files,
        steps,
    }
}

fn random_check_step(rng: &mut SeededRng) -> SeededStep {
    match rng.range_usize(4) {
        0 => SeededStep::CheckDefinitions,
        1 => SeededStep::CheckReferences,
        2 => SeededStep::CheckHover,
        3 => SeededStep::CheckTypes,
        value => panic!(
            "INVARIANT VIOLATED: seeded check step index `{}` is impossible. This is a bug because range_usize(4) must return 0..=3. Fix: inspect SeededRng::range_usize.",
            value
        ),
    }
}

fn initial_open_files(project: &SyntheticProject) -> Vec<String> {
    let render = project.render();
    let mut files = Vec::new();
    // Exercise host changes while their callers and definitions are open. The
    // random schedule still closes/reopens them; the final open-all pass is not
    // the first time these interaction cases are observed.
    let oracle = super::OracleState::all_files(project, &render.map);
    for call in &render.map.calls {
        if !matches!(call.shape, CallShape::Bare)
            || oracle.instance_receivers(&call.caller.owner).len() < 2
        {
            continue;
        }
        push_unique(&mut files, call.pos.file.clone());
        for receiver in oracle.instance_receivers(&call.caller.owner) {
            if let Some(site) = render.map.namespaces.get(&receiver) {
                push_unique(&mut files, site.pos.file.clone());
            }
        }
        for target in oracle
            .resolve_call_targets(call)
            .iter()
            .chain(std::iter::once(&call.target))
        {
            if let Some(pos) = render.map.defs.get(target) {
                push_unique(&mut files, pos.file.clone());
            }
        }
        for reflected in &render.map.calls {
            if matches!(reflected.shape, CallShape::InstanceMethodObject)
                && reflected.target == call.target
            {
                push_unique(&mut files, reflected.pos.file.clone());
            }
        }
    }
    for step in &project.edits {
        for expected in &step.expected {
            match expected {
                super::project::ExpectedCheck::UnresolvedMethod { file, .. }
                | super::project::ExpectedCheck::NoUnresolvedMethod { file, .. }
                | super::project::ExpectedCheck::UnresolvedConstant { file, .. }
                | super::project::ExpectedCheck::NoUnresolvedConstant { file, .. } => {
                    push_unique(&mut files, file.clone());
                }
                super::project::ExpectedCheck::NoMethodDefinitionTarget {
                    stale_target, ..
                } => {
                    if let Some(pos) = render.map.defs.get(stale_target) {
                        push_unique(&mut files, pos.file.clone());
                    }
                }
                super::project::ExpectedCheck::NoConstantDefinitionTarget {
                    stale_target, ..
                } => {
                    if let Some(pos) = render.map.constants.get(stale_target) {
                        push_unique(&mut files, pos.file.clone());
                    }
                }
            }
        }
    }
    for call in &render.map.calls {
        push_unique(&mut files, call.pos.file.clone());
        if files.len() >= 2 {
            break;
        }
    }
    assert!(
        !files.is_empty(),
        "INVARIANT VIOLATED: seeded initial open set is empty. This is a bug because seeded project must generate queryable files. Fix: inspect seeded_project."
    );
    files
}

fn lifecycle_files(project: &SyntheticProject) -> Vec<String> {
    let render = project.render();
    let mut files = initial_open_files(project);
    for call in &render.map.calls {
        push_unique(&mut files, call.pos.file.clone());
        if let Some(def) = render.map.defs.get(&call.target) {
            push_unique(&mut files, def.file.clone());
        }
        if files.len() >= 5 {
            break;
        }
    }
    for file in render.files.keys() {
        push_unique(&mut files, file.clone());
        if files.len() >= 5 {
            break;
        }
    }
    files
}

fn push_unique(values: &mut Vec<String>, value: String) {
    if !values.contains(&value) {
        values.push(value);
    }
}

fn pick_open_file(rng: &mut SeededRng, open_files: &[String]) -> String {
    assert!(
        !open_files.is_empty(),
        "INVARIANT VIOLATED: seeded open file set is empty. This is a bug because lifecycle generation keeps at least one file open. Fix: inspect seeded_script."
    );
    open_files[rng.range_usize(open_files.len())].clone()
}

fn pick_closed_file(
    rng: &mut SeededRng,
    lifecycle_files: &[String],
    open_files: &[String],
) -> Option<String> {
    let closed = lifecycle_files
        .iter()
        .filter(|file| !open_files.contains(file))
        .cloned()
        .collect::<Vec<_>>();
    if closed.is_empty() {
        return None;
    }
    Some(closed[rng.range_usize(closed.len())].clone())
}

fn remove_random_open_file(rng: &mut SeededRng, open_files: &mut Vec<String>) -> String {
    assert!(
        open_files.len() > 1,
        "INVARIANT VIOLATED: seeded lifecycle tried to close last open file. This is a bug because partial simulation needs at least one queryable file. Fix: inspect seeded_script."
    );
    let index = rng.range_usize(open_files.len());
    open_files.remove(index)
}

pub(super) fn seeded_edit_groups(names: &SeededNames) -> Vec<Vec<EditStep>> {
    let method_delete = EditStep::new("seed delete primary method", |edit| {
        edit.delete_method(&names.capture_target())
            .expect_unresolved_method(&names.invoice_file, &names.capture_method)
            .expect_no_method_definition_target(&names.capture_target(), &names.capture_target());
    });

    let method_restore = EditStep::new("seed restore primary method", |edit| {
        edit.restore_method(&names.capture_target())
            .expect_no_unresolved_method(&names.invoice_file, &names.capture_method);
    });

    let constant_delete = EditStep::new("seed delete primary constant", |edit| {
        edit.delete_constant(&names.currency_constant_fqn)
            .expect_unresolved_constant(&names.invoice_file, &names.currency_constant)
            .expect_no_constant_definition_target(
                &names.currency_constant_fqn,
                &names.currency_constant_fqn,
            );
    });

    let constant_restore = EditStep::new("seed restore primary constant", |edit| {
        edit.restore_constant(&names.currency_constant_fqn)
            .expect_no_unresolved_constant(&names.invoice_file, &names.currency_constant);
    });

    let relation_update = EditStep::new("seed update relation", |edit| {
        edit.remove_include(&names.invoice, &names.trackable)
            .change_superclass(&names.invoice, &names.fallback_gateway);
    });

    vec![
        vec![method_delete, method_restore],
        vec![constant_delete, constant_restore],
        vec![relation_update],
    ]
}
