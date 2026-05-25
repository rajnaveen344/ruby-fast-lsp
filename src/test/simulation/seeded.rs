use super::graph::CallShape;
use super::project::{EditStep, SyntheticProject};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const SIM_GENERATOR_VERSION: u32 = 37;
const FIXED_SEEDS: &[u64] = &[1, 42, 20_260_524];
const REGRESSION_SEEDS_TEXT: &str = include_str!("regression_seeds.txt");
pub const LARGE_SCALE_RUBY_FILES: usize = 2_284;
pub const LARGE_SCALE_METHOD_DEFS: usize = 23_328;
pub const LARGE_SCALE_MIN_GRAPH_EDGES: usize = 18_000;

#[derive(Debug, Clone)]
pub struct SeededScript {
    pub seed: u64,
    pub project: SyntheticProject,
    pub initial_open_files: Vec<String>,
    pub steps: Vec<SeededStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SeededStep {
    CheckDefinitions,
    CheckReferences,
    CheckHover,
    CheckTypes,
    ApplyEdit { index: usize },
    CloseReopen { file: String },
    OpenFile { file: String },
    CloseFile { file: String },
}

pub fn write_seed_artifact(script: &SeededScript) -> PathBuf {
    let root = std::env::temp_dir()
        .join("ruby-fast-lsp-sim")
        .join(format!("seed-{}", script.seed));
    if root.exists() {
        fs::remove_dir_all(&root).unwrap_or_else(|err| {
            panic!(
                "INVARIANT VIOLATED: failed to remove stale simulation artifact `{}`: {}. This is a bug because seed artifacts must be replaceable. Fix: inspect temp dir permissions.",
                root.display(),
                err
            )
        });
    }
    fs::create_dir_all(root.join("files")).unwrap_or_else(|err| {
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
            "Replay:\nSIM_SEED={} cargo test generated_project_runs_seeded_edit_sequence -- --nocapture\n\nPersist regression:\nAdd `{}` to src/test/simulation/regression_seeds.txt after reducing the failure.\n",
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

pub fn simulation_seeds_from_env() -> Vec<u64> {
    if let Ok(value) = std::env::var("SIM_SEED") {
        return vec![parse_seed("SIM_SEED", &value)];
    }

    if let Ok(value) = std::env::var("SIM_SEEDS") {
        let seeds = value
            .split(',')
            .map(str::trim)
            .filter(|seed| !seed.is_empty())
            .map(|seed| parse_seed("SIM_SEEDS", seed))
            .collect::<Vec<_>>();
        assert!(
            !seeds.is_empty(),
            "INVARIANT VIOLATED: SIM_SEEDS was set but contained no seeds. This is a bug because seeded replay needs at least one numeric seed. Fix: pass SIM_SEEDS=1,42."
        );
        return seeds;
    }

    let random_count = std::env::var("SIM_RANDOM_SEEDS")
        .ok()
        .map(|value| parse_seed_count(&value))
        .unwrap_or(0);
    let mut seeds = FIXED_SEEDS.to_vec();
    seeds.extend(regression_seeds());
    seeds.extend(random_seeds(random_count));
    seeds.sort_unstable();
    seeds.dedup();
    seeds
}

pub fn seeded_script(seed: u64) -> SeededScript {
    let project = seeded_project(seed);
    let mut rng = SeededRng::new(seed ^ 0xA11C_E5E5_D15C_A11D);
    let lifecycle_files = lifecycle_files(&project);
    let initial_open_files = initial_open_files(&project);
    let mut open_files = initial_open_files.clone();
    let mut steps = Vec::new();

    steps.push(SeededStep::CheckDefinitions);
    steps.push(SeededStep::CheckReferences);
    steps.push(SeededStep::CheckHover);
    steps.push(SeededStep::CheckTypes);

    for index in 0..project.edits.len() {
        steps.push(random_check_step(&mut rng));
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

pub fn seeded_project(seed: u64) -> SyntheticProject {
    let mut rng = SeededRng::new(seed);
    let names = SeededNames::new(&mut rng);
    let mut project = SyntheticProject::new(&format!("seeded_{seed}"));

    let capture_shape = match rng.range_usize(4) {
        0 => CallShape::local(&names.gateway_local),
        1 => CallShape::ConstructorSend,
        2 => CallShape::receiver_local(&names.gateway_local, &names.gateway),
        3 => CallShape::StaticSend,
        value => panic!(
            "INVARIANT VIOLATED: seeded call shape index `{}` is impossible. This is a bug because range_usize(4) must return 0..=3. Fix: inspect SeededRng::range_usize.",
            value
        ),
    };
    let publish_shape = match rng.range_usize(3) {
        0 => CallShape::ConstructorSend,
        1 => CallShape::local(&names.item_local),
        2 => CallShape::StaticSend,
        value => panic!(
            "INVARIANT VIOLATED: seeded publish shape index `{}` is impossible. This is a bug because range_usize(3) must return 0..=2. Fix: inspect SeededRng::range_usize.",
            value
        ),
    };

    project
        .module(&names.trackable_hook_module, |module| {
            module.extend_self();
            module.method(&names.hook_status_method).returns("String");
        })
        .module(&names.trackable_hook_include_module, |module| {
            module.method(&names.hook_render_method).returns("String");
        })
        .module(&names.trackable_hook_class_eval_module, |module| {
            module.method(&names.hook_api_method).returns("String");
        })
        .module(&names.trackable_concern_class_methods_module, |module| {
            module
                .method(&names.concern_lookup_method)
                .returns("String");
        })
        .module(&names.visibility_hidden_mixin, |module| {
            module
                .method(&names.visibility_hidden_method)
                .returns("String");
        })
        .class(&names.visibility_hidden_user, |class| {
            class.include(&names.visibility_hidden_mixin);
            class.private_visibility(&names.visibility_hidden_method);
        })
        .module(&names.visibility_public_mixin, |module| {
            module
                .method(&names.visibility_public_method)
                .returns("String")
                .private();
        })
        .class(&names.visibility_public_user, |class| {
            class.include(&names.visibility_public_mixin);
            class.public_visibility(&names.visibility_public_method);
        })
        .module(&names.trackable, |module| {
            module.included_hook_extend(&names.trackable_hook_module);
            module.included_hook_include(&names.trackable_hook_include_module);
            module.included_hook_class_eval_include(&names.trackable_hook_class_eval_module);
            module.concern_class_methods(&names.trackable_concern_class_methods_module);
            module.constant(&names.level_constant, "\"v\"");
            module
                .method(&names.audit_method)
                .ref_const(&names.level_constant_fqn);
            module.method(&names.record_method);
            module.method(&names.tagged_method);
        })
        .class(&names.gateway, |class| {
            class.extend(&names.trackable);
            class.singleton_include(&names.trackable);
            class.constant(&names.provider_constant, "\"v\"");
            class
                .method(&names.capture_method)
                .returns("String")
                .in_class_eval_block();
            class
                .method(&names.refund_method)
                .returns("String")
                .as_define_method();
            class
                .method(&names.const_get_method)
                .returns("String")
                .as_const_get_define_method();
            class.method(&names.void_method);
            class
                .method(&names.private_method)
                .returns("String")
                .private()
                .visibility_argument_list();
            class
                .method(&names.private_probe_method)
                .returns("String")
                .calls(&names.private_target(), CallShape::Bare)
                .calls(&names.private_target(), CallShape::ConstructorSend)
                .calls(&names.private_target(), CallShape::StaticSend);
            class
                .class_method(&names.default_method)
                .returns(&names.gateway);
            class
                .class_method(&names.provider_method)
                .returns("String")
                .ref_const(&names.provider_constant_fqn)
                .in_singleton_class_block();
        })
        .class(&names.fallback_gateway, |class| {
            class.superclass(&names.gateway);
            class.include(&names.trackable);
            class.method(&names.capture_method).returns("String");
            class.method(&names.queue_method);
            class.method(&names.normalize_method);
        })
        .class(&names.base_invoice, |class| {
            class.constant(&names.base_status_constant, "\"v\"");
            class.method(&names.normalize_method);
            class
                .method(&names.base_status_method)
                .ref_const(&names.base_status_constant_fqn);
            class.method(&names.super_method).returns("String");
        })
        .class(&names.account, |class| {
            class.method(&names.gateway_method).returns(&names.gateway);
            class
                .method(&names.backup_gateway_method)
                .returns(&names.fallback_gateway);
        })
        .class(&names.account, |class| {
            class.file_path(&names.account_reopen_file);
            class
                .method(&names.account_reopen_method)
                .returns("String")
                .ref_const(&names.base_status_constant_fqn);
        })
        .class(&names.invoice, |class| {
            class.superclass(&names.base_invoice);
            class.include(&names.trackable);
            class.constant(&names.currency_constant, "\"v\"");
            class.method(&names.gateway_method).returns(&names.gateway);
            class
                .method(&names.super_method)
                .returns("String")
                .calls(&names.super_target(), CallShape::Super);
            class.delegate_instance_method(&names.delegated_capture_method, &names.gateway_method);
            class
                .method(&names.charge_method)
                .returns("String")
                .with_block_type_asserts()
                .ref_const(&names.currency_constant_fqn)
                .ref_const_const_get(&names.provider_constant_fqn)
                .ref_const_const_defined(&names.provider_constant_fqn)
                .calls(
                    &names.capture_target(),
                    CallShape::array_block_param(&names.block_item_local),
                )
                .calls(
                    &names.capture_target(),
                    CallShape::yield_block_param(&names.yield_item_local),
                )
                .calls(&names.gateway_target(), CallShape::MethodObject)
                .calls(&names.capture_target(), capture_shape)
                .calls(&names.capture_target(), CallShape::InstanceMethodObject)
                .calls(
                    &names.visibility_hidden_target(),
                    CallShape::receiver_local(
                        &names.visibility_hidden_local,
                        &names.visibility_hidden_user,
                    ),
                )
                .calls(
                    &names.visibility_public_target(),
                    CallShape::receiver_local(
                        &names.visibility_public_local,
                        &names.visibility_public_user,
                    ),
                )
                .calls(
                    &names.account_reopen_target(),
                    CallShape::receiver_local(&names.account_reopen_local, &names.account),
                )
                .calls(&names.refund_target(), CallShape::ivar(&names.gateway_ivar))
                .calls(
                    &names.const_get_target(),
                    CallShape::local(&names.gateway_local),
                )
                .calls(&names.default_target(), CallShape::ClassSend)
                .calls(&names.default_target(), CallShape::MethodObject)
                .calls(&names.audit_target(), CallShape::Bare)
                .calls(&names.hook_render_target(), CallShape::Bare)
                .calls(&names.hook_api_target(), CallShape::Bare)
                .calls(&names.normalize_target(), CallShape::Bare);
            class
                .method(&names.block_scoped_method)
                .returns("String")
                .calls(&names.audit_target(), CallShape::BareInDoBlock)
                .calls(&names.record_target(), CallShape::BareInBraceBlock)
                .calls(&names.tagged_target(), CallShape::BareInLambda)
                .calls(&names.normalize_target(), CallShape::BareInProc);
            class.method(&names.chain_charge_method).calls(
                &names.capture_target(),
                CallShape::one_hop(&names.account_local, &names.account, &names.gateway_method),
            );
            class
                .method(&names.constructor_charge_method)
                .calls(&names.capture_target(), CallShape::ConstructorSend);
        })
        .class(&names.sku, |class| {
            class.include(&names.trackable);
            class.constant(&names.prefix_constant, "\"v\"");
            class
                .method(&names.format_method)
                .returns("String")
                .ref_const(&names.prefix_constant_fqn);
            class
                .method(&names.audit_sku_method)
                .calls(&names.audit_target(), CallShape::Bare);
        })
        .class(&names.item, |class| {
            class.method(&names.sku_method).returns(&names.sku);
            class
                .method(&names.publish_method)
                .returns("String")
                .calls(&names.format_target(), publish_shape);
        })
        .class(&names.dynamic_record, |class| {
            class.method("method_missing").returns("String");
        })
        .class(&names.summary, |class| {
            class
                .method(&names.render_method)
                .returns("String")
                .calls(&names.charge_target(), CallShape::ConstructorSend)
                .calls(
                    &names.delegated_capture_target(),
                    CallShape::ConstructorSend,
                )
                .calls(&names.publish_target(), CallShape::ConstructorSend);
            class
                .method(&names.capture_total_method)
                .calls(
                    &names.capture_target(),
                    CallShape::local(&names.gateway_local),
                )
                .calls(
                    &names.audit_target(),
                    CallShape::class_receiver(&names.gateway),
                )
                .calls(
                    &names.hook_status_target(),
                    CallShape::class_receiver(&names.invoice),
                )
                .calls(
                    &names.concern_lookup_target(),
                    CallShape::class_receiver(&names.invoice),
                )
                .calls(&names.hook_status_target(), CallShape::ClassSend)
                .calls(&names.dynamic_virtual_target(), CallShape::ConstructorSend);
        });

    let filler_count = 40 + rng.range_usize(5);
    seeded_filler_classes(&mut project, &mut rng, filler_count);

    let mut groups = seeded_edit_groups(&names);
    rng.shuffle(&mut groups);
    for group in groups {
        for step in group {
            project.edits.push(step);
        }
    }

    project
}

pub fn large_scale_project(seed: u64) -> SyntheticProject {
    let mut rng = SeededRng::new(seed);
    let root = title_word(CONSTANT_WORDS[rng.range_usize(CONSTANT_WORDS.len())]);
    let mut project = SyntheticProject::new(&format!("large_scale_{seed}"));
    let mixin_count = 128;
    let class_count = LARGE_SCALE_RUBY_FILES - mixin_count;
    let mixin_methods = 3;
    let class_method_target = LARGE_SCALE_METHOD_DEFS - mixin_count * mixin_methods;
    let base_methods_per_class = class_method_target / class_count;
    let extra_method_classes = class_method_target % class_count;

    assert_eq!(
        LARGE_SCALE_RUBY_FILES, mixin_count + class_count,
        "INVARIANT VIOLATED: large-scale file math is wrong. This is a bug because scale simulation must be deterministic. Fix: update scale constants together."
    );
    assert!(
        base_methods_per_class >= 10,
        "INVARIANT VIOLATED: large-scale class method density is too low. This is a bug because scale simulation needs every call shape. Fix: raise method target or lower file target."
    );

    let mixins = (0..mixin_count)
        .map(|idx| format!("{root}ScaleMixins::Mixin{idx:04}"))
        .collect::<Vec<_>>();
    let classes = (0..class_count)
        .map(|idx| format!("{root}Domain{:02}::Model{idx:04}", idx % 64))
        .collect::<Vec<_>>();

    for (idx, mixin_fqn) in mixins.iter().enumerate() {
        let token = format!("LEVEL_{idx:04}");
        let token_fqn = format!("{mixin_fqn}::{token}");
        let touch = scale_method(idx, 0);
        let record = scale_method(idx, 1);
        let tag = scale_method(idx, 2);
        let touch_target = format!("{mixin_fqn}#{touch}");
        project.module(mixin_fqn, |module| {
            module.constant(&token, "\"info\"");
            module
                .method(&touch)
                .returns("String")
                .ref_const(&token_fqn);
            module
                .method(&record)
                .returns("String")
                .calls(&touch_target, CallShape::BareInDoBlock);
            module
                .method(&tag)
                .returns("String")
                .calls(&touch_target, CallShape::BareInLambda)
                .ref_const(&token_fqn);
        });
    }

    for (idx, class_fqn) in classes.iter().enumerate() {
        let method_count = base_methods_per_class + usize::from(idx < extra_method_classes);
        let previous_idx = idx.saturating_sub(1);
        let previous_fqn = &classes[previous_idx];
        let mixin_fqn = &mixins[idx % mixins.len()];
        let prepend_fqn = &mixins[(idx + 17) % mixins.len()];
        let token = format!("TOKEN_{idx:04}");
        let token_fqn = format!("{class_fqn}::{token}");
        let value = scale_method(idx, 0);
        let previous_value = scale_method(previous_idx, 0);
        let local = scale_method(idx, 1);
        let constructor = scale_method(idx, 2);
        let factory = scale_method(idx, 3);
        let previous_factory = scale_method(previous_idx, 3);
        let class_send = scale_method(idx, 4);
        let ivar = scale_method(idx, 5);
        let hop = scale_method(idx, 6);
        let one_hop = scale_method(idx, 7);
        let mixin_call = scale_method(idx, 8);
        let super_probe = "flow_super_probe".to_string();
        let previous_value_target = format!("{previous_fqn}#{previous_value}");
        let previous_factory_target = format!("{previous_fqn}.{previous_factory}");
        let mixin_touch_target = format!("{}#{}", mixin_fqn, scale_method(idx % mixins.len(), 0));
        let prepend_touch_target = format!(
            "{}#{}",
            prepend_fqn,
            scale_method((idx + 17) % mixins.len(), 0)
        );
        let self_value_target = format!("{class_fqn}#{value}");

        project.class(class_fqn, |class| {
            if idx > 0 && idx % 5 == 0 {
                class.superclass(previous_fqn);
            }
            class.include(mixin_fqn);
            if idx % 11 == 0 {
                class.prepend(prepend_fqn);
            }
            if idx % 13 == 0 {
                class.extend(mixin_fqn);
            }
            if idx % 17 == 0 {
                class.singleton_include(mixin_fqn);
            }
            if idx % 19 == 0 {
                class.singleton_prepend(prepend_fqn);
            }
            class.constant(&token, "\"token\"");
            let value_method = class.method(&value).returns("String").ref_const(&token_fqn);
            if idx % 13 == 0 {
                value_method.in_class_eval_block();
            } else if idx % 11 == 0 {
                value_method.as_define_method();
            }
            class
                .method(&local)
                .returns("String")
                .calls(&previous_value_target, CallShape::local("receiver"));
            class.method(&constructor).returns("String").calls(
                &previous_value_target,
                if idx % 23 == 0 {
                    CallShape::StaticSend
                } else {
                    CallShape::ConstructorSend
                },
            );
            let factory_method = class.class_method(&factory).returns(class_fqn);
            if idx % 7 == 0 {
                factory_method.in_singleton_class_block();
            }
            class
                .method(&class_send)
                .returns(class_fqn)
                .calls(&previous_factory_target, CallShape::ClassSend);
            class
                .method(&ivar)
                .returns("String")
                .calls(&previous_value_target, CallShape::ivar("receiver"));
            class.method(&hop).returns(previous_fqn);
            class.method(&one_hop).returns("String").calls(
                &previous_value_target,
                CallShape::one_hop("link", class_fqn, &hop),
            );
            let (mixin_call_target, mixin_call_shape) = if idx % 19 == 0 {
                (&prepend_touch_target, CallShape::class_receiver(class_fqn))
            } else if idx % 13 == 0 || idx % 17 == 0 {
                (&mixin_touch_target, CallShape::class_receiver(class_fqn))
            } else {
                (&mixin_touch_target, CallShape::Bare)
            };
            class
                .method(&mixin_call)
                .returns("String")
                .calls(mixin_call_target, mixin_call_shape);
            if idx > 0 && idx % 5 == 0 {
                class
                    .method(&super_probe)
                    .returns("String")
                    .calls(&format!("{previous_fqn}#{super_probe}"), CallShape::Super);
            } else {
                class
                    .method(&super_probe)
                    .returns("String")
                    .calls(&self_value_target, CallShape::Bare);
            }

            for extra_idx in 10..method_count {
                let extra = scale_method(idx, extra_idx);
                class
                    .method(&extra)
                    .returns("String")
                    .calls(
                        &previous_value_target,
                        CallShape::receiver_local("receiver", previous_fqn),
                    )
                    .ref_const(&token_fqn);
            }
        });
    }

    assert_eq!(
        project.namespaces.len(),
        LARGE_SCALE_RUBY_FILES,
        "INVARIANT VIOLATED: large-scale project generated the wrong file count. This is a bug because scale smoke must track the measured corpus. Fix: inspect large_scale_project."
    );
    assert_eq!(
        project.enabled_method_count(),
        LARGE_SCALE_METHOD_DEFS,
        "INVARIANT VIOLATED: large-scale project generated the wrong method count. This is a bug because scale smoke must track the measured corpus. Fix: inspect large_scale_project."
    );
    assert!(
        project.meaningful_edge_count() >= LARGE_SCALE_MIN_GRAPH_EDGES,
        "INVARIANT VIOLATED: large-scale project has too few graph edges. This is a bug because scale smoke must exercise semantic refs, not dead files. Fix: add calls/refs to generated methods."
    );

    project
}

fn scale_method(namespace_idx: usize, method_idx: usize) -> String {
    format!("flow_{namespace_idx:04}_{method_idx:02}")
}

fn seeded_filler_classes(project: &mut SyntheticProject, rng: &mut SeededRng, count: usize) {
    let mut allocator = SeededNameAllocator::new();
    let root = allocator.constant(rng);
    let mut previous_target: Option<String> = None;

    for idx in 0..count {
        let class_fqn = loop {
            let candidate = fqn(&[&root, &allocator.constant(rng)]);
            if !project.namespace_enabled(&candidate) {
                break candidate;
            }
        };
        let token_constant = allocator.constant(rng).to_ascii_uppercase();
        let token_constant_fqn = format!("{class_fqn}::{token_constant}");
        let ping_method = allocator.method(rng);
        let build_method = allocator.method(rng);
        let relay_method = allocator.method(rng);
        let previous_local = allocator.local(rng);

        project.class(&class_fqn, |class| {
            class.constant(&token_constant, &format!("\"token-{idx}\""));
            let ping = class.method(&ping_method);
            if idx < 10 {
                ping.ref_const(&token_constant_fqn);
            }
            if idx % 5 == 0 {
                ping.in_class_eval_block();
            } else if idx % 7 == 0 {
                ping.as_define_method();
            }
            if idx % 3 == 0 {
                class.class_method(&build_method).in_singleton_class_block();
            } else {
                class.class_method(&build_method);
            }
            if idx <= 18 {
                if let Some(previous_target) = &previous_target {
                    class
                        .method(&relay_method)
                        .calls(previous_target, CallShape::local(&previous_local));
                }
            }
        });

        previous_target = Some(format!("{class_fqn}#{ping_method}"));
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

fn parse_seed(env_name: &str, value: &str) -> u64 {
    value.parse::<u64>().unwrap_or_else(|err| {
        panic!(
            "INVARIANT VIOLATED: {} `{}` is not a u64: {}. This is a bug because seeded simulation replay needs numeric seeds. Fix: pass {}=<u64>.",
            env_name, value, err, env_name
        )
    })
}

fn regression_seeds() -> Vec<u64> {
    REGRESSION_SEEDS_TEXT
        .lines()
        .enumerate()
        .filter_map(|(line_idx, line)| {
            let seed = line.split('#').next().unwrap_or("").trim();
            if seed.is_empty() {
                None
            } else {
                Some(parse_seed(
                    &format!("src/test/simulation/regression_seeds.txt:{}", line_idx + 1),
                    seed,
                ))
            }
        })
        .collect()
}

fn parse_seed_count(value: &str) -> usize {
    let count = value.parse::<usize>().unwrap_or_else(|err| {
        panic!(
            "INVARIANT VIOLATED: SIM_RANDOM_SEEDS `{}` is not a usize: {}. This is a bug because random simulation needs a numeric count. Fix: pass SIM_RANDOM_SEEDS=<count>.",
            value, err
        )
    });
    assert!(
        count <= 100,
        "INVARIANT VIOLATED: SIM_RANDOM_SEEDS `{}` is too large. This is a bug because unit tests must stay bounded. Fix: run a smaller count or move soak testing to a dedicated command.",
        count
    );
    count
}

fn random_seeds(count: usize) -> Vec<u64> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|err| {
            panic!(
                "INVARIANT VIOLATED: system clock is before UNIX_EPOCH: {}. This is a bug because random simulation seeds need monotonic-ish entropy. Fix: check system clock.",
                err
            )
        });
    let mut rng = SeededRng::new(now.as_nanos() as u64 ^ now.as_secs().rotate_left(17));
    (0..count).map(|_| rng.next_u64()).collect()
}

fn initial_open_files(project: &SyntheticProject) -> Vec<String> {
    let render = project.render();
    let mut files = Vec::new();
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

fn seeded_edit_groups(names: &SeededNames) -> Vec<Vec<EditStep>> {
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

#[derive(Debug, Clone)]
struct SeededNames {
    trackable: String,
    trackable_hook_module: String,
    trackable_hook_include_module: String,
    trackable_hook_class_eval_module: String,
    trackable_concern_class_methods_module: String,
    visibility_hidden_mixin: String,
    visibility_hidden_user: String,
    visibility_public_mixin: String,
    visibility_public_user: String,
    gateway: String,
    fallback_gateway: String,
    base_invoice: String,
    account: String,
    invoice: String,
    sku: String,
    item: String,
    dynamic_record: String,
    summary: String,
    invoice_file: String,
    account_reopen_file: String,
    level_constant: String,
    level_constant_fqn: String,
    provider_constant: String,
    provider_constant_fqn: String,
    base_status_constant: String,
    base_status_constant_fqn: String,
    currency_constant: String,
    currency_constant_fqn: String,
    prefix_constant: String,
    prefix_constant_fqn: String,
    audit_method: String,
    hook_status_method: String,
    hook_render_method: String,
    hook_api_method: String,
    concern_lookup_method: String,
    visibility_hidden_method: String,
    visibility_public_method: String,
    record_method: String,
    tagged_method: String,
    capture_method: String,
    refund_method: String,
    const_get_method: String,
    default_method: String,
    void_method: String,
    private_method: String,
    private_probe_method: String,
    provider_method: String,
    queue_method: String,
    normalize_method: String,
    super_method: String,
    base_status_method: String,
    gateway_method: String,
    backup_gateway_method: String,
    account_reopen_method: String,
    charge_method: String,
    delegated_capture_method: String,
    block_scoped_method: String,
    chain_charge_method: String,
    constructor_charge_method: String,
    format_method: String,
    audit_sku_method: String,
    sku_method: String,
    publish_method: String,
    render_method: String,
    capture_total_method: String,
    dynamic_virtual_method: String,
    gateway_local: String,
    gateway_ivar: String,
    item_local: String,
    account_local: String,
    block_item_local: String,
    yield_item_local: String,
    visibility_hidden_local: String,
    visibility_public_local: String,
    account_reopen_local: String,
}

impl SeededNames {
    fn new(rng: &mut SeededRng) -> Self {
        let mut allocator = SeededNameAllocator::new();
        let audit_root = allocator.constant(rng);
        let payment_root = allocator.constant(rng);
        let billing_root = allocator.constant(rng);
        let catalog_root = allocator.constant(rng);
        let reporting_root = allocator.constant(rng);

        let trackable = fqn(&[&audit_root, &allocator.constant(rng)]);
        let trackable_hook_module = fqn(&[&trackable, &allocator.constant(rng)]);
        let trackable_hook_include_module = fqn(&[&trackable, &allocator.constant(rng)]);
        let trackable_hook_class_eval_module = fqn(&[&trackable, &allocator.constant(rng)]);
        let trackable_concern_class_methods_module = fqn(&[&trackable, &allocator.constant(rng)]);
        let visibility_hidden_mixin = fqn(&[&audit_root, &allocator.constant(rng)]);
        let visibility_hidden_user = fqn(&[&audit_root, &allocator.constant(rng)]);
        let visibility_public_mixin = fqn(&[&audit_root, &allocator.constant(rng)]);
        let visibility_public_user = fqn(&[&audit_root, &allocator.constant(rng)]);
        let gateway = fqn(&[&payment_root, &allocator.constant(rng)]);
        let fallback_gateway = fqn(&[&payment_root, &allocator.constant(rng)]);
        let base_invoice = fqn(&[&billing_root, &allocator.constant(rng)]);
        let account = fqn(&[&billing_root, &allocator.constant(rng)]);
        let invoice = fqn(&[&billing_root, &allocator.constant(rng)]);
        let sku = fqn(&[&catalog_root, &allocator.constant(rng)]);
        let item = fqn(&[&catalog_root, &allocator.constant(rng)]);
        let summary = fqn(&[&reporting_root, &allocator.constant(rng)]);

        let level_constant = allocator.constant(rng).to_ascii_uppercase();
        let provider_constant = allocator.constant(rng).to_ascii_uppercase();
        let base_status_constant = allocator.constant(rng).to_ascii_uppercase();
        let currency_constant = allocator.constant(rng).to_ascii_uppercase();
        let prefix_constant = allocator.constant(rng).to_ascii_uppercase();

        let audit_method = allocator.method(rng);
        let hook_status_method = allocator.method(rng);
        let hook_render_method = allocator.method(rng);
        let hook_api_method = allocator.method(rng);
        let concern_lookup_method = allocator.method(rng);
        let visibility_hidden_method = allocator.method(rng);
        let visibility_public_method = allocator.method(rng);
        let record_method = allocator.method(rng);
        let tagged_method = allocator.method(rng);
        let capture_method = allocator.method(rng);
        let refund_method = allocator.method(rng);
        let const_get_method = allocator.method(rng);
        let default_method = allocator.method(rng);
        let void_method = allocator.method(rng);
        let private_method = allocator.method(rng);
        let private_probe_method = allocator.method(rng);
        let provider_method = allocator.method(rng);
        let queue_method = allocator.method(rng);
        let normalize_method = allocator.method(rng);
        let super_method = allocator.method(rng);
        let base_status_method = allocator.method(rng);
        let gateway_method = allocator.method(rng);
        let backup_gateway_method = allocator.method(rng);
        let account_reopen_method = allocator.method(rng);
        let charge_method = allocator.method(rng);
        let delegated_capture_method = allocator.method(rng);
        let block_scoped_method = allocator.method(rng);
        let chain_charge_method = allocator.method(rng);
        let constructor_charge_method = allocator.method(rng);
        let format_method = allocator.method(rng);
        let audit_sku_method = allocator.method(rng);
        let sku_method = allocator.method(rng);
        let publish_method = allocator.method(rng);
        let render_method = allocator.method(rng);
        let capture_total_method = allocator.method(rng);
        let dynamic_record = fqn(&[&reporting_root, &allocator.constant(rng)]);
        let dynamic_virtual_method = allocator.method(rng);

        Self {
            invoice_file: file_for_fqn(&invoice),
            account_reopen_file: reopen_file_for_fqn(&account),
            level_constant_fqn: format!("{trackable}::{level_constant}"),
            provider_constant_fqn: format!("{gateway}::{provider_constant}"),
            base_status_constant_fqn: format!("{base_invoice}::{base_status_constant}"),
            currency_constant_fqn: format!("{invoice}::{currency_constant}"),
            prefix_constant_fqn: format!("{sku}::{prefix_constant}"),
            trackable,
            trackable_hook_module,
            trackable_hook_include_module,
            trackable_hook_class_eval_module,
            trackable_concern_class_methods_module,
            visibility_hidden_mixin,
            visibility_hidden_user,
            visibility_public_mixin,
            visibility_public_user,
            gateway,
            fallback_gateway,
            base_invoice,
            account,
            invoice,
            sku,
            item,
            dynamic_record,
            summary,
            level_constant,
            provider_constant,
            base_status_constant,
            currency_constant,
            prefix_constant,
            audit_method,
            hook_status_method,
            hook_render_method,
            hook_api_method,
            concern_lookup_method,
            visibility_hidden_method,
            visibility_public_method,
            record_method,
            tagged_method,
            capture_method,
            refund_method,
            const_get_method,
            default_method,
            void_method,
            private_method,
            private_probe_method,
            provider_method,
            queue_method,
            normalize_method,
            super_method,
            base_status_method,
            gateway_method,
            backup_gateway_method,
            account_reopen_method,
            charge_method,
            delegated_capture_method,
            block_scoped_method,
            chain_charge_method,
            constructor_charge_method,
            format_method,
            audit_sku_method,
            sku_method,
            publish_method,
            render_method,
            capture_total_method,
            dynamic_virtual_method,
            gateway_local: allocator.local(rng),
            gateway_ivar: allocator.local(rng),
            item_local: allocator.local(rng),
            account_local: allocator.local(rng),
            block_item_local: allocator.local(rng),
            yield_item_local: allocator.local(rng),
            visibility_hidden_local: allocator.local(rng),
            visibility_public_local: allocator.local(rng),
            account_reopen_local: allocator.local(rng),
        }
    }

    fn capture_target(&self) -> String {
        format!("{}#{}", self.gateway, self.capture_method)
    }

    fn refund_target(&self) -> String {
        format!("{}#{}", self.gateway, self.refund_method)
    }

    fn const_get_target(&self) -> String {
        format!("{}#{}", self.gateway, self.const_get_method)
    }

    fn private_target(&self) -> String {
        format!("{}#{}", self.gateway, self.private_method)
    }

    fn default_target(&self) -> String {
        format!("{}.{}", self.gateway, self.default_method)
    }

    fn gateway_target(&self) -> String {
        format!("{}#{}", self.invoice, self.gateway_method)
    }

    fn account_reopen_target(&self) -> String {
        format!("{}#{}", self.account, self.account_reopen_method)
    }

    fn audit_target(&self) -> String {
        format!("{}#{}", self.trackable, self.audit_method)
    }

    fn hook_status_target(&self) -> String {
        format!("{}#{}", self.trackable_hook_module, self.hook_status_method)
    }

    fn hook_render_target(&self) -> String {
        format!(
            "{}#{}",
            self.trackable_hook_include_module, self.hook_render_method
        )
    }

    fn hook_api_target(&self) -> String {
        format!(
            "{}#{}",
            self.trackable_hook_class_eval_module, self.hook_api_method
        )
    }

    fn concern_lookup_target(&self) -> String {
        format!(
            "{}#{}",
            self.trackable_concern_class_methods_module, self.concern_lookup_method
        )
    }

    fn visibility_hidden_target(&self) -> String {
        format!(
            "{}#{}",
            self.visibility_hidden_mixin, self.visibility_hidden_method
        )
    }

    fn visibility_public_target(&self) -> String {
        format!(
            "{}#{}",
            self.visibility_public_mixin, self.visibility_public_method
        )
    }

    fn record_target(&self) -> String {
        format!("{}#{}", self.trackable, self.record_method)
    }

    fn tagged_target(&self) -> String {
        format!("{}#{}", self.trackable, self.tagged_method)
    }

    fn normalize_target(&self) -> String {
        format!("{}#{}", self.base_invoice, self.normalize_method)
    }

    fn super_target(&self) -> String {
        format!("{}#{}", self.base_invoice, self.super_method)
    }

    fn format_target(&self) -> String {
        format!("{}#{}", self.sku, self.format_method)
    }

    fn charge_target(&self) -> String {
        format!("{}#{}", self.invoice, self.charge_method)
    }

    fn delegated_capture_target(&self) -> String {
        format!("{}#{}", self.invoice, self.delegated_capture_method)
    }

    fn publish_target(&self) -> String {
        format!("{}#{}", self.item, self.publish_method)
    }

    fn dynamic_virtual_target(&self) -> String {
        format!("{}#{}", self.dynamic_record, self.dynamic_virtual_method)
    }
}

struct SeededNameAllocator {
    used_constants: Vec<String>,
    used_methods: Vec<String>,
    used_locals: Vec<String>,
}

impl SeededNameAllocator {
    fn new() -> Self {
        Self {
            used_constants: Vec::new(),
            used_methods: Vec::new(),
            used_locals: Vec::new(),
        }
    }

    fn constant(&mut self, rng: &mut SeededRng) -> String {
        loop {
            let value = format!("N{:016x}", rng.next_u64());
            if !self.used_constants.contains(&value) {
                self.used_constants.push(value.clone());
                return value;
            }
        }
    }

    fn method(&mut self, rng: &mut SeededRng) -> String {
        loop {
            let value = format!("m_{:016x}", rng.next_u64());
            if !RUBY_RESERVED.contains(&value.as_str()) && !self.used_methods.contains(&value) {
                self.used_methods.push(value.clone());
                return value;
            }
        }
    }

    fn local(&mut self, rng: &mut SeededRng) -> String {
        loop {
            let value = format!("v_{:016x}", rng.next_u64());
            if !RUBY_RESERVED.contains(&value.as_str()) && !self.used_locals.contains(&value) {
                self.used_locals.push(value.clone());
                return value;
            }
        }
    }
}

const CONSTANT_WORDS: &[&str] = &[
    "atlas", "bravo", "cedar", "delta", "ember", "fable", "garnet", "harbor", "ion", "juno",
    "kava", "lumen", "mango", "nero", "onyx", "pavo", "quartz", "riven", "sable", "tavo",
];

const RUBY_RESERVED: &[&str] = &[
    "alias", "and", "begin", "break", "case", "class", "def", "defined", "do", "else", "elsif",
    "end", "ensure", "false", "for", "if", "in", "module", "next", "nil", "not", "or", "redo",
    "rescue", "retry", "return", "self", "super", "then", "true", "undef", "unless", "until",
    "when", "while", "yield",
];

fn title_word(input: &str) -> String {
    let mut chars = input.chars();
    let first = chars
        .next()
        .expect("INVARIANT VIOLATED: empty seed word. This is a bug because name word lists must contain non-empty words. Fix: inspect CONSTANT_WORDS.");
    format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
}

fn fqn(parts: &[&str]) -> String {
    parts.join("::")
}

fn file_for_fqn(fqn: &str) -> String {
    let path = fqn
        .split("::")
        .map(underscore)
        .collect::<Vec<_>>()
        .join("/");
    format!("{path}.rb")
}

fn reopen_file_for_fqn(fqn: &str) -> String {
    let base = file_for_fqn(fqn);
    let Some(stem) = base.strip_suffix(".rb") else {
        panic!(
            "INVARIANT VIOLATED: generated Ruby file `{}` does not end in .rb. This is a bug because reopened namespace files derive from Ruby paths. Fix: inspect file_for_fqn.",
            base
        )
    };
    format!("{stem}_reopen.rb")
}

fn underscore(input: &str) -> String {
    let mut out = String::new();
    for (idx, ch) in input.chars().enumerate() {
        if ch.is_uppercase() && idx > 0 {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

struct SeededRng {
    state: u64,
}

impl SeededRng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed ^ 0x9E37_79B9_7F4A_7C15,
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.state
    }

    fn range_usize(&mut self, upper: usize) -> usize {
        assert!(
            upper > 0,
            "INVARIANT VIOLATED: seeded RNG upper bound is zero. This is a bug because random ranges must have at least one value. Fix: pass a positive upper bound."
        );
        (self.next_u64() as usize) % upper
    }

    fn shuffle<T>(&mut self, values: &mut [T]) {
        for idx in (1..values.len()).rev() {
            let swap_idx = self.range_usize(idx + 1);
            values.swap(idx, swap_idx);
        }
    }
}
