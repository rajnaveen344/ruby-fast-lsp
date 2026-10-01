use crate::test::simulation::{CallShape, SyntheticProject};

pub(super) fn define_method_project() -> SyntheticProject {
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

pub(super) fn const_get_define_method_project() -> SyntheticProject {
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

pub(super) fn const_get_constant_ref_project() -> SyntheticProject {
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

pub(super) fn const_defined_constant_ref_project() -> SyntheticProject {
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

pub(super) fn static_send_project() -> SyntheticProject {
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

pub(super) fn visibility_project() -> SyntheticProject {
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

pub(super) fn module_function_mode_project() -> SyntheticProject {
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

pub(super) fn extend_self_project() -> SyntheticProject {
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

pub(super) fn extend_class_method_project() -> SyntheticProject {
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

pub(super) fn singleton_class_mixin_project() -> SyntheticProject {
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

pub(super) fn included_hook_project() -> SyntheticProject {
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

pub(super) fn concern_project() -> SyntheticProject {
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

pub(super) fn alias_project() -> SyntheticProject {
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

pub(super) fn delegate_project() -> SyntheticProject {
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

pub(super) fn forwardable_project() -> SyntheticProject {
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

pub(super) fn class_attribute_project() -> SyntheticProject {
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

pub(super) fn method_missing_project() -> SyntheticProject {
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

pub(super) fn framework_route_block_project() -> SyntheticProject {
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

pub(super) fn method_object_project() -> SyntheticProject {
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
