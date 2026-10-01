use std::collections::BTreeSet;

use ruby_analysis::core::{GeneratedOwnerId, RubyConstant, RubyMethod};
use ruby_fast_lsp_extension_api::{
    BlockExecutionContextPatch, CallContext, ExecutionContextTarget, GeneratedOwnerScope,
    IndexPatch, ResponsePatch, SourceRange,
};

use crate::extensions::patches::types::analysis_ruby_type_from_extension;

pub(in crate::extensions) fn index_patch_extension_id(patch: &IndexPatch) -> &str {
    match patch {
        IndexPatch::DefineNamespace(namespace) => &namespace.source.extension_id,
        IndexPatch::DefineConstant(constant) => &constant.source.extension_id,
        IndexPatch::AddReference(reference) => &reference.source.extension_id,
        IndexPatch::DefineMethod(method) => &method.source.extension_id,
        IndexPatch::SetSuperclass(superclass) => &superclass.source.extension_id,
        IndexPatch::ApplyMixin(mixin) => &mixin.source.extension_id,
        IndexPatch::ConnectExecutionContext(connection) => &connection.source.extension_id,
    }
}

pub(in crate::extensions) fn validate_index_patch_provenance(
    expected_extension_id: &str,
    patches: &[IndexPatch],
) -> Result<(), String> {
    if let Some(spoofed_id) = patches.iter().find_map(|patch| {
        let source_id = index_patch_extension_id(patch);
        (source_id != expected_extension_id).then(|| source_id.to_string())
    }) {
        return Err(spoofed_id);
    }
    Ok(())
}

fn response_patch_extension_id(patch: &ResponsePatch) -> &str {
    match patch {
        ResponsePatch::Diagnostic(diagnostic) => &diagnostic.source.extension_id,
        ResponsePatch::CodeLens(lens) => &lens.source.extension_id,
        ResponsePatch::DocumentSymbol(symbol) => &symbol.source.extension_id,
    }
}

pub(in crate::extensions) fn validate_response_patch_provenance(
    expected_extension_id: &str,
    patches: &[ResponsePatch],
) -> Result<(), String> {
    if let Some(spoofed_id) = patches.iter().find_map(|patch| {
        let source_id = response_patch_extension_id(patch);
        (source_id != expected_extension_id).then(|| source_id.to_string())
    }) {
        return Err(spoofed_id);
    }
    Ok(())
}

pub(in crate::extensions) fn validate_index_patch_payloads(
    patches: &[IndexPatch],
) -> Result<(), String> {
    for patch in patches {
        match patch {
            IndexPatch::DefineNamespace(namespace) => {
                if namespace.namespace.is_empty() {
                    return Err("namespace declaration must not be empty".to_string());
                }
                validate_extension_namespace(&namespace.namespace, "namespace declaration")?;
                validate_source_range(namespace.location, "namespace location")?;
            }
            IndexPatch::DefineConstant(constant) => {
                validate_extension_namespace(&constant.namespace, "constant namespace")?;
                RubyConstant::new(&constant.name)
                    .map_err(|err| format!("invalid constant name `{}`: {err}", constant.name))?;
                validate_source_range(constant.location, "constant location")?;
                analysis_ruby_type_from_extension(constant.ruby_type.as_ref())?;
            }
            IndexPatch::AddReference(reference) => {
                validate_reference_target(&reference.target)?;
                validate_source_range(reference.location, "reference location")?;
            }
            IndexPatch::DefineMethod(method) => {
                RubyMethod::new(&method.name)
                    .map_err(|err| format!("invalid method name `{}`: {err}", method.name))?;
                validate_extension_namespace(&method.namespace, "method namespace")?;
                if let Some(target) = &method.owner_target {
                    validate_patch_owner_target(
                        target,
                        &method.source.extension_id,
                        "method owner",
                    )?;
                }
                validate_source_range(method.location, "method location")?;
                if method.params.iter().any(|param| param.name.is_empty()) {
                    return Err("method parameter names must not be empty".to_string());
                }
                if method.return_type.is_some() && method.return_type_source.is_some() {
                    return Err(
                        "method patch must use either `return_type` or `return_type_source`, not both"
                            .to_string(),
                    );
                }
                analysis_ruby_type_from_extension(method.return_type.as_ref())?;
            }
            IndexPatch::SetSuperclass(superclass) => {
                if superclass.namespace.is_empty() {
                    return Err("superclass namespace must not be empty".to_string());
                }
                if superclass.superclass.is_empty() {
                    return Err("superclass target must not be empty".to_string());
                }
                validate_extension_namespace(&superclass.namespace, "superclass namespace")?;
                validate_extension_namespace(&superclass.superclass, "superclass target")?;
                validate_source_range(superclass.location, "superclass location")?;
            }
            IndexPatch::ApplyMixin(mixin) => {
                validate_extension_namespace(&mixin.namespace, "mixin namespace")?;
                if let Some(target) = &mixin.owner_target {
                    validate_patch_owner_target(target, &mixin.source.extension_id, "mixin owner")?;
                }
                match &mixin.mixin_target {
                    Some(target) => {
                        if !mixin.mixin.is_empty() {
                            return Err(
                                "mixin patch must use either `mixin` or `mixin_target`, not both"
                                    .to_string(),
                            );
                        }
                        validate_patch_owner_target(
                            target,
                            &mixin.source.extension_id,
                            "semantic mixin target",
                        )?;
                    }
                    None => {
                        if mixin.mixin.is_empty() {
                            return Err(
                                "mixin patch must provide `mixin` or `mixin_target`".to_string()
                            );
                        }
                        validate_extension_namespace(&mixin.mixin, "mixin target")?;
                    }
                }
                validate_source_range(mixin.location, "mixin location")?;
            }
            IndexPatch::ConnectExecutionContext(connection) => {
                validate_patch_owner_target(
                    &connection.template,
                    &connection.source.extension_id,
                    "execution context template",
                )?;
                validate_patch_owner_target(
                    &connection.application,
                    &connection.source.extension_id,
                    "execution context application",
                )?;
                validate_source_range(
                    connection.location,
                    "execution context application location",
                )?;
            }
        }
    }
    for superclass in patches.iter().filter_map(|patch| match patch {
        IndexPatch::SetSuperclass(superclass) => Some(superclass),
        IndexPatch::DefineNamespace(_)
        | IndexPatch::DefineConstant(_)
        | IndexPatch::AddReference(_)
        | IndexPatch::DefineMethod(_)
        | IndexPatch::ApplyMixin(_)
        | IndexPatch::ConnectExecutionContext(_) => None,
    }) {
        let declares_class = patches.iter().any(|patch| match patch {
            IndexPatch::DefineNamespace(namespace) => {
                namespace.namespace == superclass.namespace
                    && namespace.kind
                        == ruby_fast_lsp_extension_api::NamespaceDeclarationKind::Class
                    && namespace.source.extension_id == superclass.source.extension_id
            }
            IndexPatch::DefineConstant(_)
            | IndexPatch::AddReference(_)
            | IndexPatch::DefineMethod(_)
            | IndexPatch::SetSuperclass(_)
            | IndexPatch::ApplyMixin(_)
            | IndexPatch::ConnectExecutionContext(_) => false,
        });
        if !declares_class {
            return Err(format!(
                "superclass patch for `{}` requires a matching generated class declaration from the same extension output",
                superclass.namespace.join("::")
            ));
        }
    }
    Ok(())
}

fn execution_target_requires_project(target: &ExecutionContextTarget) -> bool {
    matches!(target, ExecutionContextTarget::ProjectGeneratedOwner { .. })
}

pub(in crate::extensions) fn index_patch_requires_project_context(patch: &IndexPatch) -> bool {
    match patch {
        IndexPatch::DefineMethod(method) => method
            .owner_target
            .as_ref()
            .is_some_and(execution_target_requires_project),
        IndexPatch::ApplyMixin(mixin) => {
            mixin
                .owner_target
                .as_ref()
                .is_some_and(execution_target_requires_project)
                || mixin
                    .mixin_target
                    .as_ref()
                    .is_some_and(execution_target_requires_project)
        }
        IndexPatch::ConnectExecutionContext(connection) => {
            execution_target_requires_project(&connection.template)
                || execution_target_requires_project(&connection.application)
        }
        IndexPatch::DefineNamespace(_)
        | IndexPatch::DefineConstant(_)
        | IndexPatch::AddReference(_)
        | IndexPatch::SetSuperclass(_) => false,
    }
}

fn validate_patch_owner_target(
    target: &ExecutionContextTarget,
    extension_id: &str,
    label: &str,
) -> Result<(), String> {
    match target {
        ExecutionContextTarget::Namespace { namespace, .. } => {
            if namespace.is_empty() {
                return Err(format!("{label} namespace must not be empty"));
            }
            validate_extension_namespace(namespace, label)
        }
        ExecutionContextTarget::GeneratedOwner { local_id, .. } => {
            GeneratedOwnerId::new(extension_id, "validation-source", local_id)
                .map(|_| ())
                .map_err(|err| format!("invalid {label} generated owner `{local_id}`: {err}"))
        }
        ExecutionContextTarget::ProjectGeneratedOwner { local_id, .. } => {
            GeneratedOwnerId::new(extension_id, "validation-project", local_id)
                .map(|_| ())
                .map_err(|err| {
                    format!("invalid {label} project-generated owner `{local_id}`: {err}")
                })
        }
    }
}

pub(in crate::extensions) fn validate_execution_contexts(
    expected_extension_id: &str,
    call: &CallContext,
    contexts: &[BlockExecutionContextPatch],
) -> Result<(), String> {
    validate_execution_contexts_for_project(
        expected_extension_id,
        call,
        call.project.is_some(),
        contexts,
    )
}

pub(in crate::extensions) fn validate_execution_contexts_for_project(
    expected_extension_id: &str,
    call: &CallContext,
    project_present: bool,
    contexts: &[BlockExecutionContextPatch],
) -> Result<(), String> {
    if contexts.len() > 1 {
        return Err("an extension may emit at most one execution context for one call".to_string());
    }
    for context in contexts {
        if context.source.extension_id != expected_extension_id {
            return Err(format!(
                "context provenance `{}` does not match loaded manifest id `{expected_extension_id}`",
                context.source.extension_id
            ));
        }
        if context.call_range != call.call_range {
            return Err("context call_range must exactly match the current call".to_string());
        }
        let expected_block = call.block_range.ok_or_else(|| {
            "execution context requires the current call to have a block".to_string()
        })?;
        if context.block_range != expected_block {
            return Err("context block_range must exactly match the current block".to_string());
        }
        validate_source_range(context.call_range, "execution context call range")?;
        validate_source_range(context.block_range, "execution context block range")?;
        let mut declared = BTreeSet::new();
        for owner in &context.generated_owners {
            if owner.scope == GeneratedOwnerScope::Project && !project_present {
                return Err(format!(
                    "project-generated owner `{}` requires an owning ProjectContext",
                    owner.local_id
                ));
            }
            GeneratedOwnerId::new(expected_extension_id, "validation-source", &owner.local_id)
                .map_err(|err| format!("invalid generated owner `{}`: {err}", owner.local_id))?;
            if !declared.insert((owner.scope, owner.local_id.clone())) {
                return Err(format!(
                    "{:?} generated owner `{}` is declared more than once",
                    owner.scope, owner.local_id
                ));
            }
        }
        for owner in &context.generated_owners {
            if let Some(parent) = &owner.parent {
                validate_execution_context_target(parent, &declared, "generated owner parent")?;
            }
        }
        validate_execution_context_target(
            &context.implicit_receiver,
            &declared,
            "implicit receiver",
        )?;
        validate_execution_context_target(
            &context.method_definition_owner,
            &declared,
            "method-definition owner",
        )?;
    }
    Ok(())
}

fn validate_execution_context_target(
    target: &ExecutionContextTarget,
    declared: &BTreeSet<(GeneratedOwnerScope, String)>,
    label: &str,
) -> Result<(), String> {
    match target {
        ExecutionContextTarget::Namespace { namespace, .. } => {
            if namespace.is_empty() {
                return Err(format!("{label} namespace must not be empty"));
            }
            validate_extension_namespace(namespace, label)
        }
        ExecutionContextTarget::GeneratedOwner { local_id, .. } => {
            if !declared.contains(&(GeneratedOwnerScope::Source, local_id.clone())) {
                return Err(format!(
                    "{label} references undeclared generated owner `{local_id}`"
                ));
            }
            Ok(())
        }
        ExecutionContextTarget::ProjectGeneratedOwner { local_id, .. } => {
            if !declared.contains(&(GeneratedOwnerScope::Project, local_id.clone())) {
                return Err(format!(
                    "{label} references undeclared project-generated owner `{local_id}`"
                ));
            }
            Ok(())
        }
    }
}

fn validate_extension_namespace(parts: &[String], label: &str) -> Result<(), String> {
    for part in parts {
        RubyConstant::new(part)
            .map_err(|err| format!("invalid {label} component `{part}`: {err}"))?;
    }
    Ok(())
}

fn validate_reference_target(
    target: &ruby_fast_lsp_extension_api::ReferenceTarget,
) -> Result<(), String> {
    match target {
        ruby_fast_lsp_extension_api::ReferenceTarget::Namespace(namespace) => {
            if namespace.is_empty() {
                return Err("reference namespace target must not be empty".to_string());
            }
            validate_extension_namespace(namespace, "reference namespace target")
        }
        ruby_fast_lsp_extension_api::ReferenceTarget::Constant { namespace, name } => {
            validate_extension_namespace(namespace, "reference constant namespace")?;
            RubyConstant::new(name)
                .map(|_| ())
                .map_err(|err| format!("invalid reference constant name `{name}`: {err}"))
        }
        ruby_fast_lsp_extension_api::ReferenceTarget::Method {
            namespace, name, ..
        } => {
            if namespace.is_empty() {
                return Err("reference method namespace must not be empty".to_string());
            }
            validate_extension_namespace(namespace, "reference method namespace")?;
            RubyMethod::new(name)
                .map(|_| ())
                .map_err(|err| format!("invalid reference method name `{name}`: {err}"))
        }
    }
}

fn validate_source_range(range: SourceRange, label: &str) -> Result<(), String> {
    let start = (range.start.line, range.start.character);
    let end = (range.end.line, range.end.character);
    if start > end {
        return Err(format!(
            "{label} start {}:{} is after end {}:{}",
            range.start.line, range.start.character, range.end.line, range.end.character
        ));
    }
    Ok(())
}
