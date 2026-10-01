use super::hierarchy_projects::{
    class_eval_project, eval_constant_scope_project, mro_prepend_project, mro_project,
    singleton_class_block_project, super_call_project,
};
use super::location_matches_pos;
use super::metaprogramming_projects::{
    alias_project, class_attribute_project, concern_project, const_defined_constant_ref_project,
    const_get_constant_ref_project, const_get_define_method_project, define_method_project,
    delegate_project, extend_class_method_project, extend_self_project, forwardable_project,
    framework_route_block_project, included_hook_project, method_missing_project,
    method_object_project, module_function_mode_project, singleton_class_mixin_project,
    static_send_project, visibility_project,
};
use super::{phase1_project, superclass_switch_project};
use crate::test::harness::FakeEditor;
use crate::test::simulation::ruby_gen::UNPROVEN_BLOCK_RECEIVER_GAP;
use crate::test::simulation::{EditStep, OracleState, SimulationRunner};

#[tokio::test]
async fn generated_eval_constant_auto_refs_respect_lexical_scope() {
    let project = eval_constant_scope_project(false);
    let render = project.render();
    for reference in &render.map.constant_refs {
        let expected = match reference.caller.name.as_str() {
            "ordinary" | "defined" => "VALUE",
            "evaluated" | "reflected" => "LexicalScope::Target::VALUE",
            unexpected => panic!("Unexpected test method {unexpected}"),
        };
        assert_eq!(reference.text, expected);
    }
    let runner = SimulationRunner::start(project).await;
    runner.check_initial().await;
    runner.check_fresh_equivalence().await;
}

#[tokio::test]
async fn generated_eval_relative_constants_do_not_inherit_target_scope() {
    let project = eval_constant_scope_project(true);
    let render = project.render();
    let oracle = OracleState::all_files(&project, &render.map);
    let mut editor = FakeEditor::new().await;
    for (file, content) in &render.files {
        editor.open(file, content).await;
    }
    for reference in &render.map.constant_refs {
        let expected = match reference.caller.name.as_str() {
            "ordinary" | "defined" => Some("LexicalScope::Target::VALUE".to_string()),
            "evaluated" | "reflected" => None,
            unexpected => panic!("Unexpected test method {unexpected}"),
        };
        assert_eq!(oracle.resolve_constant_ref(reference), expected);
        let locations = editor
            .goto_def_at(
                &reference.pos.file,
                reference.pos.line,
                reference.pos.character,
            )
            .await;
        if expected.is_some() {
            assert!(locations.iter().any(|location| location_matches_pos(
                location,
                &render.map.constants["LexicalScope::Target::VALUE"]
            )));
        } else {
            assert!(
                locations.is_empty(),
                "top-level eval must not introduce a lexical class scope: {locations:?}"
            );
        }
    }
}

#[tokio::test]
async fn generated_project_tracks_navigation_support_gaps() {
    let runner = SimulationRunner::start(phase1_project()).await;
    let gaps = runner.known_gap_reasons();
    let expected = [UNPROVEN_BLOCK_RECEIVER_GAP].into_iter().collect();

    assert_eq!(gaps, expected);
}

#[tokio::test]
async fn generated_project_checks_definitions_for_agent_navigation() {
    let runner = SimulationRunner::start(phase1_project()).await;
    runner.check_definitions().await;
}

#[tokio::test]
async fn generated_project_checks_references_for_agent_navigation() {
    let runner = SimulationRunner::start(phase1_project()).await;
    runner.check_references().await;
}

#[tokio::test]
async fn generated_project_checks_hover_for_agent_navigation() {
    let runner = SimulationRunner::start(phase1_project()).await;
    runner.check_hover().await;
}

#[test]
fn generated_project_emits_singleton_class_block_methods() {
    let render = singleton_class_block_project().render();
    let source = render.files.get("sim_singleton/gateway.rb").unwrap_or_else(|| {
        panic!(
            "INVARIANT VIOLATED: singleton class block project did not render gateway file. This is a bug because project file paths must be stable from FQNs. Fix: inspect file_for_namespace."
        )
    });

    assert!(
        source.contains("class << self\n      # @return [SimSingleton::Gateway]\n      def build"),
        "expected generated source to use class << self block, got:\n{}",
        source
    );
}

#[tokio::test]
async fn generated_project_resolves_singleton_class_block_class_method() {
    let runner = SimulationRunner::start(singleton_class_block_project()).await;

    runner
        .assert_call_resolves_to("SimSingleton::Gateway.build")
        .await;
    runner.check_definitions().await;
    runner.check_references().await;
    runner.check_hover().await;
}

#[tokio::test]
async fn generated_project_resolves_class_eval_block_method() {
    let runner = SimulationRunner::start(class_eval_project()).await;

    runner.check_definitions().await;
    runner.check_references().await;
    runner.check_hover().await;
}

#[tokio::test]
async fn generated_project_resolves_define_method_method() {
    let runner = SimulationRunner::start(define_method_project()).await;

    runner.check_definitions().await;
    runner.check_references().await;
    runner.check_hover().await;
}

#[tokio::test]
async fn generated_project_resolves_const_get_define_method_method() {
    let runner = SimulationRunner::start(const_get_define_method_project()).await;

    runner.check_definitions().await;
    runner.check_references().await;
    runner.check_hover().await;
}

#[tokio::test]
async fn generated_project_resolves_const_get_constant_ref() {
    let runner = SimulationRunner::start(const_get_constant_ref_project()).await;

    runner.check_definitions().await;
    runner.check_references().await;
    runner.check_hover().await;
}

#[tokio::test]
async fn generated_project_resolves_const_defined_constant_ref() {
    let runner = SimulationRunner::start(const_defined_constant_ref_project()).await;

    runner.check_definitions().await;
    runner.check_references().await;
    runner.check_hover().await;
}

#[tokio::test]
async fn generated_project_resolves_static_send_method() {
    let runner = SimulationRunner::start(static_send_project()).await;

    runner.check_definitions().await;
    runner.check_references().await;
    runner.check_hover().await;
}

#[tokio::test]
async fn generated_project_filters_private_explicit_receiver_calls() {
    let runner = SimulationRunner::start(visibility_project()).await;

    runner.check_definitions().await;
    runner.check_references().await;
    runner.check_hover().await;
}

#[tokio::test]
async fn generated_project_resolves_bare_module_function_method() {
    let runner = SimulationRunner::start(module_function_mode_project()).await;

    runner.check_definitions().await;
    runner.check_references().await;
    runner.check_hover().await;
}

#[tokio::test]
async fn generated_project_resolves_extend_self_module_method() {
    let runner = SimulationRunner::start(extend_self_project()).await;

    runner.check_definitions().await;
    runner.check_references().await;
    runner.check_hover().await;
    runner.check_types().await;
}

#[tokio::test]
async fn generated_project_resolves_extend_as_class_method() {
    let runner = SimulationRunner::start(extend_class_method_project()).await;

    runner
        .assert_call_resolves_to("SimExtend::ClassMethods#configure")
        .await;
    runner
        .assert_call_resolves_to("SimExtend::ClassMethods#publish")
        .await;
    runner.check_definitions().await;
    runner.check_references().await;
    runner.check_hover().await;
}

#[tokio::test]
async fn generated_project_resolves_singleton_class_mixins_as_class_methods() {
    let runner = SimulationRunner::start(singleton_class_mixin_project()).await;

    runner
        .assert_call_resolves_to("SimSingletonMixin::Included#configure")
        .await;
    runner
        .assert_call_resolves_to("SimSingletonMixin::Prepended#audit")
        .await;
    runner.check_definitions().await;
    runner.check_references().await;
    runner.check_hover().await;
}

#[tokio::test]
async fn generated_project_resolves_included_hook_class_methods() {
    let runner = SimulationRunner::start(included_hook_project()).await;

    runner
        .assert_call_resolves_to("SimIncludedHook::FeatureFlags::ClassMethods#enabled?")
        .await;
    runner
        .assert_call_resolves_to("SimIncludedHook::DailyTrends::SharedMethods#get_html")
        .await;
    runner
        .assert_call_resolves_to("SimIncludedHook::AdminHelper::RequestHelpers#api_get")
        .await;
    runner.check_definitions().await;
    runner.check_references().await;
    runner.check_hover().await;
}

#[tokio::test]
async fn generated_project_resolves_concern_class_methods() {
    let runner = SimulationRunner::start(concern_project()).await;

    runner
        .assert_call_resolves_to("SimConcern::Searchable::ClassMethods#find_by_term")
        .await;
    runner.check_definitions().await;
    runner.check_references().await;
    runner.check_hover().await;
}

#[tokio::test]
async fn generated_project_resolves_super_call() {
    let runner = SimulationRunner::start(super_call_project()).await;

    runner
        .assert_call_resolves_to("SimSuperCall::Parent#process")
        .await;
    runner.check_definitions().await;
    runner.check_references().await;
    runner.check_hover().await;
}

#[tokio::test]
async fn generated_project_resolves_alias_method() {
    let runner = SimulationRunner::start(alias_project()).await;

    runner
        .assert_call_resolves_to("SimAlias::User#full_name")
        .await;
    runner
        .assert_call_resolves_to("SimAlias::User#display_name")
        .await;
    runner.check_definitions().await;
    runner.check_references().await;
    runner.check_hover().await;
}

#[tokio::test]
async fn generated_project_resolves_delegate_method() {
    let runner = SimulationRunner::start(delegate_project()).await;

    runner
        .assert_call_resolves_to("SimDelegate::Order#name")
        .await;
    runner.check_definitions().await;
    runner.check_references().await;
    runner.check_hover().await;
}

#[tokio::test]
async fn generated_project_resolves_forwardable_delegate_method() {
    let runner = SimulationRunner::start(forwardable_project()).await;

    runner
        .assert_call_resolves_to("SimForwardable::ServiceFlags.allow?")
        .await;
    runner.check_definitions().await;
    runner.check_references().await;
    runner.check_hover().await;
}

#[tokio::test]
async fn generated_project_resolves_class_attribute_methods() {
    let runner = SimulationRunner::start(class_attribute_project()).await;

    runner
        .assert_call_resolves_to("SimClassAttribute::Worker.queue_config")
        .await;
    runner.check_definitions().await;
    runner.check_references().await;
}

#[tokio::test]
async fn generated_project_uses_method_missing_fallback_without_goto_definition() {
    let runner = SimulationRunner::start(method_missing_project()).await;

    runner.check_definitions().await;
    runner.check_references().await;
    runner.check_hover().await;
}

#[tokio::test]
async fn generated_project_resolves_framework_route_block_helper_call() {
    let runner = SimulationRunner::start(framework_route_block_project()).await;

    runner.check_definitions().await;
    runner.check_references().await;
    runner.check_hover().await;
}

#[tokio::test]
async fn generated_project_resolves_method_object_symbols() {
    let runner = SimulationRunner::start(method_object_project()).await;

    runner
        .assert_call_resolves_to("SimMethodObject::FeatureSettings.get")
        .await;
    runner
        .assert_call_resolves_to("SimMethodObject::FeatureSettings#copy_data")
        .await;
    runner
        .assert_call_resolves_to("SimMethodObject::SinatraBase#health_checks")
        .await;
    runner.check_definitions().await;
    runner.check_references().await;
    runner.check_hover().await;
}

#[tokio::test]
async fn generated_project_resolves_partial_namespace_open_order() {
    let mut runner = SimulationRunner::start_with_open_files(
        mro_project(),
        &["sim_mro/child.rb", "sim_mro/caller.rb"],
    )
    .await;

    runner
        .assert_call_does_not_resolve_to("SimMro::Second#resolve_token")
        .await;

    for file in ["sim_mro/base.rb", "sim_mro/second.rb", "sim_mro/first.rb"] {
        runner.open_file(file).await;
    }

    runner
        .assert_call_resolves_to("SimMro::Second#resolve_token")
        .await;
    runner.check_definitions().await;
}

#[tokio::test]
async fn generated_project_oracles_method_resolution_order() {
    let runner = SimulationRunner::start(mro_project()).await;

    runner
        .assert_call_resolves_to("SimMro::Second#resolve_token")
        .await;
    runner.check_definitions().await;
    runner.check_references().await;
}

#[tokio::test]
async fn generated_project_rejects_stale_include_method_target() {
    let mut runner = SimulationRunner::start(mro_project()).await;
    runner
        .assert_call_resolves_to("SimMro::Second#resolve_token")
        .await;

    let step = EditStep::new("remove second include", |edit| {
        edit.remove_include("SimMro::Child", "SimMro::Second")
            .expect_no_method_definition_target(
                "SimMro::Second#resolve_token",
                "SimMro::Second#resolve_token",
            );
    });
    runner.apply_step(&step).await;
}

#[tokio::test]
async fn generated_project_rejects_stale_prepend_method_target() {
    let mut runner = SimulationRunner::start(mro_prepend_project()).await;
    runner
        .assert_call_resolves_to("SimMroPrepend::Second#resolve_token")
        .await;

    let step = EditStep::new("remove second prepend", |edit| {
        edit.remove_prepend("SimMroPrepend::Child", "SimMroPrepend::Second")
            .expect_no_method_definition_target(
                "SimMroPrepend::Second#resolve_token",
                "SimMroPrepend::Second#resolve_token",
            );
    });
    runner.apply_step(&step).await;
}

#[tokio::test]
async fn generated_project_rejects_stale_superclass_method_target() {
    let mut runner = SimulationRunner::start(superclass_switch_project()).await;
    runner
        .assert_call_resolves_to("SimSuper::OldBase#resolve_token")
        .await;

    let step = EditStep::new("switch superclass", |edit| {
        edit.change_superclass("SimSuper::Child", "SimSuper::NewBase")
            .expect_no_method_definition_target(
                "SimSuper::OldBase#resolve_token",
                "SimSuper::OldBase#resolve_token",
            );
    });
    runner.apply_step(&step).await;
}
