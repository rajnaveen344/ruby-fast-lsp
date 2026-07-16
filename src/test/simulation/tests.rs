use super::{
    large_scale_project, seeded_script, simulation_seeds_from_env, write_seed_artifact, CallShape,
    ConstantRefShape, EditOp, EditStep, EngineSimulationRunner, MethodDefForm, MethodKind,
    MethodTarget, MethodVisibility, MethodVisibilitySyntax, NamespaceKind, OracleState,
    SimulationRunner, SyntheticProject, LARGE_SCALE_METHOD_DEFS, LARGE_SCALE_MIN_GRAPH_EDGES,
    LARGE_SCALE_RUBY_FILES,
};
use crate::capabilities::indexing;
use crate::indexer::file_processor::FileProcessor;
use crate::server::RubyLanguageServer;
use crate::test::harness::FakeEditor;
use ruby_analysis::core::{FullyQualifiedName, SourceKind, TextRange};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tower_lsp::lsp_types::{
    DidOpenTextDocumentParams, GotoDefinitionParams, GotoDefinitionResponse, HoverParams,
    InlayHintParams, Location, PartialResultParams, Position, Range, ReferenceContext,
    ReferenceParams, TextDocumentIdentifier, TextDocumentItem, TextDocumentPositionParams,
    WorkDoneProgressParams,
};
use tower_lsp::LanguageServer;

use super::ruby_gen::{CallSite, SourcePos};

fn phase1_project() -> SyntheticProject {
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

fn superclass_switch_project() -> SyntheticProject {
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
        (40..=60).contains(&render.files.len()),
        "seed {seed}: expected 40-60 files, got {}",
        render.files.len()
    );
    assert!(
        (100..=150).contains(&first.project.enabled_method_count()),
        "seed {seed}: expected 100-150 methods, got {}",
        first.project.enabled_method_count()
    );
    assert!(
        (30..=90).contains(&first.project.meaningful_edge_count()),
        "seed {seed}: expected 30-90 meaningful graph edges, got {}",
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
        "fnv1a64:2b7aab09bbb01f75"
    );
}

#[tokio::test]
async fn generated_project_tracks_navigation_support_gaps() {
    let runner = SimulationRunner::start(phase1_project()).await;
    let gaps = runner.known_gap_reasons();
    let expected = [].into_iter().collect();

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

#[tokio::test]
async fn generated_project_large_scale_smoke() {
    if std::env::var("SIM_LARGE_SCALE").ok().as_deref() != Some("1") {
        eprintln!(
            "skipping large-scale smoke; run with SIM_LARGE_SCALE=1 cargo test generated_project_large_scale_smoke -- --nocapture"
        );
        return;
    }

    let seed = std::env::var("SIM_LARGE_SCALE_SEED")
        .ok()
        .map(|value| {
            value.parse::<u64>().unwrap_or_else(|err| {
                panic!(
                    "INVARIANT VIOLATED: SIM_LARGE_SCALE_SEED `{}` is not a u64: {}. This is a bug because scale replay needs a numeric seed. Fix: pass SIM_LARGE_SCALE_SEED=<u64>.",
                    value, err
                )
            })
        })
        .unwrap_or(20_260_524);
    let project = large_scale_project(seed);
    let render = project.render();

    assert_eq!(render.files.len(), LARGE_SCALE_RUBY_FILES);
    assert_eq!(project.enabled_method_count(), LARGE_SCALE_METHOD_DEFS);
    assert!(project.meaningful_edge_count() >= LARGE_SCALE_MIN_GRAPH_EDGES);

    let engine_start = Instant::now();
    let engine_runner = EngineSimulationRunner::start(project.clone());
    let engine_elapsed = engine_start.elapsed();
    let stats = engine_runner.stats();
    eprintln!(
        "large-scale engine index: elapsed={:?} files={} methods={} refs={} types={} graph_edges={}",
        engine_elapsed, stats.files, stats.methods, stats.references, stats.types, stats.graph_edges
    );
    assert_elapsed_under_env_budget(
        "SIM_LARGE_SCALE_ENGINE_MAX_MS",
        Duration::from_secs(120),
        engine_elapsed,
        "large-scale engine index",
    );
    assert!(
        stats.files >= LARGE_SCALE_RUBY_FILES,
        "expected at least {} indexed files, got {:?}",
        LARGE_SCALE_RUBY_FILES,
        stats
    );
    assert!(
        stats.methods >= LARGE_SCALE_METHOD_DEFS,
        "expected at least {} indexed methods, got {:?}",
        LARGE_SCALE_METHOD_DEFS,
        stats
    );

    let samples = sample_call_shapes(&render.map.calls);
    let lsp_start = Instant::now();
    let mut editor = FakeEditor::new().await;
    let mut opened = BTreeSet::new();
    for call in &samples {
        let def = render.map.defs.get(&call.target).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: generated sample target `{}` has no def. This is a bug because source map should contain every generated target. Fix: inspect large_scale_project.",
                call.target.signature()
            )
        });
        open_once(&mut editor, &render.files, &mut opened, &def.file).await;
    }
    for call in &samples {
        if opened.contains(&call.pos.file) {
            editor.close(&call.pos.file).await;
            opened.remove(&call.pos.file);
        }
        open_once(&mut editor, &render.files, &mut opened, &call.pos.file).await;
    }

    for call in &samples {
        let def = render.map.defs.get(&call.target).unwrap_or_else(|| {
            panic!(
                "INVARIANT VIOLATED: generated sample target `{}` has no def. This is a bug because source map should contain every generated target. Fix: inspect large_scale_project.",
                call.target.signature()
            )
        });

        let defs = editor
            .goto_def_at(&call.pos.file, call.pos.line, call.pos.character)
            .await;
        assert!(
            defs.iter()
                .any(|location| location_matches_pos(location, def)),
            "expected scale goto for `{}` shape `{}` from {}:{} to include {}:{}, got {:?}",
            call.target.signature(),
            call.shape_name,
            call.pos.file,
            call.pos.line,
            def.file,
            def.line,
            defs
        );

        let refs = editor
            .references_at(&def.file, def.line, def.character)
            .await;
        assert!(
            refs.iter()
                .any(|location| location_matches_pos(location, &call.pos)),
            "expected scale refs for `{}` shape `{}` to include {}:{}, got {:?}",
            call.target.signature(),
            call.shape_name,
            call.pos.file,
            call.pos.line,
            refs
        );

        let hover = editor
            .hover_at(&call.pos.file, call.pos.line, call.pos.character)
            .await
            .unwrap_or_else(|| {
                panic!(
                    "expected scale hover for `{}` shape `{}` at {}:{}",
                    call.target.signature(),
                    call.shape_name,
                    call.pos.file,
                    call.pos.line
                )
            });
        let hover_text = format!("{hover:?}");
        assert!(
            !hover_text.trim().is_empty(),
            "expected non-empty scale hover for `{}` shape `{}`, got {}",
            call.target.signature(),
            call.shape_name,
            hover_text
        );
    }

    let local_call = render
        .map
        .calls
        .iter()
        .find(|call| call.shape_name == "local")
        .expect("scale project must generate a local receiver call");
    let hints = editor.inlay_hints(&local_call.pos.file).await;
    assert!(
        hints
            .iter()
            .any(|hint| hint.position.line + 1 == local_call.pos.line),
        "expected constructor local receiver type hint before scale call {}:{}, got {:?}",
        local_call.pos.file,
        local_call.pos.line,
        hints
    );
    let lsp_elapsed = lsp_start.elapsed();
    eprintln!(
        "large-scale sampled LSP checks: elapsed={:?} samples={}",
        lsp_elapsed,
        samples.len()
    );
    assert_elapsed_under_env_budget(
        "SIM_LARGE_SCALE_LSP_MAX_MS",
        Duration::from_secs(120),
        lsp_elapsed,
        "large-scale sampled LSP checks",
    );
}

#[test]
fn generated_project_large_scale_engine_checks_all_edges() {
    if std::env::var("SIM_LARGE_SCALE_ALL_EDGES").ok().as_deref() != Some("1") {
        eprintln!(
            "skipping large-scale all-edge engine smoke; run with SIM_LARGE_SCALE_ALL_EDGES=1 cargo test generated_project_large_scale_engine_checks_all_edges -- --nocapture"
        );
        return;
    }

    let seed = std::env::var("SIM_LARGE_SCALE_SEED")
        .ok()
        .map(|value| {
            value.parse::<u64>().unwrap_or_else(|err| {
                panic!(
                    "INVARIANT VIOLATED: SIM_LARGE_SCALE_SEED `{}` is not a u64: {}. This is a bug because scale replay needs a numeric seed. Fix: pass SIM_LARGE_SCALE_SEED=<u64>.",
                    value, err
                )
            })
        })
        .unwrap_or(20_260_524);
    let project = large_scale_project(seed);
    let render = project.render();
    eprintln!(
        "large-scale all-edge shape: files={} methods={} edges={} calls={} const_refs={}",
        render.files.len(),
        project.enabled_method_count(),
        project.meaningful_edge_count(),
        render.map.calls.len(),
        render.map.constant_refs.len()
    );

    let engine_start = Instant::now();
    let runner = EngineSimulationRunner::start(project);
    let stats = runner.stats();
    let engine_elapsed = engine_start.elapsed();
    eprintln!(
        "large-scale all-edge index stats: elapsed={:?} files={} methods={} refs={} types={} graph_edges={}",
        engine_elapsed, stats.files, stats.methods, stats.references, stats.types, stats.graph_edges
    );
    assert_elapsed_under_env_budget(
        "SIM_LARGE_SCALE_ALL_EDGES_INDEX_MAX_MS",
        Duration::from_secs(120),
        engine_elapsed,
        "large-scale all-edge engine index",
    );
    assert_eq!(render.files.len(), LARGE_SCALE_RUBY_FILES);
    assert_eq!(stats.methods, LARGE_SCALE_METHOD_DEFS);

    let check_start = Instant::now();
    runner.check_definitions();
    runner.check_references();
    runner.check_types();
    let check_elapsed = check_start.elapsed();
    eprintln!(
        "large-scale all-edge oracle checks: elapsed={:?}",
        check_elapsed
    );
    assert_elapsed_under_env_budget(
        "SIM_LARGE_SCALE_ALL_EDGES_CHECK_MAX_MS",
        Duration::from_secs(300),
        check_elapsed,
        "large-scale all-edge oracle checks",
    );
}

#[tokio::test]
async fn generated_project_real_corpus_smoke() {
    if std::env::var("SIM_REAL_CORPUS").ok().as_deref() != Some("1") {
        eprintln!(
            "skipping real corpus smoke; run with SIM_REAL_CORPUS=1 SIM_REAL_CORPUS_ROOT=/path/to/app cargo test generated_project_real_corpus_smoke -- --nocapture"
        );
        return;
    }

    let Some(root) = real_corpus_root() else {
        eprintln!("skipping real corpus smoke; set SIM_REAL_CORPUS_ROOT=/path/to/app");
        return;
    };
    if !root.is_dir() {
        eprintln!("skipping real corpus smoke; missing {}", root.display());
        return;
    }

    let files = collect_real_corpus_ruby_files(&root);
    let shape = CorpusShape::from_files(&files);
    eprintln!(
        "real corpus shape: files={} loc={} bytes={} namespace_defs={} method_defs={} rough_call_refs={}",
        shape.files,
        shape.loc,
        shape.bytes,
        shape.namespace_defs,
        shape.method_defs,
        shape.rough_call_refs
    );
    assert!(
        shape.files >= 2_000,
        "expected large-scale Ruby files, got {:?}",
        shape
    );
    assert!(
        shape.method_defs >= 20_000,
        "expected large-scale method defs, got {:?}",
        shape
    );
    assert!(
        shape.rough_call_refs >= 200_000,
        "expected large-scale call/ref density, got {:?}",
        shape
    );

    let server = RubyLanguageServer::default();
    let workspace_uri = tower_lsp::lsp_types::Url::from_file_path(&root).unwrap_or_else(|_| {
        panic!(
            "INVARIANT VIOLATED: real corpus root `{}` is not file-URI convertible. This is a bug because LSP smoke needs local file URIs. Fix: pass a canonical filesystem path.",
            root.display()
        )
    });
    server.add_workspace(workspace_uri);

    let index_start = Instant::now();
    index_core_stubs_for_smoke(&server);
    index_project_files_for_smoke(&server, &files);
    let index_elapsed = index_start.elapsed();
    let stats = server.analysis_engine.read().stats();
    eprintln!(
        "real corpus index: elapsed={:?} files={} methods={} refs={} types={} graph_edges={} diagnostics={}",
        index_elapsed,
        stats.files,
        stats.methods,
        stats.references,
        stats.types,
        stats.graph_edges,
        stats.diagnostics
    );
    print_real_corpus_diagnostic_sample(&server);
    assert_elapsed_under_env_budget(
        "SIM_REAL_CORPUS_INDEX_MAX_MS",
        Duration::from_secs(1_200),
        index_elapsed,
        "real corpus index",
    );
    assert!(
        stats.files >= shape.files,
        "expected all project files indexed, shape={:?}, stats={:?}",
        shape,
        stats
    );
    assert!(
        stats.methods >= 15_000,
        "expected substantial real method facts, shape={:?}, stats={:?}",
        shape,
        stats
    );
    assert!(
        stats.references >= 10_000,
        "expected substantial real resolved refs, shape={:?}, stats={:?}",
        shape,
        stats
    );
    assert!(
        stats.types >= 1_000,
        "expected substantial real type facts, shape={:?}, stats={:?}",
        shape,
        stats
    );

    let samples = select_real_method_reference_samples(&server, 12);
    assert!(
        samples.len() >= 8,
        "expected at least 8 real method reference samples after indexing {}, got {} samples, stats={:?}",
        root.display(),
        samples.len(),
        stats
    );
    for sample in &samples {
        open_real_document(&server, &sample.definition.path).await;
        if sample.reference.path != sample.definition.path {
            open_real_document(&server, &sample.reference.path).await;
        }

        let defs = goto_def_locations(&server, &sample.reference).await;
        assert!(
            defs.iter()
                .any(|location| location_start_matches(location, &sample.definition)),
            "expected real goto for {} from {}:{} to include {}:{}, got {:?}",
            sample.label,
            sample.reference.path.display(),
            sample.reference.position.line,
            sample.definition.path.display(),
            sample.definition.position.line,
            defs
        );

        let refs = reference_locations(&server, &sample.reference).await;
        assert!(
            refs.iter()
                .any(|location| location_start_matches(location, &sample.reference))
                && refs.len() >= sample.reference_count.min(3),
            "expected real refs for {} at usage {}:{} to include usage and at least {} refs, got {:?}",
            sample.label,
            sample.reference.path.display(),
            sample.reference.position.line,
            sample.reference_count.min(3),
            refs
        );

        let hover = hover_at(&server, &sample.reference)
            .await
            .unwrap_or_else(|| {
                panic!(
                    "expected real hover for {} at {}:{}",
                    sample.label,
                    sample.reference.path.display(),
                    sample.reference.position.line
                )
            });
        assert!(
            !format!("{hover:?}").trim().is_empty(),
            "expected non-empty real hover for {}, got {:?}",
            sample.label,
            hover
        );
    }

    let hinted = assert_real_type_inlay_samples(&server, 3).await;
    eprintln!(
        "real corpus semantic samples: goto/refs/hover={} type_hint_files={}",
        samples.len(),
        hinted
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
}

fn print_real_corpus_diagnostic_sample(server: &RubyLanguageServer) {
    if std::env::var("SIM_REAL_CORPUS_DIAGNOSTIC_SAMPLE")
        .ok()
        .as_deref()
        != Some("1")
    {
        return;
    }

    let engine = server.analysis_engine.read();
    let query = engine.query();
    let mut grouped = BTreeMap::<(String, String), (usize, Vec<String>)>::new();
    for diagnostic in query.all_diagnostic_facts() {
        let key = (diagnostic.code.clone(), diagnostic.message.clone());
        let entry = grouped.entry(key).or_default();
        entry.0 += 1;
        if entry.1.len() < 3 {
            let sample = query
                .file(diagnostic.range.file_id)
                .and_then(|file| {
                    let (line, character) =
                        file.byte_offset_to_line_character(diagnostic.range.start_byte)?;
                    Some(format!(
                        "{}:{}:{}",
                        file.path.display(),
                        line + 1,
                        character + 1
                    ))
                })
                .unwrap_or_else(|| "<unknown>".to_string());
            entry.1.push(sample);
        }
    }

    let mut rows = grouped
        .into_iter()
        .map(|((code, message), (count, samples))| (count, code, message, samples))
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));

    eprintln!("real corpus diagnostics top groups:");
    for (count, code, message, samples) in rows.into_iter().take(12) {
        eprintln!(
            "  count={} code={} message={} samples={}",
            count,
            code,
            message,
            samples.join(", ")
        );
    }
}

fn assert_elapsed_under_env_budget(
    env_name: &str,
    default_budget: Duration,
    elapsed: Duration,
    label: &str,
) {
    let budget = std::env::var(env_name)
        .ok()
        .map(|value| {
            let millis = value.parse::<u64>().unwrap_or_else(|err| {
                panic!(
                    "INVARIANT VIOLATED: {} `{}` is not a u64 millisecond budget: {}. \
                     This is a bug because perf budgets must be numeric. \
                     Fix: pass {}=<milliseconds>.",
                    env_name, value, err, env_name
                )
            });
            Duration::from_millis(millis)
        })
        .unwrap_or(default_budget);

    assert!(
        elapsed <= budget,
        "{} exceeded perf budget {:?}: elapsed {:?}. Override with {}=<milliseconds> if this machine is intentionally slower.",
        label,
        budget,
        elapsed,
        env_name
    );
}

fn sample_call_shapes(calls: &[CallSite]) -> Vec<&CallSite> {
    let required = [
        "bare",
        "local",
        "ivar",
        "class",
        "constructor",
        "one-hop",
        "receiver-local",
        "super",
    ];
    required
        .iter()
        .map(|shape| {
            calls
                .iter()
                .find(|call| {
                    call.shape_name == *shape
                        && call.definition_support.is_supported()
                        && call.reference_support.is_supported()
                        && call.hover_support.is_supported()
                })
                .unwrap_or_else(|| {
                    panic!(
                        "INVARIANT VIOLATED: scale project did not generate supported `{}` call. This is a bug because scale smoke must cover every agent navigation call shape. Fix: inspect large_scale_project.",
                        shape
                    )
                })
        })
        .collect()
}

async fn open_once(
    editor: &mut FakeEditor,
    files: &std::collections::BTreeMap<String, String>,
    opened: &mut BTreeSet<String>,
    file: &str,
) {
    if opened.contains(file) {
        return;
    }
    let content = files.get(file).unwrap_or_else(|| {
        panic!(
            "INVARIANT VIOLATED: scale smoke tried to open missing file `{}`. This is a bug because source map positions must point at rendered files. Fix: inspect render_project.",
            file
        )
    });
    editor.open(file, content).await;
    opened.insert(file.to_string());
}

fn location_matches_pos(location: &tower_lsp::lsp_types::Location, pos: &SourcePos) -> bool {
    location.uri.path().ends_with(&pos.file) && location.range.start.line == pos.line
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

#[derive(Debug)]
struct CorpusShape {
    files: usize,
    loc: usize,
    bytes: usize,
    namespace_defs: usize,
    method_defs: usize,
    rough_call_refs: usize,
}

impl CorpusShape {
    fn from_files(files: &[PathBuf]) -> Self {
        let mut shape = Self {
            files: files.len(),
            loc: 0,
            bytes: 0,
            namespace_defs: 0,
            method_defs: 0,
            rough_call_refs: 0,
        };

        for file in files {
            let content = std::fs::read_to_string(file).unwrap_or_else(|err| {
                panic!(
                    "INVARIANT VIOLATED: failed to read real corpus file `{}`: {}. This is a bug because corpus shape requires readable Ruby files. Fix: inspect file permissions.",
                    file.display(),
                    err
                )
            });
            shape.bytes += content.len();
            for line in content.lines() {
                shape.loc += 1;
                let trimmed = line.trim_start();
                if trimmed.starts_with("class ") || trimmed.starts_with("module ") {
                    shape.namespace_defs += 1;
                }
                if trimmed.starts_with("def ") {
                    shape.method_defs += 1;
                }
                if !trimmed.starts_with('#')
                    && (trimmed.contains('.') || trimmed.contains("::") || trimmed.contains('('))
                {
                    shape.rough_call_refs += 1;
                }
            }
        }

        shape
    }
}

#[derive(Debug, Clone)]
struct LspPoint {
    path: PathBuf,
    position: Position,
}

#[derive(Debug, Clone)]
struct RealMethodSample {
    label: String,
    definition: LspPoint,
    reference: LspPoint,
    reference_count: usize,
}

fn real_corpus_root() -> Option<PathBuf> {
    std::env::var("SIM_REAL_CORPUS_ROOT")
        .ok()
        .map(PathBuf::from)
}

fn collect_real_corpus_ruby_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_real_corpus_ruby_files_into(root, &mut files);
    files.sort();
    files
}

fn collect_real_corpus_ruby_files_into(dir: &Path, files: &mut Vec<PathBuf>) {
    let entries = std::fs::read_dir(dir).unwrap_or_else(|err| {
        panic!(
            "INVARIANT VIOLATED: failed to read corpus dir `{}`: {}. This is a bug because corpus smoke needs readable dirs. Fix: inspect path/permissions.",
            dir.display(),
            err
        )
    });
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if matches!(name, ".git" | "vendor" | ".bundle" | "tmp" | "log") {
                continue;
            }
            collect_real_corpus_ruby_files_into(&path, files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rb") {
            files.push(path);
        }
    }
}

fn index_project_files_for_smoke(server: &RubyLanguageServer, files: &[PathBuf]) {
    index_files_for_smoke(server, files, SourceKind::Project);
}

fn index_core_stubs_for_smoke(server: &RubyLanguageServer) {
    let Some(stubs_dir) = core_stubs_dir_for_smoke() else {
        eprintln!("real corpus smoke: core stubs unavailable");
        return;
    };
    let files = collect_real_corpus_ruby_files(&stubs_dir);
    eprintln!(
        "real corpus smoke: indexing {} core stub files from {}",
        files.len(),
        stubs_dir.display()
    );
    index_files_for_smoke(server, &files, SourceKind::Stub);
}

fn core_stubs_dir_for_smoke() -> Option<PathBuf> {
    let stubs_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("editors")
        .join("vscode")
        .join("vsix")
        .join("stubs");
    [
        "rubystubs33",
        "rubystubs34",
        "rubystubs32",
        "rubystubs31",
        "rubystubs30",
    ]
    .into_iter()
    .map(|name| stubs_root.join(name))
    .find(|path| path.is_dir())
}

fn index_files_for_smoke(server: &RubyLanguageServer, files: &[PathBuf], source_kind: SourceKind) {
    let processor = FileProcessor::with_extension_registry(server.extension_registry.clone());
    for file in files {
        let content = std::fs::read_to_string(file).unwrap_or_else(|err| {
            panic!(
                "INVARIANT VIOLATED: failed to read real corpus file `{}` during indexing: {}. This is a bug because indexed files must be readable. Fix: inspect file permissions.",
                file.display(),
                err
            )
        });
        let uri = tower_lsp::lsp_types::Url::from_file_path(file).unwrap_or_else(|_| {
            panic!(
                "INVARIANT VIOLATED: real corpus file `{}` is not file-URI convertible. This is a bug because LSP indexing requires file URIs. Fix: inspect path.",
                file.display()
            )
        });
        processor
            .collect_file_facts_as_deferred_resolution(&uri, &content, server, source_kind)
            .unwrap_or_else(|err| {
                panic!(
                    "INVARIANT VIOLATED: failed to index real corpus file `{}`: {}. This is a bug because smoke indexing should tolerate valid Ruby project files. Fix: inspect parser/fact collector failure.",
                    file.display(),
                    err
                )
            });
    }
    server.analysis_engine.write().resolve();
}

fn select_real_method_reference_samples(
    server: &RubyLanguageServer,
    limit: usize,
) -> Vec<RealMethodSample> {
    let engine = server.analysis_engine.read();
    let query = engine.query();
    let mut methods = engine.all_method_facts();
    methods.sort_by_key(|method| {
        (
            method.range.file_id,
            method.range.start_byte,
            method.fqn.to_string(),
        )
    });

    let mut samples = Vec::new();
    let mut seen_labels = BTreeSet::new();
    let mut seen_reference_files = BTreeSet::new();

    for method in methods {
        let FullyQualifiedName::Method(_, ruby_method) = &method.fqn else {
            continue;
        };
        if matches!(ruby_method.to_string().as_str(), "new" | "initialize") {
            continue;
        }
        let label = method.fqn.to_string();
        if seen_labels.contains(&label) {
            continue;
        }
        let mut refs = query.method_reference_ranges(&method.owner, ruby_method);
        refs.sort_by_key(|range| (range.file_id, range.start_byte, range.end_byte));
        let reference_count = refs.len();
        for reference_range in refs {
            if reference_range == method.range {
                continue;
            }
            let Some(definition) = lsp_point_for_range(&engine, method.range) else {
                continue;
            };
            let Some(reference) = lsp_point_for_range(&engine, reference_range) else {
                continue;
            };
            if !is_preferred_real_sample_path(&definition.path)
                || !is_preferred_real_sample_path(&reference.path)
            {
                continue;
            }
            let reference_file = reference.path.clone();
            if !seen_reference_files.insert(reference_file) && samples.len() < limit / 2 {
                continue;
            }
            seen_labels.insert(label.clone());
            samples.push(RealMethodSample {
                label: label.clone(),
                definition,
                reference,
                reference_count,
            });
            if samples.len() >= limit {
                return samples;
            }
            break;
        }
    }

    samples
}

fn is_preferred_real_sample_path(path: &Path) -> bool {
    let path = path.to_string_lossy();
    !path.contains("/.ai-docs/")
        && !path.contains("/spec/")
        && !path.contains("/editors/vscode/vsix/stubs/")
}

fn lsp_point_for_range(
    engine: &ruby_analysis::engine::AnalysisEngine,
    range: TextRange,
) -> Option<LspPoint> {
    let file = engine.file(range.file_id)?;
    let (line, character) = file.byte_offset_to_line_character(range.start_byte)?;
    Some(LspPoint {
        path: file.path.clone(),
        position: Position::new(line, character),
    })
}

async fn open_real_document(server: &RubyLanguageServer, path: &Path) {
    let uri = tower_lsp::lsp_types::Url::from_file_path(path).unwrap_or_else(|_| {
        panic!(
            "INVARIANT VIOLATED: real document `{}` is not file-URI convertible. This is a bug because didOpen requires a valid file URI. Fix: inspect path.",
            path.display()
        )
    });
    if server.docs.lock().contains_key(&uri) {
        return;
    }
    let content = std::fs::read_to_string(path).unwrap_or_else(|err| {
        panic!(
            "INVARIANT VIOLATED: failed to open real document `{}`: {}. This is a bug because LSP smoke samples must be readable. Fix: inspect file permissions.",
            path.display(),
            err
        )
    });
    indexing::handle_did_open(
        server,
        DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri,
                language_id: "ruby".to_string(),
                version: 1,
                text: content,
            },
        },
    )
    .await;
}

async fn goto_def_locations(server: &RubyLanguageServer, point: &LspPoint) -> Vec<Location> {
    let uri = tower_lsp::lsp_types::Url::from_file_path(&point.path).unwrap_or_else(|_| {
        panic!(
            "INVARIANT VIOLATED: goto point path `{}` is not file-URI convertible. This is a bug because LSP queries need file URIs. Fix: inspect sample selection.",
            point.path.display()
        )
    });
    let params = GotoDefinitionParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri },
            position: point.position,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    };
    match server.goto_definition(params).await {
        Ok(Some(GotoDefinitionResponse::Scalar(location))) => vec![location],
        Ok(Some(GotoDefinitionResponse::Array(locations))) => locations,
        Ok(Some(GotoDefinitionResponse::Link(links))) => links
            .into_iter()
            .map(|link| Location {
                uri: link.target_uri,
                range: link.target_range,
            })
            .collect(),
        Ok(None) => Vec::new(),
        Err(err) => panic!(
            "INVARIANT VIOLATED: real corpus goto request failed at `{}`:{}:{}: {}. This is a bug because LSP requests should return JSON-RPC success. Fix: inspect goto handler.",
            point.path.display(),
            point.position.line,
            point.position.character,
            err
        ),
    }
}

async fn reference_locations(server: &RubyLanguageServer, point: &LspPoint) -> Vec<Location> {
    let uri = tower_lsp::lsp_types::Url::from_file_path(&point.path).unwrap_or_else(|_| {
        panic!(
            "INVARIANT VIOLATED: reference point path `{}` is not file-URI convertible. This is a bug because LSP queries need file URIs. Fix: inspect sample selection.",
            point.path.display()
        )
    });
    let params = ReferenceParams {
        text_document_position: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri },
            position: point.position,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
        context: ReferenceContext {
            include_declaration: true,
        },
    };
    server
        .references(params)
        .await
        .unwrap_or_else(|err| {
            panic!(
                "INVARIANT VIOLATED: real corpus references request failed at `{}`:{}:{}: {}. This is a bug because LSP requests should return JSON-RPC success. Fix: inspect references handler.",
                point.path.display(),
                point.position.line,
                point.position.character,
                err
            )
        })
        .unwrap_or_default()
}

async fn hover_at(
    server: &RubyLanguageServer,
    point: &LspPoint,
) -> Option<tower_lsp::lsp_types::Hover> {
    let uri = tower_lsp::lsp_types::Url::from_file_path(&point.path).unwrap_or_else(|_| {
        panic!(
            "INVARIANT VIOLATED: hover point path `{}` is not file-URI convertible. This is a bug because LSP queries need file URIs. Fix: inspect sample selection.",
            point.path.display()
        )
    });
    let params = HoverParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier { uri },
            position: point.position,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    server.hover(params).await.unwrap_or_else(|err| {
        panic!(
            "INVARIANT VIOLATED: real corpus hover request failed at `{}`:{}:{}: {}. This is a bug because LSP requests should return JSON-RPC success. Fix: inspect hover handler.",
            point.path.display(),
            point.position.line,
            point.position.character,
            err
        )
    })
}

async fn assert_real_type_inlay_samples(server: &RubyLanguageServer, limit: usize) -> Vec<PathBuf> {
    let candidates = {
        let engine = server.analysis_engine.read();
        let mut counts = BTreeMap::new();
        for fact in engine.type_store().all_facts() {
            *counts.entry(fact.range.file_id).or_insert(0usize) += 1;
        }
        let mut candidates = counts
            .into_iter()
            .filter_map(|(file_id, count)| {
                engine.file(file_id).map(|file| (count, file.path.clone()))
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
        candidates
    };

    let mut samples = Vec::new();
    for (_count, path) in candidates.into_iter().take(50) {
        open_real_document(server, &path).await;
        let hints = inlay_hints_for_path(server, &path).await;
        if !hints.is_empty() {
            samples.push(path);
            if samples.len() >= limit {
                return samples;
            }
        }
    }

    assert!(
        !samples.is_empty(),
        "expected at least one real corpus file to produce type inlay hints from indexed type facts"
    );
    samples
}

async fn inlay_hints_for_path(
    server: &RubyLanguageServer,
    path: &Path,
) -> Vec<tower_lsp::lsp_types::InlayHint> {
    let uri = tower_lsp::lsp_types::Url::from_file_path(path).unwrap_or_else(|_| {
        panic!(
            "INVARIANT VIOLATED: inlay hint path `{}` is not file-URI convertible. This is a bug because LSP queries need file URIs. Fix: inspect sample selection.",
            path.display()
        )
    });
    let content = std::fs::read_to_string(path).unwrap_or_else(|err| {
        panic!(
            "INVARIANT VIOLATED: failed to read inlay hint sample `{}`: {}. This is a bug because sampled file must be readable. Fix: inspect file permissions.",
            path.display(),
            err
        )
    });
    let line_count = content.lines().count() as u32;
    let params = InlayHintParams {
        text_document: TextDocumentIdentifier { uri },
        range: Range::new(Position::new(0, 0), Position::new(line_count, 0)),
        work_done_progress_params: WorkDoneProgressParams::default(),
    };
    server
        .inlay_hint(params)
        .await
        .unwrap_or_else(|err| {
            panic!(
                "INVARIANT VIOLATED: real corpus inlay hint request failed for `{}`: {}. This is a bug because LSP requests should return JSON-RPC success. Fix: inspect inlay hint handler.",
                path.display(),
                err
            )
        })
        .unwrap_or_default()
}

fn location_start_matches(location: &Location, point: &LspPoint) -> bool {
    location
        .uri
        .to_file_path()
        .ok()
        .as_deref()
        .is_some_and(|path| path == point.path)
        && location.range.start == point.position
}
