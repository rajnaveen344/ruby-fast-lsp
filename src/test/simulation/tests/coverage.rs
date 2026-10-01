use super::hierarchy_projects::{
    class_eval_project, const_namespace_project, mro_prepend_project, mro_project,
    reopened_namespace_project, singleton_class_block_project, super_call_project,
};
use super::metaprogramming_projects::{
    alias_project, class_attribute_project, concern_project, const_defined_constant_ref_project,
    const_get_constant_ref_project, const_get_define_method_project, define_method_project,
    delegate_project, extend_class_method_project, extend_self_project, forwardable_project,
    framework_route_block_project, included_hook_project, method_missing_project,
    method_object_project, module_function_mode_project, singleton_class_mixin_project,
    static_send_project, visibility_project,
};
use super::{phase1_project, superclass_switch_project};
use crate::test::simulation::{
    CallShape, ConstantRefShape, EditOp, MethodDefForm, MethodKind, MethodVisibility,
    MethodVisibilitySyntax, NamespaceKind, OracleState, SyntheticProject,
};
use std::collections::BTreeMap;

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

pub(super) fn resolved_signature(
    oracle: &OracleState<'_>,
    receiver_owner: &str,
    method: &str,
) -> Option<String> {
    oracle
        .resolve_instance_method(receiver_owner, method)
        .map(|target| target.signature())
}

pub(super) fn resolved_constant(
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
