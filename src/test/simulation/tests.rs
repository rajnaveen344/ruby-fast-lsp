use super::ruby_gen::{SourcePos, UNPROVEN_BLOCK_RECEIVER_GAP};
use super::{
    seeded_script, simulation_seeds_from_env, write_seed_artifact, CallShape, ConstantRefShape,
    EditOp, EditStep, EngineSimulationRunner, MethodDefForm, MethodKind, MethodTarget,
    MethodVisibility, MethodVisibilitySyntax, NamespaceKind, OracleState, SimulationRunner,
    SyntheticProject,
};
use crate::test::harness::FakeEditor;
use std::collections::{BTreeMap, BTreeSet};

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

pub(super) fn phase1_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_phase1");

    project
        .module("Audit::Trackable", |module| {
            module.constant("LEVEL", "\"info\"");
            module.method("audit").ref_const("Audit::Trackable::LEVEL");
            module.method("record_event");
            module.method("tagged");
        })
        .class("Payments::Gateway", |class| {
            class.constant("DEFAULT_PROVIDER", "\"stripe\"");
            class.method("capture").returns("String");
            class.method("refund").returns("String");
            class.method("void");
            class.class_method("default").returns("Payments::Gateway");
            class
                .class_method("provider")
                .returns("String")
                .ref_const("Payments::Gateway::DEFAULT_PROVIDER");
        })
        .class("Payments::FallbackGateway", |class| {
            class.superclass("Payments::Gateway");
            class.include("Audit::Trackable");
            class.method("capture").returns("String");
            class.method("queue");
            class.method("normalize");
        })
        .class("Billing::BaseInvoice", |class| {
            class.constant("BASE_STATUS", "\"ready\"");
            class.method("normalize");
            class
                .method("base_status")
                .ref_const("Billing::BaseInvoice::BASE_STATUS");
        })
        .class("Billing::Account", |class| {
            class.method("gateway").returns("Payments::Gateway");
            class
                .method("backup_gateway")
                .returns("Payments::FallbackGateway");
        })
        .class("Billing::Invoice", |class| {
            class.superclass("Billing::BaseInvoice");
            class.include("Audit::Trackable");
            class.constant("DEFAULT_CURRENCY", "\"USD\"");
            class
                .method("gateway_for_delegate")
                .returns("Payments::Gateway");
            class.delegate_instance_method("delegated_capture", "gateway_for_delegate");
            class
                .method("charge")
                .returns("String")
                .ref_const("Billing::Invoice::DEFAULT_CURRENCY")
                .calls("Payments::Gateway#capture", CallShape::local("gateway"))
                .calls(
                    "Payments::Gateway#capture",
                    CallShape::array_block_param("gateway_from_block"),
                )
                .calls(
                    "Payments::Gateway#capture",
                    CallShape::yield_block_param("gateway_from_yield"),
                )
                .calls("Payments::Gateway#refund", CallShape::ivar("gateway"))
                .calls("Payments::Gateway.default", CallShape::ClassSend)
                .calls("Audit::Trackable#audit", CallShape::Bare)
                .calls("Billing::BaseInvoice#normalize", CallShape::Bare);
            class
                .method("block_scoped_audit")
                .returns("String")
                .calls("Audit::Trackable#audit", CallShape::BareInDoBlock)
                .calls("Audit::Trackable#record_event", CallShape::BareInBraceBlock)
                .calls("Audit::Trackable#tagged", CallShape::BareInLambda)
                .calls("Billing::BaseInvoice#normalize", CallShape::BareInProc);
            class.method("chain_charge").calls(
                "Payments::Gateway#capture",
                CallShape::one_hop("account", "Billing::Account", "gateway"),
            );
            class
                .method("constructor_charge")
                .calls("Payments::Gateway#capture", CallShape::ConstructorSend);
        })
        .class("Catalog::Sku", |class| {
            class.include("Audit::Trackable");
            class.constant("PREFIX", "\"sku\"");
            class
                .method("format")
                .returns("String")
                .ref_const("Catalog::Sku::PREFIX");
            class
                .method("audit_sku")
                .calls("Audit::Trackable#audit", CallShape::Bare);
        })
        .class("Catalog::Item", |class| {
            class.method("sku").returns("Catalog::Sku");
            class.method("publish").returns("String").calls(
                "Catalog::Sku#format",
                CallShape::one_hop("item", "Catalog::Item", "sku"),
            );
        })
        .class("Reporting::Summary", |class| {
            class
                .method("render")
                .returns("String")
                .calls("Billing::Invoice#charge", CallShape::ConstructorSend)
                .calls(
                    "Billing::Invoice#delegated_capture",
                    CallShape::ConstructorSend,
                )
                .calls("Catalog::Item#publish", CallShape::ConstructorSend);
            class
                .method("capture_total")
                .calls("Payments::Gateway#capture", CallShape::local("gateway"));
        })
        .filler_classes(44)
        .edit("delete gateway capture", |edit| {
            edit.delete_method("Payments::Gateway#capture")
                .expect_unresolved_method("billing/invoice.rb", "capture")
                .expect_no_method_definition_target(
                    "Payments::Gateway#capture",
                    "Payments::Gateway#capture",
                );
        })
        .edit("restore gateway capture", |edit| {
            edit.restore_method("Payments::Gateway#capture")
                .expect_no_unresolved_method("billing/invoice.rb", "capture");
        })
        .edit("delete invoice currency", |edit| {
            edit.delete_constant("Billing::Invoice::DEFAULT_CURRENCY")
                .expect_unresolved_constant("billing/invoice.rb", "DEFAULT_CURRENCY")
                .expect_no_constant_definition_target(
                    "Billing::Invoice::DEFAULT_CURRENCY",
                    "Billing::Invoice::DEFAULT_CURRENCY",
                );
        })
        .edit("restore invoice currency", |edit| {
            edit.restore_constant("Billing::Invoice::DEFAULT_CURRENCY")
                .expect_no_unresolved_constant("billing/invoice.rb", "DEFAULT_CURRENCY");
        })
        .edit("change inheritance and mixin", |edit| {
            edit.remove_include("Billing::Invoice", "Audit::Trackable")
                .change_superclass("Billing::Invoice", "Payments::FallbackGateway");
        });

    project
}

fn mro_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_mro");

    project
        .module("SimMro::First", |module| {
            module.method("resolve_token").returns("String");
        })
        .module("SimMro::Second", |module| {
            module.method("resolve_token").returns("String");
        })
        .class("SimMro::Base", |class| {
            class.method("resolve_token").returns("String");
        })
        .class("SimMro::Child", |class| {
            class.superclass("SimMro::Base");
            class.include("SimMro::First");
            class.include("SimMro::Second");
        })
        .class("SimMro::Caller", |class| {
            class.method("run").calls(
                "SimMro::Second#resolve_token",
                CallShape::receiver_local("child", "SimMro::Child"),
            );
        });

    project
}

fn mro_prepend_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_mro_prepend");

    project
        .module("SimMroPrepend::First", |module| {
            module.method("resolve_token").returns("String");
        })
        .module("SimMroPrepend::Second", |module| {
            module.method("resolve_token").returns("String");
        })
        .class("SimMroPrepend::Base", |class| {
            class.method("resolve_token").returns("String");
        })
        .class("SimMroPrepend::Child", |class| {
            class.superclass("SimMroPrepend::Base");
            class.prepend("SimMroPrepend::First");
            class.prepend("SimMroPrepend::Second");
            class.method("resolve_token").returns("String");
        })
        .class("SimMroPrepend::Caller", |class| {
            class.method("run").calls(
                "SimMroPrepend::Second#resolve_token",
                CallShape::receiver_local("child", "SimMroPrepend::Child"),
            );
        });

    project
}

pub(super) fn superclass_switch_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_superclass_switch");

    project
        .class("SimSuper::OldBase", |class| {
            class.method("resolve_token").returns("String");
        })
        .class("SimSuper::NewBase", |class| {
            class.method("resolve_token").returns("String");
        })
        .class("SimSuper::Child", |class| {
            class.superclass("SimSuper::OldBase");
        })
        .class("SimSuper::Caller", |class| {
            class.method("run").calls(
                "SimSuper::OldBase#resolve_token",
                CallShape::receiver_local("child", "SimSuper::Child"),
            );
        });

    project
}

fn super_call_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_super_call");

    project
        .class("SimSuperCall::Parent", |class| {
            class.method("process").returns("String");
        })
        .class("SimSuperCall::Child", |class| {
            class
                .superclass("SimSuperCall::Parent")
                .method("process")
                .calls("SimSuperCall::Parent#process", CallShape::Super)
                .returns("String");
        });

    project
}

fn const_namespace_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_const_namespace");

    project
        .module("SimConst::Shared", |module| {
            module.constant("TOKEN", "\"shared\"");
        })
        .module("SimConst::Outer", |module| {
            module.constant("TOKEN", "\"outer\"");
            module.constant("PARENT_TOKEN", "\"parent\"");
        })
        .module("SimConst::Outer::Inner", |module| {
            module.constant("TOKEN", "\"inner\"");
        })
        .class("SimConst::Outer::Inner::Reader", |class| {
            class
                .method("read")
                .ref_const_relative("SimConst::Outer::Inner::TOKEN", "TOKEN")
                .ref_const_relative("SimConst::Outer::PARENT_TOKEN", "PARENT_TOKEN")
                .ref_const_absolute("SimConst::Outer::TOKEN")
                .ref_const_qualified("SimConst::Shared::TOKEN", "Shared::TOKEN");
        });

    project
}

fn reopened_namespace_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_reopened_namespace");

    project
        .class("SimReopen::Box", |class| {
            class.file_path("sim_reopen/box_core.rb");
            class.constant("TOKEN", "\"core\"");
            class.method("core").returns("String");
        })
        .class("SimReopen::Box", |class| {
            class.file_path("sim_reopen/box_extension.rb");
            class
                .method("extension")
                .returns("String")
                .calls("SimReopen::Box#core", CallShape::Bare)
                .ref_const_relative("SimReopen::Box::TOKEN", "TOKEN");
        })
        .class("SimReopen::Caller", |class| {
            class
                .method("run")
                .calls("SimReopen::Box#extension", CallShape::local("box"));
        });

    project
}

fn block_type_flow_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_block_type_flow");

    project.class("SimBlockType::Builder", |class| {
        class
            .method("build")
            .returns("String")
            .with_block_type_asserts();
    });

    project
}

fn singleton_class_block_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_singleton_class_block");

    project
        .class("SimSingleton::Gateway", |class| {
            class
                .class_method("build")
                .returns("SimSingleton::Gateway")
                .in_singleton_class_block();
            class.method("capture").returns("String");
        })
        .class("SimSingleton::Caller", |class| {
            class
                .method("run")
                .returns("SimSingleton::Gateway")
                .calls("SimSingleton::Gateway.build", CallShape::ClassSend);
        });

    project
}

fn class_eval_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_class_eval");

    project
        .class("SimClassEval::Target", |class| {
            class
                .method("patched")
                .returns("String")
                .in_class_eval_block();
        })
        .class("SimClassEval::Caller", |class| {
            class
                .method("run")
                .calls("SimClassEval::Target#patched", CallShape::ConstructorSend);
        });

    project
}

fn eval_constant_scope_project(relative: bool) -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_eval_constant_scope");
    project.class("LexicalScope::Target", |class| {
        class.constant("VALUE", "\"token\"");
        for name in ["ordinary", "defined", "evaluated", "reflected"] {
            let method = class.method(name);
            if relative {
                method.ref_const_relative("LexicalScope::Target::VALUE", "VALUE");
            } else {
                method.ref_const("LexicalScope::Target::VALUE");
            }
            match name {
                "ordinary" => {}
                "defined" => {
                    method.as_define_method();
                }
                "evaluated" => {
                    method.in_class_eval_block();
                }
                "reflected" => {
                    method.as_const_get_define_method();
                }
                unexpected => panic!("Unexpected test method {unexpected}"),
            }
        }
    });
    project
}

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

fn define_method_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_define_method");

    project
        .class("SimDefineMethod::Target", |class| {
            class.method("helper").returns("String");
            class
                .method("patched")
                .returns("String")
                .calls("SimDefineMethod::Target#helper", CallShape::Bare)
                .as_define_method();
        })
        .class("SimDefineMethod::Caller", |class| {
            class.method("run").calls(
                "SimDefineMethod::Target#patched",
                CallShape::ConstructorSend,
            );
        });

    project
}

fn const_get_define_method_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_const_get_define_method");

    project
        .class("SimConstGetDefineMethod::SMTP", |class| {
            class.method("helper").returns("String");
            class
                .method("tls?")
                .returns("String")
                .calls("SimConstGetDefineMethod::SMTP#helper", CallShape::Bare)
                .as_const_get_define_method();
        })
        .class("SimConstGetDefineMethod::Caller", |class| {
            class.method("run").calls(
                "SimConstGetDefineMethod::SMTP#tls?",
                CallShape::ConstructorSend,
            );
        });

    project
}

fn const_get_constant_ref_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_const_get_constant_ref");

    project
        .class("SimConstGetRef::TriggerHelpers", |class| {
            class.constant("TOKEN", "\"trigger\"");
        })
        .class("SimConstGetRef::Caller", |class| {
            class
                .method("run")
                .ref_const_const_get("SimConstGetRef::TriggerHelpers");
        });

    project
}

fn const_defined_constant_ref_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_const_defined_constant_ref");

    project
        .class("SimConstDefinedRef::PushUnit", |class| {
            class.constant("TYPE", "\"push\"");
        })
        .class("SimConstDefinedRef::Caller", |class| {
            class
                .method("run")
                .ref_const_const_defined("SimConstDefinedRef::PushUnit::TYPE")
                .ref_const_const_get("SimConstDefinedRef::PushUnit::TYPE");
        });

    project
}

fn static_send_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_static_send");

    project
        .class("SimStaticSend::Target", |class| {
            class.method("patched").returns("String");
        })
        .class("SimStaticSend::Caller", |class| {
            class
                .method("run")
                .calls("SimStaticSend::Target#patched", CallShape::StaticSend);
        });

    project
}

fn visibility_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_visibility");

    project
        .class("SimVisibility::Vault", |class| {
            class
                .method("secret")
                .returns("String")
                .private()
                .visibility_argument_list();
            class
                .method("semi_secret")
                .returns("String")
                .protected()
                .visibility_argument_list();
            class
                .method("probe")
                .returns("String")
                .calls("SimVisibility::Vault#secret", CallShape::Bare)
                .calls("SimVisibility::Vault#secret", CallShape::ConstructorSend)
                .calls("SimVisibility::Vault#secret", CallShape::StaticSend);
            class.method("protected_probe").returns("String").calls(
                "SimVisibility::Vault#semi_secret",
                CallShape::receiver_local("other", "SimVisibility::Vault"),
            );
        })
        .class("SimVisibility::Caller", |class| {
            class
                .method("run")
                .calls("SimVisibility::Vault#secret", CallShape::ConstructorSend);
        })
        .module("SimVisibility::HiddenMixin", |module| {
            module.method("hidden").returns("String");
        })
        .class("SimVisibility::HiddenUser", |class| {
            class.include("SimVisibility::HiddenMixin");
            class.private_visibility("hidden");
            class
                .method("inside")
                .calls("SimVisibility::HiddenMixin#hidden", CallShape::Bare);
        })
        .class("SimVisibility::HiddenCaller", |class| {
            class.method("run").calls(
                "SimVisibility::HiddenMixin#hidden",
                CallShape::receiver_local("other", "SimVisibility::HiddenUser"),
            );
        })
        .module("SimVisibility::PublicMixin", |module| {
            module.method("visible_again").returns("String").private();
        })
        .class("SimVisibility::PublicUser", |class| {
            class.include("SimVisibility::PublicMixin");
            class.public_visibility("visible_again");
        })
        .class("SimVisibility::PublicCaller", |class| {
            class.method("run").calls(
                "SimVisibility::PublicMixin#visible_again",
                CallShape::receiver_local("other", "SimVisibility::PublicUser"),
            );
        })
        .module("SimVisibility::ProtectedMixin", |module| {
            module.method("guarded").returns("String");
        })
        .class("SimVisibility::ProtectedUser", |class| {
            class.include("SimVisibility::ProtectedMixin");
            class.protected_visibility("guarded");
        })
        .class("SimVisibility::ProtectedChild", |class| {
            class.superclass("SimVisibility::ProtectedUser");
            class.method("run").calls(
                "SimVisibility::ProtectedMixin#guarded",
                CallShape::receiver_local("other", "SimVisibility::ProtectedUser"),
            );
        })
        .class("SimVisibility::ProtectedCaller", |class| {
            class.method("run").calls(
                "SimVisibility::ProtectedMixin#guarded",
                CallShape::receiver_local("other", "SimVisibility::ProtectedUser"),
            );
        });

    project
}

fn module_function_mode_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_module_function_mode");

    project
        .module("SimModuleFunction::Utils", |module| {
            module
                .method("helper")
                .returns("String")
                .in_module_function_mode();
        })
        .class("SimModuleFunction::Caller", |class| {
            class
                .method("run")
                .calls("SimModuleFunction::Utils#helper", CallShape::ClassSend);
        });

    project
}

fn extend_self_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_extend_self");

    project
        .module("SimExtendSelf::Utils", |module| {
            module.extend_self();
            module.method("helper").returns("String");
        })
        .class("SimExtendSelf::Caller", |class| {
            class
                .method("run")
                .returns("String")
                .calls("SimExtendSelf::Utils#helper", CallShape::ClassSend);
        });

    project
}

fn extend_class_method_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_extend_class_method");

    project
        .module("SimExtend::ClassMethods", |module| {
            module.method("configure").returns("String");
            module.method("publish").returns("String");
        })
        .class("SimExtend::Base", |class| {
            class.extend("SimExtend::ClassMethods");
        })
        .class("SimExtend::Child", |class| {
            class.superclass("SimExtend::Base");
        })
        .class("SimExtend::Caller", |class| {
            class
                .method("run")
                .returns("String")
                .calls(
                    "SimExtend::ClassMethods#configure",
                    CallShape::class_receiver("SimExtend::Child"),
                )
                .calls(
                    "SimExtend::ClassMethods#publish",
                    CallShape::class_receiver("SimExtend::Base"),
                );
        });

    project
}

fn singleton_class_mixin_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_singleton_class_mixin");

    project
        .module("SimSingletonMixin::Included", |module| {
            module.method("configure").returns("String");
        })
        .module("SimSingletonMixin::Prepended", |module| {
            module.method("audit").returns("String");
        })
        .class("SimSingletonMixin::Gateway", |class| {
            class
                .singleton_include("SimSingletonMixin::Included")
                .singleton_prepend("SimSingletonMixin::Prepended");
        })
        .class("SimSingletonMixin::Caller", |class| {
            class
                .method("run")
                .returns("String")
                .calls(
                    "SimSingletonMixin::Included#configure",
                    CallShape::class_receiver("SimSingletonMixin::Gateway"),
                )
                .calls(
                    "SimSingletonMixin::Prepended#audit",
                    CallShape::class_receiver("SimSingletonMixin::Gateway"),
                );
        });

    project
}

fn included_hook_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_included_hook");

    project
        .module("SimIncludedHook::FeatureFlags::ClassMethods", |module| {
            module.method("enabled?").returns("String");
        })
        .module("SimIncludedHook::DailyTrends::SharedMethods", |module| {
            module.method("get_html").returns("String");
        })
        .module("SimIncludedHook::AdminHelper::RequestHelpers", |module| {
            module.method("api_get").returns("String");
        })
        .module("SimIncludedHook::FeatureFlags", |module| {
            module.included_hook_extend("SimIncludedHook::FeatureFlags::ClassMethods");
        })
        .module("SimIncludedHook::DailyTrends", |module| {
            module.included_hook_include("SimIncludedHook::DailyTrends::SharedMethods");
        })
        .module("SimIncludedHook::AdminHelper", |module| {
            module.included_hook_class_eval_include("SimIncludedHook::AdminHelper::RequestHelpers");
        })
        .class("SimIncludedHook::Worker", |class| {
            class.include("SimIncludedHook::FeatureFlags");
        })
        .class("SimIncludedHook::TrendWorker", |class| {
            class.include("SimIncludedHook::DailyTrends");
        })
        .class("SimIncludedHook::SpecContext", |class| {
            class.include("SimIncludedHook::AdminHelper");
        })
        .class("SimIncludedHook::Caller", |class| {
            class
                .method("run")
                .returns("String")
                .calls(
                    "SimIncludedHook::FeatureFlags::ClassMethods#enabled?",
                    CallShape::class_receiver("SimIncludedHook::Worker"),
                )
                .calls(
                    "SimIncludedHook::DailyTrends::SharedMethods#get_html",
                    CallShape::receiver_local("trend_worker", "SimIncludedHook::TrendWorker"),
                )
                .calls(
                    "SimIncludedHook::AdminHelper::RequestHelpers#api_get",
                    CallShape::receiver_local("spec_context", "SimIncludedHook::SpecContext"),
                );
        });

    project
}

fn concern_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_concern");

    project
        .module("SimConcern::Searchable::ClassMethods", |module| {
            module.method("find_by_term").returns("String");
        })
        .module("SimConcern::Searchable", |module| {
            module.concern_class_methods("SimConcern::Searchable::ClassMethods");
        })
        .class("SimConcern::Product", |class| {
            class.include("SimConcern::Searchable");
        })
        .class("SimConcern::Caller", |class| {
            class.method("run").returns("String").calls(
                "SimConcern::Searchable::ClassMethods#find_by_term",
                CallShape::class_receiver("SimConcern::Product"),
            );
        });

    project
}

fn alias_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_alias");

    project
        .class("SimAlias::User", |class| {
            class.method("name").returns("String");
            class.alias_instance_method("full_name", "name");
            class.alias_method_instance_method("display_name", "name");
        })
        .class("SimAlias::Caller", |class| {
            class
                .method("run")
                .returns("String")
                .calls("SimAlias::User#full_name", CallShape::ConstructorSend)
                .calls("SimAlias::User#display_name", CallShape::ConstructorSend);
        });

    project
}

fn delegate_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_delegate");

    project
        .class("SimDelegate::User", |class| {
            class.method("name").returns("String");
        })
        .class("SimDelegate::Order", |class| {
            class.method("user").returns("SimDelegate::User");
            class.delegate_instance_method("name", "user");
        })
        .class("SimDelegate::Caller", |class| {
            class
                .method("run")
                .returns("String")
                .calls("SimDelegate::Order#name", CallShape::ConstructorSend);
        });

    project
}

fn forwardable_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_forwardable");

    project
        .class("SimForwardable::Flags", |class| {
            class.method("allow?").returns("String");
        })
        .class("SimForwardable::ServiceFlags", |class| {
            class
                .class_method("instance")
                .returns("SimForwardable::Flags");
            class.forwardable_class_method("allow?", "instance");
        })
        .class("SimForwardable::Caller", |class| {
            class
                .method("run")
                .returns("String")
                .calls("SimForwardable::ServiceFlags.allow?", CallShape::ClassSend);
        });

    project
}

fn class_attribute_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_class_attribute");

    project
        .class("SimClassAttribute::Worker", |class| {
            class.class_attribute("queue_config");
        })
        .class("SimClassAttribute::Caller", |class| {
            class.method("run").calls(
                "SimClassAttribute::Worker.queue_config",
                CallShape::ClassSend,
            );
        });

    project
}

fn method_missing_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_method_missing");

    project
        .class("SimDynamic::Record", |class| {
            class.method("method_missing").returns("String");
        })
        .class("SimDynamic::Caller", |class| {
            class.method("run").calls(
                "SimDynamic::Record#virtual_total",
                CallShape::ConstructorSend,
            );
        });

    project
}

fn framework_route_block_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_framework_route_block");

    project
        .module("SimFramework::Commerce", |module| {
            module.method("fetch_credits").returns("Array");
        })
        .module("SimFramework::API", |module| {
            module.include("SimFramework::Commerce");
        })
        .class("SimFramework::BaseApp", |class| {
            class.include("SimFramework::API");
        })
        .class("SimFramework::AdminApp", |class| {
            class
                .superclass("SimFramework::BaseApp")
                .route_call("SimFramework::Commerce#fetch_credits");
        });

    project
}

fn method_object_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_method_object");

    project
        .class("SimMethodObject::FeatureSettings", |class| {
            class.class_method("get").returns("String");
            class.method("copy_data").returns("String");
        })
        .class("SimMethodObject::SinatraBase", |class| {
            class.method("health_checks").returns("String");
            class
                .method("run")
                .returns("String")
                .calls(
                    "SimMethodObject::FeatureSettings.get",
                    CallShape::MethodObject,
                )
                .calls(
                    "SimMethodObject::FeatureSettings#copy_data",
                    CallShape::InstanceMethodObject,
                )
                .calls(
                    "SimMethodObject::SinatraBase#health_checks",
                    CallShape::MethodObject,
                );
        });

    project
}

#[derive(Debug, Default)]
struct SimulationCoverage {
    buckets: BTreeMap<String, usize>,
}

impl SimulationCoverage {
    fn from_projects(projects: &[SyntheticProject]) -> Self {
        let mut coverage = Self::default();
        for project in projects {
            coverage.add_project(project);
        }
        coverage
    }

    fn add_project(&mut self, project: &SyntheticProject) {
        self.add_dispatch_interactions(project);
        for namespace in &project.namespaces {
            match namespace.kind {
                NamespaceKind::Class => self.add("namespace:class"),
                NamespaceKind::Module => self.add("namespace:module"),
            }

            if namespace.superclass.is_some() {
                self.add("namespace:superclass");
            }
            self.add_enabled(
                "mixin:include",
                namespace.includes.iter().any(|item| item.enabled),
            );
            self.add_enabled(
                "mixin:prepend",
                namespace.prepends.iter().any(|item| item.enabled),
            );
            self.add_enabled(
                "mixin:extend",
                namespace.extends.iter().any(|item| item.enabled),
            );
            self.add_enabled("mixin:extend-self", namespace.extend_self);
            self.add_enabled(
                "mixin:singleton-include",
                namespace.singleton_includes.iter().any(|item| item.enabled),
            );
            self.add_enabled(
                "mixin:singleton-prepend",
                namespace.singleton_prepends.iter().any(|item| item.enabled),
            );
            self.add_enabled(
                "mixin:included-hook-extend",
                namespace
                    .included_hook_extends
                    .iter()
                    .any(|item| item.enabled),
            );
            self.add_enabled(
                "mixin:included-hook-include",
                namespace
                    .included_hook_includes
                    .iter()
                    .any(|item| item.enabled),
            );
            self.add_enabled(
                "mixin:included-hook-class-eval-include",
                namespace
                    .included_hook_class_eval_includes
                    .iter()
                    .any(|item| item.enabled),
            );
            self.add_enabled(
                "mixin:concern-class-methods",
                namespace
                    .concern_class_methods
                    .iter()
                    .any(|item| item.enabled),
            );

            for route_call in &namespace.route_calls {
                self.add_call_shape(&route_call.shape);
            }
            for method in &namespace.methods {
                if !method.enabled {
                    continue;
                }
                self.add_method_kind(method.kind);
                self.add_def_form(method.def_form);
                self.add_visibility(method.visibility);
                self.add_visibility_syntax(method.visibility_syntax);

                for call in &method.calls {
                    self.add_call_shape(&call.shape);
                }
                for constant_ref in &method.constant_refs {
                    self.add_constant_ref_shape(&constant_ref.shape);
                }
            }

            self.add_enabled(
                "macro:alias",
                namespace.aliases.iter().any(|item| item.enabled),
            );
            self.add_enabled(
                "macro:delegate",
                namespace.delegates.iter().any(|item| item.enabled),
            );
            self.add_enabled(
                "macro:class-attribute",
                namespace.class_attributes.iter().any(|item| item.enabled),
            );
            self.add_enabled(
                "visibility:override",
                namespace
                    .visibility_overrides
                    .iter()
                    .any(|item| item.enabled),
            );
        }

        for step in &project.edits {
            for op in &step.ops {
                self.add_edit_op(op);
            }
        }
    }

    fn add_dispatch_interactions(&mut self, project: &SyntheticProject) {
        let render = project.render();
        let oracle = OracleState::all_files(project, &render.map);
        for call in &render.map.calls {
            if !call.definition_support.is_supported() {
                continue;
            }
            let is_module = project
                .namespaces
                .iter()
                .any(|ns| ns.fqn == call.caller.owner && ns.kind == NamespaceKind::Module);
            if is_module && matches!(call.shape, CallShape::Bare) {
                let receivers = oracle.instance_receivers(&call.caller.owner);
                let targets = oracle.resolve_call_targets(call);
                self.add_enabled("dispatch:multiple-hosts", receivers.len() > 1);
                self.add_enabled("dispatch:distinct-targets", targets.len() > 1);
                self.add_enabled(
                    "dispatch:host-override",
                    targets
                        .iter()
                        .any(|target| receivers.contains(&target.owner)),
                );
                self.add_enabled(
                    "dispatch:host-prepend",
                    targets.iter().any(|target| {
                        project.namespaces.iter().any(|ns| {
                            receivers.contains(&ns.fqn)
                                && ns
                                    .prepends
                                    .iter()
                                    .any(|edge| edge.enabled && edge.fqn == target.owner)
                        })
                    }),
                );
            }
            if matches!(call.shape, CallShape::InstanceMethodObject) {
                self.add_enabled(
                    "dispatch:reflection-with-host-override",
                    oracle.resolve_call_targets(call)
                        != oracle.resolve_instance_dispatch(&call.target.owner, &call.target.name),
                );
            }
        }
        let mut edited = project.clone();
        for step in &project.edits {
            let before = edited.render();
            let before_oracle = OracleState::all_files(&edited, &before.map);
            let expectations = before
                .map
                .calls
                .iter()
                .map(|call| (call.clone(), before_oracle.resolve_call_targets(call)))
                .collect::<Vec<_>>();
            for op in &step.ops {
                edited.apply_op(op);
            }
            let after = edited.render();
            let after_oracle = OracleState::all_files(&edited, &after.map);
            self.add_enabled(
                "dispatch:edit-changes-targets",
                expectations.iter().any(|(call, targets)| {
                    matches!(call.shape, CallShape::Bare)
                        && *targets != after_oracle.resolve_call_targets(call)
                }),
            );
        }
    }

    fn add(&mut self, bucket: &str) {
        *self.buckets.entry(bucket.to_string()).or_default() += 1;
    }

    fn add_enabled(&mut self, bucket: &str, enabled: bool) {
        if enabled {
            self.add(bucket);
        }
    }

    fn add_method_kind(&mut self, kind: MethodKind) {
        match kind {
            MethodKind::Instance => self.add("method-kind:instance"),
            MethodKind::Class => self.add("method-kind:class"),
        }
    }

    fn add_def_form(&mut self, def_form: MethodDefForm) {
        match def_form {
            MethodDefForm::Regular => self.add("def-form:regular"),
            MethodDefForm::SingletonClassBlock => self.add("def-form:singleton-class-block"),
            MethodDefForm::ClassEvalBlock => self.add("def-form:class-eval-block"),
            MethodDefForm::DefineMethod => self.add("def-form:define-method"),
            MethodDefForm::ConstGetDefineMethod => self.add("def-form:const-get-define-method"),
            MethodDefForm::ModuleFunctionMode => self.add("def-form:module-function-mode"),
        }
    }

    fn add_visibility(&mut self, visibility: MethodVisibility) {
        match visibility {
            MethodVisibility::Public => self.add("visibility:public"),
            MethodVisibility::Protected => self.add("visibility:protected"),
            MethodVisibility::Private => self.add("visibility:private"),
        }
    }

    fn add_visibility_syntax(&mut self, syntax: MethodVisibilitySyntax) {
        match syntax {
            MethodVisibilitySyntax::ScopeKeyword => self.add("visibility-syntax:scope-keyword"),
            MethodVisibilitySyntax::ArgumentList => self.add("visibility-syntax:argument-list"),
        }
    }

    fn add_call_shape(&mut self, shape: &CallShape) {
        self.add(&format!("call:{}", shape.label()));
    }

    fn add_constant_ref_shape(&mut self, shape: &ConstantRefShape) {
        match shape {
            ConstantRefShape::Auto => self.add("constant-ref:auto"),
            ConstantRefShape::Absolute => self.add("constant-ref:absolute"),
            ConstantRefShape::ConstGet => self.add("constant-ref:const-get"),
            ConstantRefShape::ConstDefined => self.add("constant-ref:const-defined"),
            ConstantRefShape::RelativeName { .. } => self.add("constant-ref:relative-name"),
            ConstantRefShape::Qualified { .. } => self.add("constant-ref:qualified"),
        }
    }

    fn add_edit_op(&mut self, op: &EditOp) {
        match op {
            EditOp::DeleteMethod(_) => self.add("edit:delete-method"),
            EditOp::RestoreMethod(_) => self.add("edit:restore-method"),
            EditOp::DeleteConstant(_) => self.add("edit:delete-constant"),
            EditOp::RestoreConstant(_) => self.add("edit:restore-constant"),
            EditOp::DeleteNamespace(_) => self.add("edit:delete-namespace"),
            EditOp::RestoreNamespace(_) => self.add("edit:restore-namespace"),
            EditOp::RemoveInclude { .. } => self.add("edit:remove-include"),
            EditOp::AddInclude { .. } => self.add("edit:add-include"),
            EditOp::RemovePrepend { .. } => self.add("edit:remove-prepend"),
            EditOp::AddPrepend { .. } => self.add("edit:add-prepend"),
            EditOp::ChangeSuperclass { .. } => self.add("edit:change-superclass"),
            EditOp::ClearSuperclass { .. } => self.add("edit:clear-superclass"),
        }
    }

    fn require(&self, buckets: &[&str]) {
        let missing = buckets
            .iter()
            .filter(|bucket| !self.buckets.contains_key(**bucket))
            .copied()
            .collect::<Vec<_>>();
        assert!(
            missing.is_empty(),
            "INVARIANT VIOLATED: simulation coverage is missing required buckets: {:?}. This is a bug because simulator completeness depends on deterministic coverage for known Ruby shapes. Fix: add a fixture that exercises the missing shape or remove the bucket if the simulator no longer supports it.\n\nCoverage summary:\n{}",
            missing,
            self.summary()
        );
    }

    fn summary(&self) -> String {
        self.buckets
            .iter()
            .map(|(bucket, count)| format!("{bucket}={count}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn simulation_coverage_projects() -> Vec<SyntheticProject> {
    vec![
        {
            let mut project = SyntheticProject::new("module_dispatch_coverage");
            super::seeded::add_module_dispatch_scenario(&mut project, "DispatchCoverage");
            project
        },
        phase1_project(),
        mro_project(),
        mro_prepend_project(),
        superclass_switch_project(),
        super_call_project(),
        const_namespace_project(),
        reopened_namespace_project(),
        singleton_class_block_project(),
        class_eval_project(),
        define_method_project(),
        const_get_define_method_project(),
        const_get_constant_ref_project(),
        const_defined_constant_ref_project(),
        static_send_project(),
        visibility_project(),
        module_function_mode_project(),
        extend_self_project(),
        extend_class_method_project(),
        singleton_class_mixin_project(),
        included_hook_project(),
        concern_project(),
        alias_project(),
        delegate_project(),
        forwardable_project(),
        class_attribute_project(),
        method_missing_project(),
        framework_route_block_project(),
        method_object_project(),
        simulation_edit_coverage_project(),
    ]
}

fn simulation_edit_coverage_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_edit_coverage");

    project
        .module("SimEdit::Mixin", |module| {
            module.method("included");
        })
        .module("SimEdit::Prepended", |module| {
            module.method("prepended");
        })
        .class("SimEdit::Base", |class| {
            class.method("base");
        })
        .class("SimEdit::Gone", |class| {
            class.method("gone");
        })
        .class("SimEdit::Child", |class| {
            class.superclass("SimEdit::Base");
            class.include("SimEdit::Mixin");
            class.prepend("SimEdit::Prepended");
        })
        .edit("exercise edit op buckets", |edit| {
            edit.add_include("SimEdit::Child", "SimEdit::Mixin")
                .add_prepend("SimEdit::Child", "SimEdit::Prepended")
                .remove_prepend("SimEdit::Child", "SimEdit::Prepended")
                .clear_superclass("SimEdit::Child")
                .delete_namespace("SimEdit::Gone")
                .restore_namespace("SimEdit::Gone");
        });

    project
}

fn resolved_signature(
    oracle: &OracleState<'_>,
    receiver_owner: &str,
    method: &str,
) -> Option<String> {
    oracle
        .resolve_instance_method(receiver_owner, method)
        .map(|target| target.signature())
}

fn resolved_constant(
    oracle: &OracleState<'_>,
    project: &SyntheticProject,
    text: &str,
) -> Option<String> {
    let render = project.render();
    let constant_ref = render
        .map
        .constant_refs
        .iter()
        .find(|constant_ref| constant_ref.text == text)
        .unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: generated constant ref `{}` is missing. This is a bug because test assertions must target generated refs. Fix: update const namespace fixture.",
                text
            )
        });
    oracle.resolve_constant_ref(constant_ref)
}

#[test]
fn generated_project_shape_coverage_tracks_required_buckets() {
    const REQUIRED_BUCKETS: &[&str] = &[
        "dispatch:multiple-hosts",
        "dispatch:distinct-targets",
        "dispatch:host-override",
        "dispatch:host-prepend",
        "dispatch:reflection-with-host-override",
        "dispatch:edit-changes-targets",
        "namespace:class",
        "namespace:module",
        "namespace:superclass",
        "mixin:include",
        "mixin:prepend",
        "mixin:extend",
        "mixin:extend-self",
        "mixin:singleton-include",
        "mixin:singleton-prepend",
        "mixin:included-hook-extend",
        "mixin:included-hook-include",
        "mixin:included-hook-class-eval-include",
        "mixin:concern-class-methods",
        "method-kind:instance",
        "method-kind:class",
        "def-form:regular",
        "def-form:singleton-class-block",
        "def-form:class-eval-block",
        "def-form:define-method",
        "def-form:const-get-define-method",
        "def-form:module-function-mode",
        "visibility:public",
        "visibility:protected",
        "visibility:private",
        "visibility-syntax:scope-keyword",
        "visibility-syntax:argument-list",
        "visibility:override",
        "call:bare",
        "call:bare-do-block",
        "call:bare-brace-block",
        "call:bare-lambda",
        "call:bare-proc",
        "call:framework-route-block",
        "call:super",
        "call:local",
        "call:ivar",
        "call:class",
        "call:method-object",
        "call:instance-method-object",
        "call:class-receiver",
        "call:constructor",
        "call:static-send",
        "call:one-hop",
        "call:receiver-local",
        "call:array-block-param",
        "call:yield-block-param",
        "constant-ref:auto",
        "constant-ref:absolute",
        "constant-ref:const-get",
        "constant-ref:const-defined",
        "constant-ref:relative-name",
        "constant-ref:qualified",
        "macro:alias",
        "macro:delegate",
        "macro:class-attribute",
        "edit:delete-method",
        "edit:restore-method",
        "edit:delete-constant",
        "edit:restore-constant",
        "edit:delete-namespace",
        "edit:restore-namespace",
        "edit:remove-include",
        "edit:add-include",
        "edit:remove-prepend",
        "edit:add-prepend",
        "edit:change-superclass",
        "edit:clear-superclass",
    ];

    let projects = simulation_coverage_projects();
    let coverage = SimulationCoverage::from_projects(&projects);
    coverage.require(REQUIRED_BUCKETS);
}

#[tokio::test]
async fn generated_project_semantic_diagnostic_false_positive_budget_is_zero() {
    let runner = SimulationRunner::start(phase1_project()).await;
    runner.assert_semantic_false_positive_budget(0).await;
}

#[test]
fn generated_project_emits_complex_ruby_graph() {
    let project = phase1_project();
    let render = project.render();

    assert!(
        (40..=60).contains(&render.files.len()),
        "expected 40-60 files, got {}",
        render.files.len()
    );
    assert!(
        (100..=150).contains(&project.enabled_method_count()),
        "expected 100-150 methods, got {}",
        project.enabled_method_count()
    );
    assert!(
        (30..=60).contains(&project.meaningful_edge_count()),
        "expected 30-60 meaningful graph edges, got {}",
        project.meaningful_edge_count()
    );
    assert!(render
        .map
        .calls
        .iter()
        .any(|call| call.shape_name == "ivar"));
    assert!(render
        .map
        .calls
        .iter()
        .any(|call| call.shape_name == "one-hop"));
    for shape in [
        "bare-do-block",
        "bare-brace-block",
        "bare-lambda",
        "bare-proc",
        "array-block-param",
        "yield-block-param",
    ] {
        assert!(
            render.map.calls.iter().any(|call| call.shape_name == shape),
            "expected generated call shape `{shape}`"
        );
    }
    assert!(!render.map.constant_refs.is_empty());
}

#[test]
fn generated_project_applies_graph_edits_to_files() {
    let mut project = phase1_project();
    let initial = project.render();

    let step = project
        .edits
        .iter()
        .find(|step| step.name == "delete invoice currency")
        .expect("test edit must exist")
        .clone();
    project.apply_step(&step);
    let after_constant_delete = project.render();
    assert_ne!(
        initial.files["billing/invoice.rb"],
        after_constant_delete.files["billing/invoice.rb"]
    );
    assert_eq!(
        initial.files["payments/gateway.rb"],
        after_constant_delete.files["payments/gateway.rb"]
    );

    let step = project
        .edits
        .iter()
        .find(|step| step.name == "change inheritance and mixin")
        .expect("test edit must exist")
        .clone();
    project.apply_step(&step);
    let after_inheritance = project.render();
    assert!(after_inheritance.files["billing/invoice.rb"].contains("Payments::FallbackGateway"));
    assert!(!after_inheritance.files["billing/invoice.rb"].contains("include Audit::Trackable"));
}

#[test]
fn generated_project_oracle_resolves_mro_table() {
    let mut project = mro_project();
    let render = project.render();
    let oracle = OracleState::all_files(&project, &render.map);
    assert_eq!(
        resolved_signature(&oracle, "SimMro::Child", "resolve_token"),
        Some("SimMro::Second#resolve_token".to_string())
    );

    project.apply_op(&EditOp::RemoveInclude {
        owner: "SimMro::Child".to_string(),
        included: "SimMro::Second".to_string(),
    });
    let render = project.render();
    let oracle = OracleState::all_files(&project, &render.map);
    assert_eq!(
        resolved_signature(&oracle, "SimMro::Child", "resolve_token"),
        Some("SimMro::First#resolve_token".to_string())
    );

    project.apply_op(&EditOp::RemoveInclude {
        owner: "SimMro::Child".to_string(),
        included: "SimMro::First".to_string(),
    });
    let render = project.render();
    let oracle = OracleState::all_files(&project, &render.map);
    assert_eq!(
        resolved_signature(&oracle, "SimMro::Child", "resolve_token"),
        Some("SimMro::Base#resolve_token".to_string())
    );

    project.apply_op(&EditOp::DeleteMethod(MethodTarget::parse(
        "SimMro::Base#resolve_token",
    )));
    let render = project.render();
    let oracle = OracleState::all_files(&project, &render.map);
    assert_eq!(
        resolved_signature(&oracle, "SimMro::Child", "resolve_token"),
        None
    );
}

#[test]
fn generated_project_oracle_resolves_prepend_before_own_method() {
    let mut project = mro_prepend_project();
    let render = project.render();
    let oracle = OracleState::all_files(&project, &render.map);
    assert_eq!(
        resolved_signature(&oracle, "SimMroPrepend::Child", "resolve_token"),
        Some("SimMroPrepend::Second#resolve_token".to_string())
    );

    project.apply_op(&EditOp::RemovePrepend {
        owner: "SimMroPrepend::Child".to_string(),
        prepended: "SimMroPrepend::Second".to_string(),
    });
    let render = project.render();
    let oracle = OracleState::all_files(&project, &render.map);
    assert_eq!(
        resolved_signature(&oracle, "SimMroPrepend::Child", "resolve_token"),
        Some("SimMroPrepend::First#resolve_token".to_string())
    );

    project.apply_op(&EditOp::RemovePrepend {
        owner: "SimMroPrepend::Child".to_string(),
        prepended: "SimMroPrepend::First".to_string(),
    });
    let render = project.render();
    let oracle = OracleState::all_files(&project, &render.map);
    assert_eq!(
        resolved_signature(&oracle, "SimMroPrepend::Child", "resolve_token"),
        Some("SimMroPrepend::Child#resolve_token".to_string())
    );
}

#[test]
fn generated_project_oracle_tracks_partial_namespace_visibility() {
    let project = mro_project();
    let render = project.render();
    let indexed_files = ["sim_mro/child.rb", "sim_mro/caller.rb", "sim_mro/base.rb"]
        .into_iter()
        .map(String::from)
        .collect::<BTreeSet<_>>();
    let oracle = OracleState::with_indexed_files(&project, &render.map, indexed_files);

    assert_eq!(
        resolved_signature(&oracle, "SimMro::Child", "resolve_token"),
        Some("SimMro::Base#resolve_token".to_string())
    );
}

#[test]
fn generated_project_oracle_resolves_constant_namespace_table() {
    let project = const_namespace_project();
    let render = project.render();
    let oracle = OracleState::all_files(&project, &render.map);

    assert_eq!(
        resolved_constant(&oracle, &project, "TOKEN"),
        Some("SimConst::Outer::Inner::TOKEN".to_string())
    );
    assert_eq!(
        resolved_constant(&oracle, &project, "PARENT_TOKEN"),
        Some("SimConst::Outer::PARENT_TOKEN".to_string())
    );
    assert_eq!(
        resolved_constant(&oracle, &project, "::SimConst::Outer::TOKEN"),
        Some("SimConst::Outer::TOKEN".to_string())
    );
    assert_eq!(
        resolved_constant(&oracle, &project, "Shared::TOKEN"),
        Some("SimConst::Shared::TOKEN".to_string())
    );
}

#[tokio::test]
async fn generated_project_resolves_reopened_namespace_fragments() {
    let runner = SimulationRunner::start(reopened_namespace_project()).await;

    runner.check_definitions().await;
    runner.check_references().await;
    runner.check_hover().await;
    runner.check_types().await;
}

#[tokio::test]
async fn generated_project_checks_block_type_flow_hover_and_hints() {
    let runner = SimulationRunner::start(block_type_flow_project()).await;

    runner.check_hover().await;
    runner.check_types().await;
}

#[test]
fn generated_project_oracle_tracks_partial_constant_visibility() {
    let project = const_namespace_project();
    let render = project.render();
    let indexed_files = [
        "sim_const/outer.rb",
        "sim_const/outer/inner.rb",
        "sim_const/outer/inner/reader.rb",
    ]
    .into_iter()
    .map(String::from)
    .collect::<BTreeSet<_>>();
    let oracle = OracleState::with_indexed_files(&project, &render.map, indexed_files);

    assert_eq!(resolved_constant(&oracle, &project, "Shared::TOKEN"), None);
    assert_eq!(
        resolved_constant(&oracle, &project, "PARENT_TOKEN"),
        Some("SimConst::Outer::PARENT_TOKEN".to_string())
    );
}

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

#[test]
fn generated_project_engine_oracle_checks_definitions_and_references() {
    let runner = EngineSimulationRunner::start(phase1_project());
    runner.check_definitions();
    runner.check_references();
    runner.check_types();
}

#[test]
fn generated_project_engine_oracle_updates_after_graph_edits() {
    let project = phase1_project();
    let mut runner = EngineSimulationRunner::start(project.clone());
    runner.check_definitions();
    runner.check_references();
    runner.check_types();

    for step in &project.edits {
        runner.apply_step(step);
        runner.check_definitions();
        runner.check_references();
        runner.check_types();
    }
}

#[tokio::test]
async fn generated_project_updates_method_diagnostics() {
    let project = phase1_project();
    let mut runner = SimulationRunner::start(project.clone()).await;
    runner.check_initial().await;

    runner
        .apply_step(
            project
                .edits
                .iter()
                .find(|step| step.name == "delete gateway capture")
                .expect("test edit must exist"),
        )
        .await;
    runner
        .apply_step(
            project
                .edits
                .iter()
                .find(|step| step.name == "restore gateway capture")
                .expect("test edit must exist"),
        )
        .await;
}

#[tokio::test]
async fn generated_project_runtime_core_methods_do_not_warn() {
    let mut project = SyntheticProject::new("synthetic_runtime_core");
    project
        .class("BasicObject", |_class| {})
        .module("Kernel", |module| {
            module.method("puts");
            module.method("warn");
            module.method("nil?");
        })
        .class("Object", |class| {
            class.include("Kernel");
        })
        .class("ScenarioExtractor", |class| {
            class
                .method("to_output")
                .calls("Kernel#puts", CallShape::Bare);
            class
                .method("to_warning")
                .calls("Kernel#warn", CallShape::Bare);
            class
                .method("to_nil_check")
                .calls("Kernel#nil?", CallShape::Bare);
        })
        .class("MinimalRuntime::Leaf", |class| {
            class.superclass("BasicObject");
            class
                .method("to_output")
                .calls("Kernel#puts", CallShape::Bare);
        });

    let runner = SimulationRunner::start(project).await;
    runner
        .assert_no_unresolved_method("scenario_extractor.rb", "puts")
        .await;
    runner
        .assert_no_unresolved_method("scenario_extractor.rb", "warn")
        .await;
    runner
        .assert_no_unresolved_method("scenario_extractor.rb", "nil?")
        .await;
    runner
        .assert_unresolved_method("minimal_runtime/leaf.rb", "puts")
        .await;
}

#[tokio::test]
async fn generated_project_rbs_builtin_methods_do_not_warn() {
    let mut project = SyntheticProject::new("synthetic_rbs_builtins");
    project.raw_file(
        "stdlib/core_receivers.rb",
        r#"
items = [1, 2, 3]
items.include?(2)
items.empty?
items.first
items.fetch(0)
items << 4

lookup = {name: "ruby", count: 1}
lookup.fetch(:name)
lookup.empty?
lookup.each_key {}

name = "ruby"
name.empty?
name.upcase
name.to_s

count = 1
count.zero?
count.to_s

missing = nil
missing.nil?
missing.to_s

items.nope_builtin(2)
lookup.nope_hash
name.nope_string
count.nope_integer
missing.nope_nil
"#,
    );

    let runner = SimulationRunner::start(project).await;
    runner
        .assert_no_unresolved_method("stdlib/core_receivers.rb", "include?")
        .await;
    runner
        .assert_no_unresolved_method("stdlib/core_receivers.rb", "empty?")
        .await;
    runner
        .assert_no_unresolved_method("stdlib/core_receivers.rb", "first")
        .await;
    runner
        .assert_no_unresolved_method("stdlib/core_receivers.rb", "fetch")
        .await;
    runner
        .assert_no_unresolved_method("stdlib/core_receivers.rb", "<<")
        .await;
    runner
        .assert_no_unresolved_method("stdlib/core_receivers.rb", "each_key")
        .await;
    runner
        .assert_no_unresolved_method("stdlib/core_receivers.rb", "upcase")
        .await;
    runner
        .assert_no_unresolved_method("stdlib/core_receivers.rb", "to_s")
        .await;
    runner
        .assert_no_unresolved_method("stdlib/core_receivers.rb", "zero?")
        .await;
    runner
        .assert_no_unresolved_method("stdlib/core_receivers.rb", "nil?")
        .await;
    runner
        .assert_unresolved_method("stdlib/core_receivers.rb", "nope_builtin")
        .await;
    runner
        .assert_unresolved_method("stdlib/core_receivers.rb", "nope_hash")
        .await;
    runner
        .assert_unresolved_method("stdlib/core_receivers.rb", "nope_string")
        .await;
    runner
        .assert_unresolved_method("stdlib/core_receivers.rb", "nope_integer")
        .await;
}

#[tokio::test]
async fn generated_project_updates_constant_diagnostics() {
    let project = phase1_project();
    let mut runner = SimulationRunner::start(project.clone()).await;
    runner.check_initial().await;

    runner
        .apply_step(
            project
                .edits
                .iter()
                .find(|step| step.name == "delete invoice currency")
                .expect("test edit must exist"),
        )
        .await;
    runner
        .apply_step(
            project
                .edits
                .iter()
                .find(|step| step.name == "restore invoice currency")
                .expect("test edit must exist"),
        )
        .await;
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
