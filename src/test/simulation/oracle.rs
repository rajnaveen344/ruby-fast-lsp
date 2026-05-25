use super::graph::{
    CallShape, MethodKind, MethodTarget, MethodVisibility, NamespaceKind, NamespaceSpec,
};
use super::project::SyntheticProject;
use super::ruby_gen::{namespace_file, CallSite, ConstantRefSite, SourceMap};
use std::collections::BTreeSet;

pub struct OracleState<'a> {
    project: &'a SyntheticProject,
    source_map: &'a SourceMap,
    indexed_files: BTreeSet<String>,
}

struct EffectiveVisibility {
    visibility: MethodVisibility,
    owner: String,
}

impl<'a> OracleState<'a> {
    pub fn all_files(project: &'a SyntheticProject, source_map: &'a SourceMap) -> Self {
        let indexed_files = source_map.files.iter().cloned().collect();
        Self {
            project,
            source_map,
            indexed_files,
        }
    }

    pub fn with_indexed_files(
        project: &'a SyntheticProject,
        source_map: &'a SourceMap,
        indexed_files: BTreeSet<String>,
    ) -> Self {
        Self {
            project,
            source_map,
            indexed_files,
        }
    }

    pub fn resolve_call(&self, call: &CallSite) -> Option<MethodTarget> {
        match &call.shape {
            CallShape::Bare
            | CallShape::BareInDoBlock
            | CallShape::BareInBraceBlock
            | CallShape::BareInLambda
            | CallShape::BareInProc => {
                self.resolve_instance_method(&call.caller.owner, &call.target.name)
            }
            CallShape::Super => self.resolve_super_method(&call.caller.owner, &call.target.name),
            CallShape::LocalVar { .. }
            | CallShape::Ivar { .. }
            | CallShape::ConstructorSend
            | CallShape::ArrayBlockParam { .. }
            | CallShape::YieldBlockParam { .. } => {
                self.resolve_public_instance_method(&call.target.owner, &call.target.name)
            }
            CallShape::StaticSend => {
                self.resolve_instance_method(&call.target.owner, &call.target.name)
            }
            CallShape::ClassSend => {
                self.resolve_class_method(&call.target.owner, &call.target.name)
            }
            CallShape::MethodObject => match call.target.kind {
                MethodKind::Class => {
                    self.resolve_class_method(&call.target.owner, &call.target.name)
                }
                MethodKind::Instance => {
                    self.resolve_instance_method(&call.caller.owner, &call.target.name)
                }
            },
            CallShape::InstanceMethodObject => {
                self.resolve_instance_method(&call.target.owner, &call.target.name)
            }
            CallShape::ClassReceiver { receiver_owner } => {
                self.resolve_class_method(receiver_owner, &call.target.name)
            }
            CallShape::OneHopChain {
                receiver_owner,
                hop_method,
                ..
            } => {
                let hop = self.resolve_instance_method(receiver_owner, hop_method)?;
                let return_type = self.project.method_return_type(&hop)?;
                self.resolve_public_instance_method(return_type, &call.target.name)
            }
            CallShape::ReceiverLocalVar { receiver_owner, .. } => self
                .resolve_protected_instance_method(
                    receiver_owner,
                    &call.target.name,
                    &call.caller.owner,
                ),
        }
    }

    pub fn resolve_instance_method(
        &self,
        receiver_owner: &str,
        name: &str,
    ) -> Option<MethodTarget> {
        let mut seen = BTreeSet::new();
        if let Some(target) =
            self.resolve_instance_method_inner(receiver_owner, name, true, &mut seen)
        {
            return Some(target);
        }
        if name == "method_missing" {
            return None;
        }
        let mut seen = BTreeSet::new();
        self.resolve_instance_method_inner(receiver_owner, "method_missing", true, &mut seen)
    }

    pub fn resolve_public_instance_method(
        &self,
        receiver_owner: &str,
        name: &str,
    ) -> Option<MethodTarget> {
        let mut seen = BTreeSet::new();
        if let Some(target) =
            self.resolve_instance_method_inner(receiver_owner, name, true, &mut seen)
        {
            let effective = self.effective_instance_visibility(receiver_owner, &target);
            return match effective.visibility {
                MethodVisibility::Public => Some(target),
                MethodVisibility::Protected | MethodVisibility::Private => None,
            };
        }
        if name == "method_missing" {
            return None;
        }
        let mut seen = BTreeSet::new();
        let target =
            self.resolve_instance_method_inner(receiver_owner, "method_missing", true, &mut seen)?;
        let effective = self.effective_instance_visibility(receiver_owner, &target);
        match effective.visibility {
            MethodVisibility::Public => Some(target),
            MethodVisibility::Protected | MethodVisibility::Private => None,
        }
    }

    pub fn resolve_protected_instance_method(
        &self,
        receiver_owner: &str,
        name: &str,
        caller_owner: &str,
    ) -> Option<MethodTarget> {
        let mut seen = BTreeSet::new();
        let target = self.resolve_instance_method_inner(receiver_owner, name, true, &mut seen)?;
        let effective = self.effective_instance_visibility(receiver_owner, &target);
        match effective.visibility {
            MethodVisibility::Public => Some(target),
            MethodVisibility::Protected
                if self.protected_visible_from(&effective.owner, caller_owner) =>
            {
                Some(target)
            }
            MethodVisibility::Protected | MethodVisibility::Private => None,
        }
    }

    fn protected_visible_from(&self, protected_owner: &str, caller_owner: &str) -> bool {
        if protected_owner == caller_owner {
            return true;
        }
        let Some(caller) = self.visible_namespace(caller_owner) else {
            return false;
        };
        caller
            .superclass
            .as_ref()
            .is_some_and(|superclass| self.protected_visible_from(protected_owner, superclass))
    }

    pub fn resolve_super_method(&self, owner: &str, name: &str) -> Option<MethodTarget> {
        let namespace = self.visible_namespace(owner)?;
        for include in namespace.includes.iter().rev() {
            if let Some(target) = self.resolve_instance_method(&include.fqn, name) {
                return Some(target);
            }
        }
        namespace
            .superclass
            .as_ref()
            .and_then(|superclass| self.resolve_instance_method(superclass, name))
    }

    pub fn resolve_class_method(&self, owner: &str, name: &str) -> Option<MethodTarget> {
        let mut seen = BTreeSet::new();
        if let Some(target) = self.resolve_class_method_inner(owner, name, &mut seen) {
            return Some(target);
        }
        if name == "method_missing" {
            return None;
        }
        let mut seen = BTreeSet::new();
        self.resolve_class_method_inner(owner, "method_missing", &mut seen)
    }

    fn resolve_class_method_inner(
        &self,
        owner: &str,
        name: &str,
        seen: &mut BTreeSet<String>,
    ) -> Option<MethodTarget> {
        if !seen.insert(owner.to_string()) {
            return None;
        }

        let namespace = self.visible_namespace(owner)?;
        for prepend in namespace
            .singleton_prepends
            .iter()
            .rev()
            .filter(|prepend| prepend.enabled)
        {
            if let Some(target) = self.resolve_instance_method(&prepend.fqn, name) {
                return Some(target);
            }
        }

        if let Some(method) = namespace
            .methods
            .iter()
            .find(|method| {
                method.enabled
                    && method.name == name
                    && (method.kind == MethodKind::Class
                        || method.def_form == super::graph::MethodDefForm::ModuleFunctionMode)
            })
            .map(|method| MethodTarget {
                owner: owner.to_string(),
                name: method.name.clone(),
                kind: method.kind,
            })
        {
            return Some(method);
        }

        if let Some(alias) = namespace
            .aliases
            .iter()
            .find(|alias| {
                alias.enabled && alias.new_name == name && alias.kind == MethodKind::Class
            })
            .map(|alias| MethodTarget {
                owner: owner.to_string(),
                name: alias.new_name.clone(),
                kind: MethodKind::Class,
            })
        {
            return Some(alias);
        }

        if let Some(attribute) = namespace
            .class_attributes
            .iter()
            .find(|attribute| attribute.enabled && attribute.name == name)
            .map(|attribute| MethodTarget {
                owner: owner.to_string(),
                name: attribute.name.clone(),
                kind: MethodKind::Class,
            })
        {
            return Some(attribute);
        }

        if let Some(delegate) = namespace
            .delegates
            .iter()
            .find(|delegate| {
                delegate.enabled && delegate.new_name == name && delegate.kind == MethodKind::Class
            })
            .map(|delegate| MethodTarget {
                owner: owner.to_string(),
                name: delegate.new_name.clone(),
                kind: MethodKind::Class,
            })
        {
            return Some(delegate);
        }

        if namespace.extend_self {
            let mut instance_seen = BTreeSet::new();
            if let Some(target) =
                self.resolve_instance_method_inner(owner, name, true, &mut instance_seen)
            {
                return Some(target);
            }
        }

        for include in namespace
            .singleton_includes
            .iter()
            .rev()
            .filter(|include| include.enabled)
        {
            if let Some(target) = self.resolve_instance_method(&include.fqn, name) {
                return Some(target);
            }
        }

        for extend in namespace
            .extends
            .iter()
            .rev()
            .filter(|extend| extend.enabled)
        {
            if let Some(target) = self.resolve_instance_method(&extend.fqn, name) {
                return Some(target);
            }
        }

        for include in namespace
            .includes
            .iter()
            .rev()
            .chain(namespace.prepends.iter().rev())
            .filter(|include| include.enabled)
        {
            let Some(included_namespace) = self.visible_namespace(&include.fqn) else {
                continue;
            };
            for hook_extend in included_namespace
                .included_hook_extends
                .iter()
                .chain(included_namespace.concern_class_methods.iter())
                .rev()
                .filter(|extend| extend.enabled)
            {
                if let Some(target) = self.resolve_instance_method(&hook_extend.fqn, name) {
                    return Some(target);
                }
            }
        }

        if namespace.kind == NamespaceKind::Class {
            if let Some(superclass) = &namespace.superclass {
                return self.resolve_class_method_inner(superclass, name, seen);
            }
        }

        None
    }

    pub fn resolve_constant_ref(&self, constant_ref: &ConstantRefSite) -> Option<String> {
        self.resolve_constant_text(&constant_ref.caller.owner, &constant_ref.text)
    }

    pub fn resolve_constant_text(&self, context: &str, text: &str) -> Option<String> {
        let text = text.strip_prefix("::").unwrap_or(text);
        if text.contains("::") {
            return self.resolve_qualified_constant(context, text);
        }
        self.resolve_unqualified_constant(context, text)
    }

    fn resolve_instance_method_inner(
        &self,
        owner: &str,
        name: &str,
        allow_private: bool,
        seen: &mut BTreeSet<String>,
    ) -> Option<MethodTarget> {
        if !seen.insert(owner.to_string()) {
            return None;
        }

        let namespaces = self.visible_namespaces(owner);
        if namespaces.is_empty() {
            return None;
        }

        let prepends = namespaces
            .iter()
            .flat_map(|namespace| namespace.prepends.iter())
            .collect::<Vec<_>>();
        for prepend in prepends.into_iter().rev().filter(|prepend| prepend.enabled) {
            if let Some(target) =
                self.resolve_instance_method_inner(&prepend.fqn, name, allow_private, seen)
            {
                return Some(target);
            }
        }

        if let Some(target) = namespaces
            .iter()
            .rev()
            .find_map(|namespace| self.own_instance_method(namespace, name, allow_private))
        {
            return Some(target);
        }

        let includes = namespaces
            .iter()
            .flat_map(|namespace| namespace.includes.iter())
            .collect::<Vec<_>>();
        for include in includes.into_iter().rev().filter(|include| include.enabled) {
            if let Some(target) =
                self.resolve_instance_method_inner(&include.fqn, name, allow_private, seen)
            {
                return Some(target);
            }
        }

        let hook_sources = namespaces
            .iter()
            .flat_map(|namespace| namespace.includes.iter().chain(namespace.prepends.iter()))
            .collect::<Vec<_>>();
        for include in hook_sources
            .into_iter()
            .rev()
            .filter(|include| include.enabled)
        {
            for included_namespace in self.visible_namespaces(&include.fqn) {
                for hook_include in included_namespace
                    .included_hook_includes
                    .iter()
                    .chain(included_namespace.included_hook_class_eval_includes.iter())
                    .rev()
                    .filter(|hook_include| hook_include.enabled)
                {
                    if let Some(target) = self.resolve_instance_method_inner(
                        &hook_include.fqn,
                        name,
                        allow_private,
                        seen,
                    ) {
                        return Some(target);
                    }
                }
            }
        }

        if namespaces
            .iter()
            .any(|namespace| namespace.kind == NamespaceKind::Class)
        {
            if let Some(superclass) = namespaces
                .iter()
                .rev()
                .find_map(|namespace| namespace.superclass.as_ref())
            {
                return self.resolve_instance_method_inner(superclass, name, allow_private, seen);
            }
        }

        None
    }

    fn own_instance_method(
        &self,
        namespace: &NamespaceSpec,
        name: &str,
        allow_private: bool,
    ) -> Option<MethodTarget> {
        namespace
            .methods
            .iter()
            .find(|method| {
                method.enabled
                    && method.name == name
                    && method.kind == MethodKind::Instance
                    && (allow_private || method.visibility == MethodVisibility::Public)
            })
            .map(|method| MethodTarget {
                owner: namespace.fqn.clone(),
                name: method.name.clone(),
                kind: MethodKind::Instance,
            })
            .or_else(|| {
                namespace
                    .aliases
                    .iter()
                    .find(|alias| {
                        alias.enabled
                            && alias.new_name == name
                            && alias.kind == MethodKind::Instance
                    })
                    .map(|alias| MethodTarget {
                        owner: namespace.fqn.clone(),
                        name: alias.new_name.clone(),
                        kind: MethodKind::Instance,
                    })
            })
            .or_else(|| {
                namespace
                    .delegates
                    .iter()
                    .find(|delegate| {
                        delegate.enabled
                            && delegate.new_name == name
                            && delegate.kind == MethodKind::Instance
                    })
                    .map(|delegate| MethodTarget {
                        owner: namespace.fqn.clone(),
                        name: delegate.new_name.clone(),
                        kind: MethodKind::Instance,
                    })
            })
            .or_else(|| {
                namespace
                    .class_attributes
                    .iter()
                    .find(|attribute| attribute.enabled && attribute.name == name)
                    .map(|attribute| MethodTarget {
                        owner: namespace.fqn.clone(),
                        name: attribute.name.clone(),
                        kind: MethodKind::Instance,
                    })
            })
    }

    fn effective_instance_visibility(
        &self,
        receiver_owner: &str,
        target: &MethodTarget,
    ) -> EffectiveVisibility {
        self.visible_namespaces(receiver_owner)
            .into_iter()
            .rev()
            .find_map(|namespace| {
                namespace
                    .visibility_overrides
                    .iter()
                    .find(|visibility_override| {
                        visibility_override.enabled && visibility_override.name == target.name
                    })
                    .map(|visibility_override| EffectiveVisibility {
                        visibility: visibility_override.visibility,
                        owner: namespace.fqn.clone(),
                    })
            })
            .unwrap_or_else(|| EffectiveVisibility {
                visibility: self.project.method_visibility(target),
                owner: target.owner.clone(),
            })
    }

    fn resolve_unqualified_constant(&self, context: &str, name: &str) -> Option<String> {
        for namespace in namespace_search_path(context) {
            let candidate = if namespace.is_empty() {
                name.to_string()
            } else {
                format!("{}::{}", namespace, name)
            };
            if self.visible_constant(&candidate) {
                return Some(candidate);
            }
        }
        None
    }

    fn resolve_qualified_constant(&self, context: &str, path: &str) -> Option<String> {
        for namespace in namespace_search_path(context) {
            let candidate = if namespace.is_empty() {
                path.to_string()
            } else {
                format!("{}::{}", namespace, path)
            };
            if self.visible_constant(&candidate) {
                return Some(candidate);
            }
        }
        None
    }

    fn visible_constant(&self, fqn: &str) -> bool {
        let Some(pos) = self.source_map.constants.get(fqn) else {
            return false;
        };
        self.indexed_files.contains(&pos.file) && self.project.constant_enabled(fqn)
    }

    fn visible_namespace(&self, fqn: &str) -> Option<&NamespaceSpec> {
        self.visible_namespaces(fqn).into_iter().next()
    }

    fn visible_namespaces(&self, fqn: &str) -> Vec<&NamespaceSpec> {
        self.project
            .namespaces
            .iter()
            .filter(|namespace| namespace.enabled && namespace.fqn == fqn)
            .filter(|namespace| {
                let file = namespace_file(namespace);
                self.indexed_files.contains(&file)
                    || self
                        .inline_concern_namespace_file(&namespace.fqn)
                        .is_some_and(|file| self.indexed_files.contains(file))
            })
            .collect()
    }

    fn inline_concern_namespace_file(&self, fqn: &str) -> Option<&str> {
        self.project.namespaces.iter().find_map(|namespace| {
            let contains_inline = namespace
                .concern_class_methods
                .iter()
                .any(|class_methods| class_methods.enabled && class_methods.fqn == fqn);
            if contains_inline {
                self.source_map
                    .namespaces
                    .get(&namespace.fqn)
                    .map(|site| site.pos.file.as_str())
            } else {
                None
            }
        })
    }
}

fn namespace_search_path(context: &str) -> Vec<String> {
    let mut path = Vec::new();
    let mut current = Some(context);
    while let Some(namespace) = current {
        path.push(namespace.to_string());
        current = namespace.rsplit_once("::").map(|(parent, _)| parent);
    }
    path.push(String::new());
    path
}
