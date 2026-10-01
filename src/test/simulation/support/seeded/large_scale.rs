use super::names::{fqn, title_word, SeededNameAllocator, CONSTANT_WORDS};
use super::rng::SeededRng;
use super::{LARGE_SCALE_METHOD_DEFS, LARGE_SCALE_MIN_GRAPH_EDGES, LARGE_SCALE_RUBY_FILES};
use crate::simulation::graph::CallShape;
use crate::simulation::project::SyntheticProject;

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

pub(super) fn seeded_filler_classes(
    project: &mut SyntheticProject,
    rng: &mut SeededRng,
    count: usize,
) {
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
