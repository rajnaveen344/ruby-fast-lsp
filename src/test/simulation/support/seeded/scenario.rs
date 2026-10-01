use super::large_scale::seeded_filler_classes;
use super::rng::SeededRng;
use super::script::seeded_edit_groups;
use super::SeededNames;
use crate::simulation::graph::CallShape;
use crate::simulation::project::SyntheticProject;

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

    add_module_dispatch_scenario(&mut project, &format!("DispatchSeed{seed}"));

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

/// Internal module sends must follow each concrete host, while reflection stays
/// on the named module. An unrelated definition is deliberate competing noise.
pub(crate) fn add_module_dispatch_scenario(project: &mut SyntheticProject, root: &str) {
    let defaults = format!("{root}::Defaults");
    let feature = format!("{root}::Feature");
    let first = format!("{root}::FirstHost");
    let second = format!("{root}::SecondHost");
    let wrapper = format!("{root}::Wrapper");
    let target = format!("{defaults}#label");
    project
        .module(&defaults, |module| {
            module.method("label");
        })
        .module(&feature, |module| {
            module.include(&defaults);
            module
                .method("invoke_member")
                .calls(&target, CallShape::Bare);
        })
        .module(&wrapper, |module| {
            module.method("label");
        })
        .class(&first, |class| {
            class.include(&feature);
            class.method("label");
        })
        .class(&second, |class| {
            class.include(&feature);
            class.prepend(&wrapper);
            class.method("label");
        })
        .class(&format!("{root}::Unrelated"), |class| {
            class.method("label");
            class
                .method("inspect_module")
                .calls(&target, CallShape::InstanceMethodObject);
        })
        .edit("remove host override", |edit| {
            edit.delete_method(&format!("{first}#label"));
        })
        .edit("remove prepended override", |edit| {
            edit.remove_prepend(&second, &wrapper);
        })
        .edit("detach second receiver", |edit| {
            edit.remove_include(&second, &feature);
        })
        .edit("restore host override", |edit| {
            edit.restore_method(&format!("{first}#label"));
        })
        .edit("restore second receiver", |edit| {
            edit.add_include(&second, &feature);
        })
        .edit("restore prepend", |edit| {
            edit.add_prepend(&second, &wrapper);
        });
}
