use crate::test::simulation::{CallShape, SyntheticProject};

pub(in crate::test::simulation) fn phase1_project() -> SyntheticProject {
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

pub(super) fn mro_project() -> SyntheticProject {
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

pub(super) fn mro_prepend_project() -> SyntheticProject {
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

pub(in crate::test::simulation) fn superclass_switch_project() -> SyntheticProject {
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

pub(super) fn super_call_project() -> SyntheticProject {
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

pub(super) fn const_namespace_project() -> SyntheticProject {
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

pub(super) fn reopened_namespace_project() -> SyntheticProject {
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

pub(super) fn block_type_flow_project() -> SyntheticProject {
    let mut project = SyntheticProject::new("synthetic_block_type_flow");

    project.class("SimBlockType::Builder", |class| {
        class
            .method("build")
            .returns("String")
            .with_block_type_asserts();
    });

    project
}

pub(super) fn singleton_class_block_project() -> SyntheticProject {
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

pub(super) fn class_eval_project() -> SyntheticProject {
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

pub(super) fn eval_constant_scope_project(relative: bool) -> SyntheticProject {
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
