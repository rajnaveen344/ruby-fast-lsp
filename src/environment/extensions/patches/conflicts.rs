use std::collections::BTreeSet;

use ruby_analysis::core::NamespaceKind;
use ruby_fast_lsp_extension_api::{
    BlockExecutionContextPatch, ExecutionContextTarget, IndexPatch,
    NamespaceKind as AbiNamespaceKind,
};

use crate::environment::extensions::patches::types::index_patch_payload_eq;
use crate::environment::extensions::patches::validation::index_patch_extension_id;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum IndexPatchIdentity {
    Declaration {
        path: Vec<String>,
    },
    Reference {
        start_line: u32,
        start_character: u32,
        end_line: u32,
        end_character: u32,
    },
    Method {
        namespace: Vec<String>,
        owner_kind: String,
        name: String,
    },
    Superclass {
        namespace: Vec<String>,
    },
    Mixin {
        namespace: Vec<String>,
        target_kind: String,
        mixin: Vec<String>,
        kind: String,
    },
    ExecutionContextConnection {
        template: Vec<String>,
        application: Vec<String>,
        start_line: u32,
        start_character: u32,
        end_line: u32,
        end_character: u32,
    },
}

impl IndexPatchIdentity {
    fn display(&self) -> String {
        match self {
            Self::Declaration { path } => path.join("::"),
            Self::Reference {
                start_line,
                start_character,
                end_line,
                end_character,
            } => format!("reference at {start_line}:{start_character}-{end_line}:{end_character}"),
            Self::Method {
                namespace,
                owner_kind,
                name,
            } => {
                let separator = if owner_kind == "singleton" { "." } else { "#" };
                format!("{}{separator}{name}", namespace.join("::"))
            }
            Self::Superclass { namespace } => {
                format!("{} superclass", namespace.join("::"))
            }
            Self::Mixin {
                namespace,
                target_kind,
                mixin,
                kind,
            } => format!(
                "{} ({target_kind}) {kind} {}",
                namespace.join("::"),
                mixin.join("::")
            ),
            Self::ExecutionContextConnection {
                template,
                application,
                start_line,
                start_character,
                end_line,
                end_character,
            } => format!(
                "execution context {} -> {} at {start_line}:{start_character}-{end_line}:{end_character}",
                template.join("::"),
                application.join("::")
            ),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(in crate::environment::extensions) struct IndexPatchConflict {
    pub(in crate::environment::extensions) extension_ids: Vec<String>,
    pub(in crate::environment::extensions) message: String,
}

pub(in crate::environment::extensions) fn resolve_index_patch_conflicts(
    mut patches: Vec<IndexPatch>,
) -> Result<Vec<IndexPatch>, IndexPatchConflict> {
    patches.sort_by(|left, right| {
        index_patch_identity(left)
            .cmp(&index_patch_identity(right))
            .then_with(|| index_patch_extension_id(left).cmp(index_patch_extension_id(right)))
    });
    let mut resolved = Vec::new();
    let mut index = 0;
    while index < patches.len() {
        let identity = index_patch_identity(&patches[index]);
        let mut end = index + 1;
        while end < patches.len() && index_patch_identity(&patches[end]) == identity {
            end += 1;
        }
        let group = &patches[index..end];
        if group
            .iter()
            .skip(1)
            .any(|patch| !index_patch_payload_eq(&group[0], patch))
        {
            let extension_ids = group
                .iter()
                .map(index_patch_extension_id)
                .map(str::to_string)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            return Err(IndexPatchConflict {
                message: format!(
                    "extensions {} emitted incompatible index patches for `{}`; conflicting semantic facts are rejected deterministically",
                    extension_ids.join(", "),
                    identity.display()
                ),
                extension_ids,
            });
        }
        resolved.push(group[0].clone());
        index = end;
    }
    Ok(resolved)
}

pub(in crate::environment::extensions) fn resolve_execution_context_conflicts(
    mut contexts: Vec<BlockExecutionContextPatch>,
) -> Result<Vec<BlockExecutionContextPatch>, IndexPatchConflict> {
    contexts.sort_by(|left, right| {
        execution_context_range_key(left)
            .cmp(&execution_context_range_key(right))
            .then_with(|| left.source.extension_id.cmp(&right.source.extension_id))
    });
    let mut resolved = Vec::new();
    let mut index = 0;
    while index < contexts.len() {
        let identity = execution_context_range_key(&contexts[index]);
        let mut end = index + 1;
        while end < contexts.len() && execution_context_range_key(&contexts[end]) == identity {
            end += 1;
        }
        let group = &contexts[index..end];
        if group
            .iter()
            .skip(1)
            .any(|context| !execution_context_payload_eq(&group[0], context))
        {
            let extension_ids = group
                .iter()
                .map(|context| context.source.extension_id.clone())
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>();
            return Err(IndexPatchConflict {
                message: format!(
                    "extensions {} emitted incompatible block execution contexts for call at {}:{}-{}:{}; conflicting runtime ownership is rejected deterministically",
                    extension_ids.join(", "),
                    identity.0,
                    identity.1,
                    identity.2,
                    identity.3
                ),
                extension_ids,
            });
        }
        resolved.push(group[0].clone());
        index = end;
    }
    Ok(resolved)
}

fn execution_context_range_key(context: &BlockExecutionContextPatch) -> (u32, u32, u32, u32) {
    (
        context.call_range.start.line,
        context.call_range.start.character,
        context.call_range.end.line,
        context.call_range.end.character,
    )
}

fn execution_context_payload_eq(
    left: &BlockExecutionContextPatch,
    right: &BlockExecutionContextPatch,
) -> bool {
    left.call_range == right.call_range
        && left.block_range == right.block_range
        && left.generated_owners == right.generated_owners
        && left.implicit_receiver == right.implicit_receiver
        && left.method_definition_owner == right.method_definition_owner
        && left.lexical_scope == right.lexical_scope
        && left.local_scope == right.local_scope
}

fn index_patch_identity(patch: &IndexPatch) -> IndexPatchIdentity {
    match patch {
        IndexPatch::DefineNamespace(namespace) => IndexPatchIdentity::Declaration {
            path: namespace.namespace.clone(),
        },
        IndexPatch::DefineConstant(constant) => {
            let mut path = constant.namespace.clone();
            path.push(constant.name.clone());
            IndexPatchIdentity::Declaration { path }
        }
        IndexPatch::AddReference(reference) => IndexPatchIdentity::Reference {
            start_line: reference.location.start.line,
            start_character: reference.location.start.character,
            end_line: reference.location.end.line,
            end_character: reference.location.end.character,
        },
        IndexPatch::DefineMethod(method) => IndexPatchIdentity::Method {
            namespace: patch_owner_identity(&method.namespace, method.owner_target.as_ref()),
            owner_kind: namespace_kind_name(method.owner_kind).to_string(),
            name: method.name.clone(),
        },
        IndexPatch::SetSuperclass(superclass) => IndexPatchIdentity::Superclass {
            namespace: superclass.namespace.clone(),
        },
        IndexPatch::ApplyMixin(mixin) => IndexPatchIdentity::Mixin {
            namespace: patch_owner_identity(&mixin.namespace, mixin.owner_target.as_ref()),
            target_kind: namespace_kind_name(mixin.target_kind).to_string(),
            mixin: mixin
                .mixin_target
                .as_ref()
                .map(|target| patch_owner_identity(&[], Some(target)))
                .unwrap_or_else(|| mixin.mixin.clone()),
            kind: match mixin.kind {
                ruby_fast_lsp_extension_api::MixinKind::Include => "include",
                ruby_fast_lsp_extension_api::MixinKind::Prepend => "prepend",
                ruby_fast_lsp_extension_api::MixinKind::Extend => "extend",
            }
            .to_string(),
        },
        IndexPatch::ConnectExecutionContext(connection) => {
            IndexPatchIdentity::ExecutionContextConnection {
                template: patch_owner_identity(&[], Some(&connection.template)),
                application: patch_owner_identity(&[], Some(&connection.application)),
                start_line: connection.location.start.line,
                start_character: connection.location.start.character,
                end_line: connection.location.end.line,
                end_character: connection.location.end.character,
            }
        }
    }
}

fn patch_owner_identity(
    namespace: &[String],
    target: Option<&ExecutionContextTarget>,
) -> Vec<String> {
    match target {
        None => namespace.to_vec(),
        Some(ExecutionContextTarget::Namespace {
            namespace,
            owner_kind,
        }) => {
            let mut identity = vec![format!("@namespace:{}", namespace_kind_name(*owner_kind))];
            identity.extend(namespace.iter().cloned());
            identity
        }
        Some(ExecutionContextTarget::GeneratedOwner {
            local_id,
            owner_kind,
        }) => {
            vec![format!(
                "@generated:{local_id}:{}",
                owner_kind.map(namespace_kind_name).unwrap_or("fallback")
            )]
        }
        Some(ExecutionContextTarget::ProjectGeneratedOwner {
            local_id,
            owner_kind,
        }) => {
            vec![format!(
                "@project-generated:{local_id}:{}",
                owner_kind.map(namespace_kind_name).unwrap_or("fallback")
            )]
        }
    }
}

fn namespace_kind_name(kind: AbiNamespaceKind) -> &'static str {
    match kind {
        AbiNamespaceKind::Instance => "instance",
        AbiNamespaceKind::Singleton => "singleton",
    }
}

pub(in crate::environment::extensions) fn namespace_kind_from_abi(
    kind: AbiNamespaceKind,
) -> NamespaceKind {
    match kind {
        AbiNamespaceKind::Instance => NamespaceKind::Instance,
        AbiNamespaceKind::Singleton => NamespaceKind::Singleton,
    }
}
