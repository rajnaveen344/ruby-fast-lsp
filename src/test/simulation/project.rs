use super::graph::{
    CallShape, MethodKind, MethodTarget, MethodVisibility, NamespaceBuilder, NamespaceKind,
    NamespaceSpec,
};
use super::ruby_gen::{render_project, ProjectRender};

#[derive(Debug, Clone)]
pub struct SyntheticProject {
    pub name: String,
    pub namespaces: Vec<NamespaceSpec>,
    pub edits: Vec<EditStep>,
}

impl SyntheticProject {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            namespaces: Vec::new(),
            edits: Vec::new(),
        }
    }

    pub fn class(&mut self, fqn: &str, build: impl FnOnce(&mut NamespaceBuilder<'_>)) -> &mut Self {
        self.namespace(fqn, NamespaceKind::Class, build)
    }

    pub fn module(
        &mut self,
        fqn: &str,
        build: impl FnOnce(&mut NamespaceBuilder<'_>),
    ) -> &mut Self {
        self.namespace(fqn, NamespaceKind::Module, build)
    }

    pub fn filler_classes(&mut self, count: usize) -> &mut Self {
        for idx in 0..count {
            let fqn = format!("Synthetic::Generated::Class{:03}", idx);
            self.class(&fqn, |class| {
                class.constant("TOKEN", &format!("\"token-{}\"", idx));
                let ping = class.method(&format!("ping_{:03}", idx));
                if idx < 10 {
                    ping.ref_const(&format!("Synthetic::Generated::Class{:03}::TOKEN", idx));
                }
                class.class_method(&format!("build_{:03}", idx));
                if idx > 0 && idx <= 18 {
                    class.method(&format!("relay_{:03}", idx)).calls(
                        &format!(
                            "Synthetic::Generated::Class{:03}#ping_{:03}",
                            idx - 1,
                            idx - 1
                        ),
                        CallShape::local("previous"),
                    );
                }
            });
        }
        self
    }

    pub fn edit(&mut self, name: &str, build: impl FnOnce(&mut EditBuilder<'_>)) -> &mut Self {
        let mut step = EditStep {
            name: name.to_string(),
            ops: Vec::new(),
            expected: Vec::new(),
        };
        build(&mut EditBuilder { step: &mut step });
        self.edits.push(step);
        self
    }

    pub fn render(&self) -> ProjectRender {
        render_project(self)
    }

    pub fn method_enabled(&self, target: &MethodTarget) -> bool {
        self.namespaces
            .iter()
            .filter(|namespace| namespace.fqn == target.owner && namespace.enabled)
            .find_map(|namespace| {
                if namespace
                    .methods
                    .iter()
                    .find(|method| method.name == target.name && method.kind == target.kind)
                    .is_some_and(|method| method.enabled)
                {
                    return Some(true);
                }
                namespace
                    .aliases
                    .iter()
                    .find(|alias| alias.new_name == target.name && alias.kind == target.kind)
                    .map(|alias| alias.enabled)
                    .or_else(|| {
                        namespace
                            .delegates
                            .iter()
                            .find(|delegate| {
                                delegate.new_name == target.name && delegate.kind == target.kind
                            })
                            .map(|delegate| delegate.enabled)
                    })
                    .or_else(|| {
                        namespace
                            .class_attributes
                            .iter()
                            .find(|attribute| {
                                attribute.name == target.name
                                    && matches!(
                                        target.kind,
                                        MethodKind::Instance | MethodKind::Class
                                    )
                            })
                            .map(|attribute| attribute.enabled)
                    })
            })
            .unwrap_or(false)
    }

    pub fn method_alias_old_target(&self, target: &MethodTarget) -> Option<MethodTarget> {
        self.namespaces
            .iter()
            .filter(|namespace| namespace.fqn == target.owner && namespace.enabled)
            .find_map(|namespace| {
                namespace
                    .aliases
                    .iter()
                    .find(|alias| alias.new_name == target.name && alias.kind == target.kind)
                    .map(|alias| MethodTarget {
                        owner: target.owner.clone(),
                        name: alias.old_name.clone(),
                        kind: alias.kind,
                    })
            })
    }

    pub fn alias_enabled(&self, target: &MethodTarget) -> bool {
        self.namespaces
            .iter()
            .filter(|namespace| namespace.fqn == target.owner && namespace.enabled)
            .find_map(|namespace| {
                namespace
                    .aliases
                    .iter()
                    .find(|alias| alias.new_name == target.name && alias.kind == target.kind)
            })
            .map(|alias| alias.enabled)
            .unwrap_or(false)
    }

    pub fn delegate_enabled(&self, target: &MethodTarget) -> bool {
        self.namespaces
            .iter()
            .filter(|namespace| namespace.fqn == target.owner && namespace.enabled)
            .find_map(|namespace| {
                namespace.delegates.iter().find(|delegate| {
                    delegate.new_name == target.name && delegate.kind == target.kind
                })
            })
            .map(|delegate| delegate.enabled)
            .unwrap_or(false)
    }

    pub fn constant_enabled(&self, fqn: &str) -> bool {
        self.namespaces
            .iter()
            .filter(|namespace| namespace.enabled)
            .find_map(|namespace| namespace.constant_mut_ref(fqn))
            .map(|constant| constant.enabled)
            .unwrap_or(false)
    }

    pub fn namespace_enabled(&self, fqn: &str) -> bool {
        self.namespaces
            .iter()
            .any(|namespace| namespace.fqn == fqn && namespace.enabled)
    }

    pub fn method_return_type(&self, target: &MethodTarget) -> Option<&str> {
        self.namespaces
            .iter()
            .filter(|namespace| namespace.fqn == target.owner && namespace.enabled)
            .find_map(|namespace| {
                if let Some(method) = namespace
                    .methods
                    .iter()
                    .find(|method| method.name == target.name && method.kind == target.kind)
                {
                    return method.return_type.as_deref();
                }
                namespace
                    .aliases
                    .iter()
                    .find(|alias| alias.new_name == target.name && alias.kind == target.kind)
                    .and_then(|alias| {
                        namespace
                            .methods
                            .iter()
                            .find(|method| {
                                method.name == alias.old_name && method.kind == alias.kind
                            })
                            .and_then(|method| method.return_type.as_deref())
                    })
                    .or_else(|| self.delegate_return_type(namespace, target))
            })
    }

    pub fn method_visibility(&self, target: &MethodTarget) -> MethodVisibility {
        self.namespaces
            .iter()
            .filter(|namespace| namespace.fqn == target.owner && namespace.enabled)
            .find_map(|namespace| {
                namespace
                    .methods
                    .iter()
                    .find(|method| method.name == target.name && method.kind == target.kind)
                    .map(|method| method.visibility)
            })
            .unwrap_or(MethodVisibility::Public)
    }

    fn delegate_return_type<'a>(
        &'a self,
        namespace: &'a NamespaceSpec,
        target: &MethodTarget,
    ) -> Option<&'a str> {
        let delegate = namespace
            .delegates
            .iter()
            .find(|delegate| delegate.new_name == target.name && delegate.kind == target.kind)?;
        let receiver_target = MethodTarget {
            owner: namespace.fqn.clone(),
            name: delegate.receiver_method.clone(),
            kind: match delegate.kind {
                MethodKind::Instance => MethodKind::Instance,
                MethodKind::Class => MethodKind::Class,
            },
        };
        let receiver_owner = self.method_return_type(&receiver_target)?;
        let delegated_target = MethodTarget {
            owner: receiver_owner.to_string(),
            name: delegate.new_name.clone(),
            kind: MethodKind::Instance,
        };
        self.method_return_type(&delegated_target)
    }

    pub fn enabled_method_count(&self) -> usize {
        self.namespaces
            .iter()
            .filter(|namespace| namespace.enabled)
            .flat_map(|namespace| namespace.methods.iter())
            .filter(|method| method.enabled)
            .count()
            + self
                .namespaces
                .iter()
                .filter(|namespace| namespace.enabled)
                .flat_map(|namespace| namespace.aliases.iter())
                .filter(|alias| alias.enabled)
                .count()
            + self
                .namespaces
                .iter()
                .filter(|namespace| namespace.enabled)
                .flat_map(|namespace| namespace.delegates.iter())
                .filter(|delegate| delegate.enabled)
                .count()
            + self
                .namespaces
                .iter()
                .filter(|namespace| namespace.enabled)
                .flat_map(|namespace| namespace.class_attributes.iter())
                .filter(|attribute| attribute.enabled)
                .count()
                * 2
    }

    pub fn meaningful_edge_count(&self) -> usize {
        self.namespaces
            .iter()
            .filter(|namespace| namespace.enabled)
            .map(|namespace| {
                namespace
                    .methods
                    .iter()
                    .filter(|method| method.enabled)
                    .map(|method| method.calls.len() + method.constant_refs.len())
                    .sum::<usize>()
                    + namespace
                        .prepends
                        .iter()
                        .filter(|prepend| prepend.enabled)
                        .count()
                    + namespace
                        .includes
                        .iter()
                        .filter(|include| include.enabled)
                        .count()
                    + namespace
                        .extends
                        .iter()
                        .filter(|extend| extend.enabled)
                        .count()
                    + usize::from(namespace.extend_self)
                    + namespace
                        .singleton_prepends
                        .iter()
                        .filter(|prepend| prepend.enabled)
                        .count()
                    + namespace
                        .singleton_includes
                        .iter()
                        .filter(|include| include.enabled)
                        .count()
                    + namespace
                        .included_hook_extends
                        .iter()
                        .filter(|extend| extend.enabled)
                        .count()
                    + namespace
                        .included_hook_includes
                        .iter()
                        .filter(|include| include.enabled)
                        .count()
                    + namespace
                        .included_hook_class_eval_includes
                        .iter()
                        .filter(|include| include.enabled)
                        .count()
                    + namespace
                        .concern_class_methods
                        .iter()
                        .filter(|class_methods| class_methods.enabled)
                        .count()
                    + namespace
                        .visibility_overrides
                        .iter()
                        .filter(|visibility_override| visibility_override.enabled)
                        .count()
                    + namespace
                        .delegates
                        .iter()
                        .filter(|delegate| delegate.enabled)
                        .count()
                    + namespace
                        .class_attributes
                        .iter()
                        .filter(|attribute| attribute.enabled)
                        .count()
                    + usize::from(namespace.superclass.is_some())
            })
            .sum()
    }

    pub fn apply_step(&mut self, step: &EditStep) {
        for op in &step.ops {
            self.apply_op(op);
        }
    }

    pub fn apply_op(&mut self, op: &EditOp) {
        match op {
            EditOp::DeleteMethod(target) => self.set_method_enabled(target, false),
            EditOp::RestoreMethod(target) => self.set_method_enabled(target, true),
            EditOp::DeleteConstant(fqn) => self.set_constant_enabled(fqn, false),
            EditOp::RestoreConstant(fqn) => self.set_constant_enabled(fqn, true),
            EditOp::DeleteNamespace(fqn) => self.set_namespace_enabled(fqn, false),
            EditOp::RestoreNamespace(fqn) => self.set_namespace_enabled(fqn, true),
            EditOp::RemoveInclude { owner, included } => {
                self.set_include_enabled(owner, included, false);
            }
            EditOp::AddInclude { owner, included } => {
                self.set_include_enabled(owner, included, true);
            }
            EditOp::RemovePrepend { owner, prepended } => {
                self.set_prepend_enabled(owner, prepended, false);
            }
            EditOp::AddPrepend { owner, prepended } => {
                self.set_prepend_enabled(owner, prepended, true);
            }
            EditOp::ChangeSuperclass { owner, superclass } => {
                self.set_superclass(owner, Some(superclass.clone()));
            }
            EditOp::ClearSuperclass { owner } => self.set_superclass(owner, None),
        }
    }

    fn namespace(
        &mut self,
        fqn: &str,
        kind: NamespaceKind,
        build: impl FnOnce(&mut NamespaceBuilder<'_>),
    ) -> &mut Self {
        self.namespaces.push(NamespaceSpec::new(fqn, kind));
        let namespace = self
            .namespaces
            .last_mut()
            .expect("INVARIANT VIOLATED: just-pushed namespace is missing. This is a bug because Vec::push must create a last element. Fix: inspect SyntheticProject::namespace.");
        build(&mut NamespaceBuilder::new(namespace));
        self
    }

    fn set_method_enabled(&mut self, target: &MethodTarget, enabled: bool) {
        for namespace in &mut self.namespaces {
            if let Some(method) = namespace.method_mut(target) {
                method.enabled = enabled;
                return;
            }
        }

        panic!(
            "INVARIANT VIOLATED: edit target `{}` does not exist. This is a bug because simulation edits must target declared methods. Fix: declare the method before editing it.",
            target.signature()
        );
    }

    fn set_constant_enabled(&mut self, fqn: &str, enabled: bool) {
        for namespace in &mut self.namespaces {
            if let Some(constant) = namespace.constant_mut(fqn) {
                constant.enabled = enabled;
                return;
            }
        }

        panic!(
            "INVARIANT VIOLATED: constant edit target `{}` does not exist. This is a bug because simulation edits must target declared constants. Fix: declare the constant before editing it.",
            fqn
        );
    }

    fn set_namespace_enabled(&mut self, fqn: &str, enabled: bool) {
        let mut found = false;
        for namespace in self
            .namespaces
            .iter_mut()
            .filter(|namespace| namespace.fqn == fqn)
        {
            namespace.enabled = enabled;
            found = true;
        }
        assert!(
            found,
            "INVARIANT VIOLATED: namespace edit target `{}` does not exist. This is a bug because simulation edits must target declared namespaces. Fix: declare the namespace before editing it.",
            fqn
        );
    }

    fn set_include_enabled(&mut self, owner: &str, included: &str, enabled: bool) {
        let namespace = self
            .namespaces
            .iter_mut()
            .find(|namespace| namespace.fqn == owner)
            .unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: include owner `{}` does not exist. This is a bug because simulation include edits must target declared namespaces. Fix: declare the owner before editing it.",
                    owner
                )
            });

        if let Some(include) = namespace
            .includes
            .iter_mut()
            .find(|include| include.fqn == included)
        {
            include.enabled = enabled;
            return;
        }

        namespace.includes.push(super::graph::IncludeSpec {
            fqn: included.to_string(),
            enabled,
        });
    }

    fn set_prepend_enabled(&mut self, owner: &str, prepended: &str, enabled: bool) {
        let namespace = self
            .namespaces
            .iter_mut()
            .find(|namespace| namespace.fqn == owner)
            .unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: prepend owner `{}` does not exist. This is a bug because simulation prepend edits must target declared namespaces. Fix: declare the owner before editing it.",
                    owner
                )
            });

        if let Some(prepend) = namespace
            .prepends
            .iter_mut()
            .find(|prepend| prepend.fqn == prepended)
        {
            prepend.enabled = enabled;
            return;
        }

        namespace.prepends.push(super::graph::IncludeSpec {
            fqn: prepended.to_string(),
            enabled,
        });
    }

    fn set_superclass(&mut self, owner: &str, superclass: Option<String>) {
        let namespace = self
            .namespaces
            .iter_mut()
            .find(|namespace| namespace.fqn == owner)
            .unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: superclass owner `{}` does not exist. This is a bug because simulation superclass edits must target declared classes. Fix: declare the owner before editing it.",
                    owner
                )
            });
        namespace.superclass = superclass;
    }
}

trait NamespaceConstLookup {
    fn constant_mut_ref(&self, fqn: &str) -> Option<&super::graph::ConstantSpec>;
}

impl NamespaceConstLookup for NamespaceSpec {
    fn constant_mut_ref(&self, fqn: &str) -> Option<&super::graph::ConstantSpec> {
        let prefix = format!("{}::", self.fqn);
        let name = fqn.strip_prefix(&prefix)?;
        self.constants.iter().find(|constant| constant.name == name)
    }
}

#[derive(Debug, Clone)]
pub struct EditStep {
    pub name: String,
    pub ops: Vec<EditOp>,
    pub expected: Vec<ExpectedCheck>,
}

impl EditStep {
    pub fn new(name: &str, build: impl FnOnce(&mut EditBuilder<'_>)) -> Self {
        let mut step = Self {
            name: name.to_string(),
            ops: Vec::new(),
            expected: Vec::new(),
        };
        build(&mut EditBuilder { step: &mut step });
        step
    }
}

#[derive(Debug, Clone)]
pub enum EditOp {
    DeleteMethod(MethodTarget),
    RestoreMethod(MethodTarget),
    DeleteConstant(String),
    RestoreConstant(String),
    DeleteNamespace(String),
    RestoreNamespace(String),
    RemoveInclude { owner: String, included: String },
    AddInclude { owner: String, included: String },
    RemovePrepend { owner: String, prepended: String },
    AddPrepend { owner: String, prepended: String },
    ChangeSuperclass { owner: String, superclass: String },
    ClearSuperclass { owner: String },
}

#[derive(Debug, Clone)]
pub enum ExpectedCheck {
    UnresolvedMethod {
        file: String,
        method: String,
    },
    NoUnresolvedMethod {
        file: String,
        method: String,
    },
    UnresolvedConstant {
        file: String,
        constant: String,
    },
    NoUnresolvedConstant {
        file: String,
        constant: String,
    },
    NoMethodDefinitionTarget {
        call_target: MethodTarget,
        stale_target: MethodTarget,
    },
    NoConstantDefinitionTarget {
        ref_target: String,
        stale_target: String,
    },
}

pub struct EditBuilder<'a> {
    step: &'a mut EditStep,
}

impl EditBuilder<'_> {
    pub fn delete_method(&mut self, target: &str) -> &mut Self {
        self.step
            .ops
            .push(EditOp::DeleteMethod(MethodTarget::parse(target)));
        self
    }

    pub fn restore_method(&mut self, target: &str) -> &mut Self {
        self.step
            .ops
            .push(EditOp::RestoreMethod(MethodTarget::parse(target)));
        self
    }

    pub fn delete_constant(&mut self, fqn: &str) -> &mut Self {
        self.step.ops.push(EditOp::DeleteConstant(fqn.to_string()));
        self
    }

    pub fn restore_constant(&mut self, fqn: &str) -> &mut Self {
        self.step.ops.push(EditOp::RestoreConstant(fqn.to_string()));
        self
    }

    pub fn delete_namespace(&mut self, fqn: &str) -> &mut Self {
        self.step.ops.push(EditOp::DeleteNamespace(fqn.to_string()));
        self
    }

    pub fn restore_namespace(&mut self, fqn: &str) -> &mut Self {
        self.step
            .ops
            .push(EditOp::RestoreNamespace(fqn.to_string()));
        self
    }

    pub fn remove_include(&mut self, owner: &str, included: &str) -> &mut Self {
        self.step.ops.push(EditOp::RemoveInclude {
            owner: owner.to_string(),
            included: included.to_string(),
        });
        self
    }

    pub fn add_include(&mut self, owner: &str, included: &str) -> &mut Self {
        self.step.ops.push(EditOp::AddInclude {
            owner: owner.to_string(),
            included: included.to_string(),
        });
        self
    }

    pub fn remove_prepend(&mut self, owner: &str, prepended: &str) -> &mut Self {
        self.step.ops.push(EditOp::RemovePrepend {
            owner: owner.to_string(),
            prepended: prepended.to_string(),
        });
        self
    }

    pub fn add_prepend(&mut self, owner: &str, prepended: &str) -> &mut Self {
        self.step.ops.push(EditOp::AddPrepend {
            owner: owner.to_string(),
            prepended: prepended.to_string(),
        });
        self
    }

    pub fn change_superclass(&mut self, owner: &str, superclass: &str) -> &mut Self {
        self.step.ops.push(EditOp::ChangeSuperclass {
            owner: owner.to_string(),
            superclass: superclass.to_string(),
        });
        self
    }

    pub fn clear_superclass(&mut self, owner: &str) -> &mut Self {
        self.step.ops.push(EditOp::ClearSuperclass {
            owner: owner.to_string(),
        });
        self
    }

    pub fn expect_unresolved_method(&mut self, file: &str, method: &str) -> &mut Self {
        self.step.expected.push(ExpectedCheck::UnresolvedMethod {
            file: file.to_string(),
            method: method.to_string(),
        });
        self
    }

    pub fn expect_no_unresolved_method(&mut self, file: &str, method: &str) -> &mut Self {
        self.step.expected.push(ExpectedCheck::NoUnresolvedMethod {
            file: file.to_string(),
            method: method.to_string(),
        });
        self
    }

    pub fn expect_unresolved_constant(&mut self, file: &str, constant: &str) -> &mut Self {
        self.step.expected.push(ExpectedCheck::UnresolvedConstant {
            file: file.to_string(),
            constant: constant.to_string(),
        });
        self
    }

    pub fn expect_no_unresolved_constant(&mut self, file: &str, constant: &str) -> &mut Self {
        self.step
            .expected
            .push(ExpectedCheck::NoUnresolvedConstant {
                file: file.to_string(),
                constant: constant.to_string(),
            });
        self
    }

    pub fn expect_no_method_definition_target(
        &mut self,
        call_target: &str,
        stale_target: &str,
    ) -> &mut Self {
        self.step
            .expected
            .push(ExpectedCheck::NoMethodDefinitionTarget {
                call_target: MethodTarget::parse(call_target),
                stale_target: MethodTarget::parse(stale_target),
            });
        self
    }

    pub fn expect_no_constant_definition_target(
        &mut self,
        ref_target: &str,
        stale_target: &str,
    ) -> &mut Self {
        self.step
            .expected
            .push(ExpectedCheck::NoConstantDefinitionTarget {
                ref_target: ref_target.to_string(),
                stale_target: stale_target.to_string(),
            });
        self
    }
}
