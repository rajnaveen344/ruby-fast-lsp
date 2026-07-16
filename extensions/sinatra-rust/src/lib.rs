use ruby_fast_lsp_extension_api::{
    ApplyMixinPatch, ArgumentValue, BlockExecutionContextPatch, CallContext, CalleeResolution,
    ExecutionContextTarget, Extension, ExtensionOutput, IndexPatch, LexicalScopeMode,
    LocalScopeMode, MixinKind, NamespaceKind, PatchSource, Receiver,
};

pub const EXTENSION_ID: &str = "sinatra-rust";

pub fn extension() -> SinatraExtension {
    SinatraExtension
}

pub struct SinatraExtension;

impl Extension for SinatraExtension {
    fn id(&self) -> &'static str {
        EXTENSION_ID
    }

    fn indexed_call_names(&self) -> &'static [&'static str] {
        &[
            "get",
            "head",
            "post",
            "put",
            "patch",
            "delete",
            "options",
            "link",
            "unlink",
            "before",
            "after",
            "error",
            "not_found",
            "helpers",
        ]
    }

    fn index_call(&self, context: &CallContext) -> Vec<IndexPatch> {
        if context.method_name != "helpers" || !is_sinatra_dsl_call(context) {
            return Vec::new();
        }
        let Some(application) = application_namespace(context) else {
            return Vec::new();
        };
        context
            .arguments
            .iter()
            .filter_map(|argument| match &argument.value {
                ArgumentValue::Constant(mixin) => Some(IndexPatch::ApplyMixin(ApplyMixinPatch {
                    namespace: application.clone(),
                    owner_target: Some(namespace_target(
                        application.clone(),
                        NamespaceKind::Instance,
                    )),
                    target_kind: NamespaceKind::Instance,
                    mixin_target: None,
                    mixin: mixin.clone(),
                    absolute: false,
                    kind: MixinKind::Include,
                    location: argument.range,
                    source: source("helpers"),
                })),
                ArgumentValue::Symbol(_)
                | ArgumentValue::String(_)
                | ArgumentValue::Boolean(_)
                | ArgumentValue::Nil
                | ArgumentValue::Unsupported => None,
            })
            .collect()
    }

    fn index_call_output(&self, context: &CallContext) -> ExtensionOutput {
        let mut output = ExtensionOutput::index_patches(self.index_call(context));
        if !is_sinatra_dsl_call(context) {
            return output;
        }
        let Some(block_range) = context.block_range else {
            return output;
        };
        let Some(application) = application_namespace(context) else {
            return output;
        };

        let (implicit_receiver, method_definition_owner) = if context.method_name == "helpers" {
            (
                namespace_target(application.clone(), NamespaceKind::Singleton),
                namespace_target(application, NamespaceKind::Instance),
            )
        } else if is_request_block(context.method_name.as_str()) {
            (
                namespace_target(application, NamespaceKind::Instance),
                lexical_definition_owner(context),
            )
        } else {
            return output;
        };

        output.execution_contexts.push(BlockExecutionContextPatch {
            call_range: context.call_range,
            block_range,
            generated_owners: Vec::new(),
            implicit_receiver,
            method_definition_owner,
            lexical_scope: LexicalScopeMode::Preserve,
            local_scope: LocalScopeMode::Preserve,
            source: source(context.method_name.as_str()),
        });
        output
    }
}

fn is_request_block(method: &str) -> bool {
    matches!(
        method,
        "get"
            | "head"
            | "post"
            | "put"
            | "patch"
            | "delete"
            | "options"
            | "link"
            | "unlink"
            | "before"
            | "after"
            | "error"
            | "not_found"
    )
}

fn is_sinatra_dsl_call(context: &CallContext) -> bool {
    context.resolved_callees.iter().any(|callee| {
        if callee.method != context.method_name || callee.resolution != CalleeResolution::Exact {
            return false;
        }
        match (callee.owner.as_slice(), callee.owner_kind) {
            ([sinatra, base], NamespaceKind::Singleton) => {
                sinatra == "Sinatra" && (base == "Base" || base == "Application")
            }
            ([sinatra, delegator], NamespaceKind::Instance) => {
                sinatra == "Sinatra" && delegator == "Delegator"
            }
            ([sinatra], NamespaceKind::Singleton) => {
                context.method_name == "helpers" && sinatra == "Sinatra"
            }
            ([], NamespaceKind::Instance | NamespaceKind::Singleton) | ([_, ..], _) => false,
        }
    })
}

fn application_namespace(context: &CallContext) -> Option<Vec<String>> {
    match &context.receiver {
        Receiver::Constant(namespace) if namespace == &["Sinatra".to_string()] => {
            Some(vec!["Sinatra".to_string(), "Application".to_string()])
        }
        Receiver::Constant(namespace) => Some(namespace.clone()),
        Receiver::None | Receiver::SelfReceiver if !context.current_namespace.is_empty() => {
            Some(context.current_namespace.clone())
        }
        Receiver::None | Receiver::SelfReceiver => {
            Some(vec!["Sinatra".to_string(), "Application".to_string()])
        }
        Receiver::LocalVariable(_)
        | Receiver::InstanceVariable(_)
        | Receiver::ClassVariable(_)
        | Receiver::GlobalVariable(_)
        | Receiver::MethodCall { .. }
        | Receiver::Literal
        | Receiver::Expression => None,
    }
}

fn lexical_definition_owner(context: &CallContext) -> ExecutionContextTarget {
    let namespace = if context.current_namespace.is_empty() {
        vec!["Object".to_string()]
    } else {
        context.current_namespace.clone()
    };
    namespace_target(namespace, context.namespace_kind)
}

fn namespace_target(namespace: Vec<String>, owner_kind: NamespaceKind) -> ExecutionContextTarget {
    ExecutionContextTarget::Namespace {
        namespace,
        owner_kind,
    }
}

fn source(macro_name: &str) -> PatchSource {
    PatchSource {
        extension_id: EXTENSION_ID.to_string(),
        macro_name: macro_name.to_string(),
    }
}

#[cfg(target_arch = "wasm32")]
ruby_fast_lsp_extension_guest_sdk::export_extension!(extension);

#[cfg(test)]
mod tests {
    use super::*;
    use ruby_fast_lsp_extension_api::{
        Argument, ArgumentValue, CalleeResolution, ExecutionContextTarget, Extension,
        LexicalScopeMode, LocalScopeMode, NamespaceKind, Receiver, ResolvedCallee, SourcePosition,
        SourceRange,
    };

    fn range(line: u32) -> SourceRange {
        SourceRange {
            start: SourcePosition { line, character: 0 },
            end: SourcePosition {
                line,
                character: 10,
            },
        }
    }

    fn context(
        method_name: &str,
        current_namespace: Vec<String>,
        callee_owner: Vec<String>,
        callee_kind: NamespaceKind,
    ) -> CallContext {
        CallContext {
            project: None,
            method_name: method_name.to_string(),
            receiver: Receiver::None,
            arguments: Vec::new(),
            current_namespace,
            namespace_kind: NamespaceKind::Instance,
            call_range: range(1),
            block_range: Some(range(2)),
            message_range: range(1),
            resolved_callees: vec![ResolvedCallee {
                owner: callee_owner,
                owner_kind: callee_kind,
                method: method_name.to_string(),
                resolution: CalleeResolution::Exact,
            }],
            enclosing_calls: Vec::new(),
        }
    }

    fn namespace_target(parts: &[&str], owner_kind: NamespaceKind) -> ExecutionContextTarget {
        ExecutionContextTarget::Namespace {
            namespace: parts.iter().map(|part| (*part).to_string()).collect(),
            owner_kind,
        }
    }

    #[test]
    fn classic_route_changes_receiver_without_changing_lexical_definee() {
        let context = context(
            "get",
            Vec::new(),
            vec!["Sinatra".to_string(), "Delegator".to_string()],
            NamespaceKind::Instance,
        );

        let output = extension().index_call_output(&context);

        assert_eq!(output.execution_contexts.len(), 1);
        let execution = &output.execution_contexts[0];
        assert_eq!(
            execution.implicit_receiver,
            namespace_target(&["Sinatra", "Application"], NamespaceKind::Instance)
        );
        assert_eq!(
            execution.method_definition_owner,
            namespace_target(&["Object"], NamespaceKind::Instance)
        );
        assert_eq!(execution.lexical_scope, LexicalScopeMode::Preserve);
        assert_eq!(execution.local_scope, LocalScopeMode::Preserve);
    }

    #[test]
    fn modular_helpers_separate_class_self_from_instance_method_owner() {
        let context = context(
            "helpers",
            vec!["Admin".to_string(), "App".to_string()],
            vec!["Sinatra".to_string(), "Base".to_string()],
            NamespaceKind::Singleton,
        );

        let output = extension().index_call_output(&context);

        assert_eq!(output.execution_contexts.len(), 1);
        let execution = &output.execution_contexts[0];
        assert_eq!(
            execution.implicit_receiver,
            namespace_target(&["Admin", "App"], NamespaceKind::Singleton)
        );
        assert_eq!(
            execution.method_definition_owner,
            namespace_target(&["Admin", "App"], NamespaceKind::Instance)
        );
        assert_eq!(execution.lexical_scope, LexicalScopeMode::Preserve);
        assert_eq!(execution.local_scope, LocalScopeMode::Preserve);
    }

    #[test]
    fn unrelated_same_named_calls_are_ignored() {
        let context = context(
            "get",
            vec!["Registry".to_string()],
            vec!["Registry".to_string()],
            NamespaceKind::Singleton,
        );

        assert!(extension()
            .index_call_output(&context)
            .execution_contexts
            .is_empty());
    }

    #[test]
    fn helper_module_is_included_in_the_application_instance() {
        let mut context = context(
            "helpers",
            vec!["Admin".to_string(), "App".to_string()],
            vec!["Sinatra".to_string(), "Base".to_string()],
            NamespaceKind::Singleton,
        );
        context.block_range = None;
        context.arguments.push(Argument {
            keyword: None,
            value: ArgumentValue::Constant(vec!["SharedHelpers".to_string()]),
            range: range(1),
        });

        let output = extension().index_call_output(&context);

        assert_eq!(output.index_patches.len(), 1);
        let IndexPatch::ApplyMixin(mixin) = &output.index_patches[0] else {
            panic!(
                "INVARIANT VIOLATED: Sinatra helpers emitted a non-mixin patch. This is a guest bug because helper modules enter request scope through inclusion. Fix: emit ApplyMixinPatch for constant helper arguments."
            );
        };
        assert_eq!(
            mixin.owner_target,
            Some(namespace_target(&["Admin", "App"], NamespaceKind::Instance))
        );
        assert_eq!(mixin.mixin, ["SharedHelpers".to_string()]);
        assert_eq!(mixin.kind, MixinKind::Include);
    }
}
