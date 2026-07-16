use ruby_fast_lsp_extension_api::{
    ApplyMixinPatch, ArgumentValue, BlockExecutionContextPatch, CallContext, CalleeResolution,
    ExecutionContextTarget, Extension, ExtensionOutput, GeneratedOwnerPatch, GeneratedOwnerScope,
    IndexPatch, LexicalScopeMode, LocalScopeMode, MixinKind, NamespaceDeclarationKind,
    NamespaceKind, PatchSource,
};

pub const EXTENSION_ID: &str = "cucumber-rust";

pub fn extension() -> CucumberExtension {
    CucumberExtension
}

pub struct CucumberExtension;

impl Extension for CucumberExtension {
    fn id(&self) -> &'static str {
        EXTENSION_ID
    }

    fn indexed_call_names(&self) -> &'static [&'static str] {
        &[
            "Given",
            "When",
            "Then",
            "And",
            "But",
            "Before",
            "After",
            "Around",
            "AfterStep",
            "BeforeAll",
            "AfterAll",
            "World",
        ]
    }

    fn index_call(&self, context: &CallContext) -> Vec<IndexPatch> {
        if context.method_name != "World" || !is_cucumber_dsl_call(context) {
            return Vec::new();
        }
        context
            .arguments
            .iter()
            .filter_map(|argument| match &argument.value {
                ArgumentValue::Constant(mixin) => Some(IndexPatch::ApplyMixin(ApplyMixinPatch {
                    namespace: Vec::new(),
                    owner_target: Some(world_target()),
                    target_kind: NamespaceKind::Instance,
                    mixin_target: None,
                    mixin: mixin.clone(),
                    absolute: false,
                    kind: MixinKind::Include,
                    location: argument.range,
                    source: source("World"),
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
        if !is_world_execution_block(context.method_name.as_str()) || !is_cucumber_dsl_call(context)
        {
            return output;
        }
        let Some(block_range) = context.block_range else {
            return output;
        };
        output.execution_contexts.push(BlockExecutionContextPatch {
            call_range: context.call_range,
            block_range,
            generated_owners: vec![GeneratedOwnerPatch {
                local_id: "world".to_string(),
                scope: GeneratedOwnerScope::Project,
                declaration_kind: NamespaceDeclarationKind::Class,
                owner_kind: NamespaceKind::Instance,
                parent: Some(ExecutionContextTarget::Namespace {
                    namespace: vec!["Object".to_string()],
                    owner_kind: NamespaceKind::Instance,
                }),
            }],
            implicit_receiver: world_target(),
            method_definition_owner: lexical_definition_owner(context),
            lexical_scope: LexicalScopeMode::Preserve,
            local_scope: LocalScopeMode::Preserve,
            source: source(context.method_name.as_str()),
        });
        output
    }
}

fn is_world_execution_block(method: &str) -> bool {
    matches!(
        method,
        "Given"
            | "When"
            | "Then"
            | "And"
            | "But"
            | "Before"
            | "After"
            | "Around"
            | "AfterStep"
            | "BeforeAll"
            | "AfterAll"
    )
}

fn is_cucumber_dsl_call(context: &CallContext) -> bool {
    context.resolved_callees.iter().any(|callee| {
        callee.method == context.method_name
            && callee.resolution == CalleeResolution::Exact
            && (callee.owner
                == [
                    "Cucumber".to_string(),
                    "Glue".to_string(),
                    "Dsl".to_string(),
                ]
                || callee.owner == ["Object".to_string()])
            && (callee.owner_kind == NamespaceKind::Instance
                || callee.owner_kind == NamespaceKind::Singleton)
    })
}

fn world_target() -> ExecutionContextTarget {
    ExecutionContextTarget::ProjectGeneratedOwner {
        local_id: "world".to_string(),
        owner_kind: Some(NamespaceKind::Instance),
    }
}

fn lexical_definition_owner(context: &CallContext) -> ExecutionContextTarget {
    ExecutionContextTarget::Namespace {
        namespace: if context.current_namespace.is_empty() {
            vec!["Object".to_string()]
        } else {
            context.current_namespace.clone()
        },
        owner_kind: context.namespace_kind,
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
        GeneratedOwnerScope, LexicalScopeMode, LocalScopeMode, NamespaceDeclarationKind,
        NamespaceKind, Receiver, ResolvedCallee, SourcePosition, SourceRange,
    };

    fn range(line: u32) -> SourceRange {
        SourceRange {
            start: SourcePosition { line, character: 0 },
            end: SourcePosition {
                line,
                character: 12,
            },
        }
    }

    fn context(method_name: &str) -> CallContext {
        CallContext {
            project: None,
            method_name: method_name.to_string(),
            receiver: Receiver::None,
            arguments: Vec::new(),
            current_namespace: Vec::new(),
            namespace_kind: NamespaceKind::Instance,
            call_range: range(1),
            block_range: Some(range(2)),
            message_range: range(1),
            resolved_callees: vec![ResolvedCallee {
                owner: vec![
                    "Cucumber".to_string(),
                    "Glue".to_string(),
                    "Dsl".to_string(),
                ],
                owner_kind: NamespaceKind::Instance,
                method: method_name.to_string(),
                resolution: CalleeResolution::Exact,
            }],
            enclosing_calls: Vec::new(),
        }
    }

    #[test]
    fn step_block_uses_project_world_but_preserves_lexical_definee() {
        let output = extension().index_call_output(&context("Given"));

        assert_eq!(output.execution_contexts.len(), 1);
        let execution = &output.execution_contexts[0];
        assert_eq!(execution.generated_owners.len(), 1);
        assert_eq!(
            execution.generated_owners[0].scope,
            GeneratedOwnerScope::Project
        );
        assert_eq!(
            execution.generated_owners[0].declaration_kind,
            NamespaceDeclarationKind::Class
        );
        assert_eq!(
            execution.implicit_receiver,
            ExecutionContextTarget::ProjectGeneratedOwner {
                local_id: "world".to_string(),
                owner_kind: Some(NamespaceKind::Instance),
            }
        );
        assert_eq!(
            execution.method_definition_owner,
            ExecutionContextTarget::Namespace {
                namespace: vec!["Object".to_string()],
                owner_kind: NamespaceKind::Instance,
            }
        );
        assert_eq!(execution.lexical_scope, LexicalScopeMode::Preserve);
        assert_eq!(execution.local_scope, LocalScopeMode::Preserve);
    }

    #[test]
    fn world_modules_are_mixed_into_the_project_world() {
        let mut context = context("World");
        context.block_range = None;
        context.arguments.push(Argument {
            keyword: None,
            value: ArgumentValue::Constant(vec!["BrowserHelpers".to_string()]),
            range: range(1),
        });

        let output = extension().index_call_output(&context);

        assert_eq!(output.index_patches.len(), 1);
        let IndexPatch::ApplyMixin(mixin) = &output.index_patches[0] else {
            panic!("INVARIANT VIOLATED: Cucumber World module emitted a non-mixin patch. This is a guest bug because World modules extend each scenario object. Fix: emit ApplyMixinPatch for constant World arguments.");
        };
        assert_eq!(
            mixin.owner_target,
            Some(ExecutionContextTarget::ProjectGeneratedOwner {
                local_id: "world".to_string(),
                owner_kind: Some(NamespaceKind::Instance),
            })
        );
        assert_eq!(mixin.mixin, ["BrowserHelpers".to_string()]);
    }

    #[test]
    fn world_factory_block_does_not_run_in_world_scope() {
        assert!(extension()
            .index_call_output(&context("World"))
            .execution_contexts
            .is_empty());
    }
}
