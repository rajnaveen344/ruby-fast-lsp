use ruby_fast_lsp_extension_api::{
    ApplyMixinPatch, ArgumentValue, BlockExecutionContextPatch, CallContext,
    ConnectExecutionContextPatch, DefineMethodPatch, ExecutionContextTarget, Extension,
    ExtensionOutput, GeneratedOwnerPatch, GeneratedOwnerScope, IndexPatch, LexicalScopeMode,
    LocalScopeMode, MethodParamKind, MethodParamPatch, MethodReturnTypeSource, MethodVisibility,
    MixinKind, NamespaceDeclarationKind, NamespaceKind, PatchSource, Receiver, ResolvedCall,
    SourceRange,
};

pub fn extension() -> RSpecExtension {
    RSpecExtension
}

pub struct RSpecExtension;

impl Extension for RSpecExtension {
    fn id(&self) -> &'static str {
        "rspec-ruby"
    }

    fn indexed_call_names(&self) -> &'static [&'static str] {
        &[
            "describe",
            "context",
            "shared_context",
            "include_context",
            "shared_examples",
            "shared_examples_for",
            "include_examples",
            "it_behaves_like",
            "it_should_behave_like",
            "it",
            "example",
            "specify",
            "before",
            "after",
            "around",
            "let",
            "let!",
            "subject",
            "subject!",
            "include",
            "prepend",
            "extend",
        ]
    }

    fn index_call(&self, ctx: &CallContext) -> Vec<IndexPatch> {
        if is_rspec_root_describe(ctx)
            || is_rspec_root_shared_context(ctx)
            || is_rspec_root_shared_examples(ctx)
        {
            return Vec::new();
        }

        if ctx.receiver != Receiver::None {
            return Vec::new();
        }
        if !inside_rspec_scope(ctx) {
            return Vec::new();
        }

        match ctx.method_name.as_str() {
            "describe" | "context" | "it" | "example" | "specify" | "before" | "after"
            | "around" => {
                vec![self.define_dsl_macro(ctx, ctx.current_namespace.clone(), ctx.namespace_kind)]
            }
            "let" | "let!" => {
                let mut patches = vec![self.define_dsl_macro(
                    ctx,
                    ctx.current_namespace.clone(),
                    ctx.namespace_kind,
                )];
                patches.extend(self.define_named_helper(ctx));
                patches
            }
            "subject" | "subject!" => {
                let mut patches = Vec::new();
                if first_symbol_or_string(ctx).is_some() || ctx.method_name != "subject" {
                    patches.push(self.define_dsl_macro(
                        ctx,
                        ctx.current_namespace.clone(),
                        ctx.namespace_kind,
                    ));
                }
                patches.extend(self.define_subject_helper(ctx));
                patches
            }
            "include" => self.apply_mixin(ctx, MixinKind::Include),
            "prepend" => self.apply_mixin(ctx, MixinKind::Prepend),
            "extend" => self.apply_mixin(ctx, MixinKind::Extend),
            "include_context" => self.include_shared_context(ctx),
            "include_examples" | "it_behaves_like" | "it_should_behave_like" => {
                self.apply_shared_examples(ctx)
            }
            _ => Vec::new(),
        }
    }

    fn index_call_output(&self, ctx: &CallContext) -> ExtensionOutput {
        let mut output = ExtensionOutput::index_patches(self.index_call(ctx));
        if let Some(context) = self
            .example_group_execution_context(ctx)
            .or_else(|| self.shared_context_execution_context(ctx))
            .or_else(|| self.shared_examples_execution_context(ctx))
            .or_else(|| self.runtime_block_execution_context(ctx))
        {
            output.execution_contexts.push(context);
        }
        output
    }
}

fn is_rspec_root_describe(ctx: &CallContext) -> bool {
    ctx.method_name == "describe"
        && ctx.receiver == Receiver::Constant(vec!["RSpec".to_string()])
        && ctx
            .resolved_callees
            .iter()
            .any(|callee| callee.owner == ["RSpec".to_string()])
}

fn is_rspec_root_shared_context(ctx: &CallContext) -> bool {
    ctx.method_name == "shared_context"
        && ctx.receiver == Receiver::Constant(vec!["RSpec".to_string()])
        && ctx.resolved_callees.iter().any(|callee| {
            callee.owner == ["RSpec".to_string()] && callee.method == "shared_context"
        })
}

fn is_rspec_root_shared_examples(ctx: &CallContext) -> bool {
    is_shared_examples_declaration(ctx.method_name.as_str())
        && ctx.receiver == Receiver::Constant(vec!["RSpec".to_string()])
        && ctx.resolved_callees.iter().any(|callee| {
            callee.owner == ["RSpec".to_string()]
                && is_shared_examples_declaration(callee.method.as_str())
        })
}

fn is_shared_examples_declaration(method: &str) -> bool {
    matches!(method, "shared_examples" | "shared_examples_for")
}

fn inside_rspec_scope(ctx: &CallContext) -> bool {
    ctx.enclosing_calls.iter().any(|call| {
        call.resolved_callees.iter().any(|callee| {
            callee.owner == ["RSpec".to_string()]
                && matches!(
                    callee.method.as_str(),
                    "describe"
                        | "context"
                        | "shared_examples"
                        | "shared_examples_for"
                        | "shared_context"
                )
        })
    })
}

impl RSpecExtension {
    fn shared_context_execution_context(
        &self,
        ctx: &CallContext,
    ) -> Option<BlockExecutionContextPatch> {
        if !is_rspec_root_shared_context(ctx) {
            return None;
        }
        let block_range = ctx.block_range?;
        let (name, _) = first_symbol_or_string(ctx)?;
        let local_id = shared_context_owner_local_id(&name);
        Some(BlockExecutionContextPatch {
            call_range: ctx.call_range,
            block_range,
            generated_owners: vec![GeneratedOwnerPatch {
                local_id: local_id.clone(),
                scope: GeneratedOwnerScope::Project,
                declaration_kind: NamespaceDeclarationKind::Module,
                owner_kind: NamespaceKind::Instance,
                parent: None,
            }],
            implicit_receiver: ExecutionContextTarget::ProjectGeneratedOwner {
                local_id: local_id.clone(),
                owner_kind: Some(NamespaceKind::Singleton),
            },
            method_definition_owner: ExecutionContextTarget::ProjectGeneratedOwner {
                local_id,
                owner_kind: Some(NamespaceKind::Instance),
            },
            lexical_scope: LexicalScopeMode::Preserve,
            local_scope: LocalScopeMode::Preserve,
            source: PatchSource {
                extension_id: self.id().to_string(),
                macro_name: ctx.method_name.clone(),
            },
        })
    }

    fn shared_examples_execution_context(
        &self,
        ctx: &CallContext,
    ) -> Option<BlockExecutionContextPatch> {
        if !is_rspec_root_shared_examples(ctx) {
            return None;
        }
        let block_range = ctx.block_range?;
        let (name, _) = first_symbol_or_string(ctx)?;
        let local_id = shared_examples_owner_local_id(&name);
        Some(BlockExecutionContextPatch {
            call_range: ctx.call_range,
            block_range,
            generated_owners: vec![GeneratedOwnerPatch {
                local_id: local_id.clone(),
                scope: GeneratedOwnerScope::Project,
                declaration_kind: NamespaceDeclarationKind::Module,
                owner_kind: NamespaceKind::Instance,
                parent: None,
            }],
            implicit_receiver: ExecutionContextTarget::ProjectGeneratedOwner {
                local_id: local_id.clone(),
                owner_kind: Some(NamespaceKind::Singleton),
            },
            method_definition_owner: ExecutionContextTarget::ProjectGeneratedOwner {
                local_id,
                owner_kind: Some(NamespaceKind::Instance),
            },
            lexical_scope: LexicalScopeMode::Preserve,
            local_scope: LocalScopeMode::Preserve,
            source: PatchSource {
                extension_id: self.id().to_string(),
                macro_name: ctx.method_name.clone(),
            },
        })
    }

    fn generated_group_chain(
        &self,
        ctx: &CallContext,
    ) -> (Vec<GeneratedOwnerPatch>, Option<ExecutionContextTarget>) {
        let mut owners = Vec::new();
        let mut parent = ExecutionContextTarget::Namespace {
            namespace: vec![
                "RSpec".to_string(),
                "Core".to_string(),
                "ExampleGroup".to_string(),
            ],
            owner_kind: NamespaceKind::Instance,
        };
        for enclosing in ctx
            .enclosing_calls
            .iter()
            .filter(|call| is_rspec_group_call(call))
        {
            let local_id = group_owner_local_id(enclosing.call_range);
            owners.push(GeneratedOwnerPatch {
                local_id: local_id.clone(),
                scope: GeneratedOwnerScope::Source,
                declaration_kind: NamespaceDeclarationKind::Class,
                owner_kind: NamespaceKind::Instance,
                parent: Some(parent),
            });
            parent = ExecutionContextTarget::GeneratedOwner {
                local_id,
                owner_kind: Some(NamespaceKind::Instance),
            };
        }
        let current = owners
            .last()
            .map(|owner| ExecutionContextTarget::GeneratedOwner {
                local_id: owner.local_id.clone(),
                owner_kind: Some(NamespaceKind::Instance),
            });
        (owners, current)
    }

    fn example_group_execution_context(
        &self,
        ctx: &CallContext,
    ) -> Option<BlockExecutionContextPatch> {
        let is_group = matches!(ctx.method_name.as_str(), "describe" | "context");
        if !is_group || !(is_rspec_root_describe(ctx) || inside_rspec_scope(ctx)) {
            return None;
        }
        let block_range = ctx.block_range?;
        let (mut owners, enclosing_target) = self.generated_group_chain(ctx);
        let parent = enclosing_target.unwrap_or(ExecutionContextTarget::Namespace {
            namespace: vec![
                "RSpec".to_string(),
                "Core".to_string(),
                "ExampleGroup".to_string(),
            ],
            owner_kind: NamespaceKind::Instance,
        });
        let current_id = group_owner_local_id(ctx.call_range);
        owners.push(GeneratedOwnerPatch {
            local_id: current_id.clone(),
            scope: GeneratedOwnerScope::Source,
            declaration_kind: NamespaceDeclarationKind::Class,
            owner_kind: NamespaceKind::Instance,
            parent: Some(parent),
        });
        let implicit_target = ExecutionContextTarget::GeneratedOwner {
            local_id: current_id,
            owner_kind: Some(NamespaceKind::Singleton),
        };
        let definition_target = ExecutionContextTarget::GeneratedOwner {
            local_id: group_owner_local_id(ctx.call_range),
            owner_kind: Some(NamespaceKind::Instance),
        };
        Some(BlockExecutionContextPatch {
            call_range: ctx.call_range,
            block_range,
            generated_owners: owners,
            implicit_receiver: implicit_target,
            method_definition_owner: definition_target,
            lexical_scope: LexicalScopeMode::Preserve,
            local_scope: LocalScopeMode::Preserve,
            source: PatchSource {
                extension_id: self.id().to_string(),
                macro_name: ctx.method_name.clone(),
            },
        })
    }

    fn runtime_block_execution_context(
        &self,
        ctx: &CallContext,
    ) -> Option<BlockExecutionContextPatch> {
        if !matches!(
            ctx.method_name.as_str(),
            "it" | "example" | "specify" | "before" | "after" | "around"
        ) || !inside_rspec_scope(ctx)
        {
            return None;
        }
        let block_range = ctx.block_range?;
        if let Some(name) = enclosing_shared_examples_name(ctx) {
            let runtime_id = shared_examples_runtime_owner_local_id(&name);
            let runtime_target = ExecutionContextTarget::ProjectGeneratedOwner {
                local_id: runtime_id.clone(),
                owner_kind: Some(NamespaceKind::Singleton),
            };
            return Some(BlockExecutionContextPatch {
                call_range: ctx.call_range,
                block_range,
                generated_owners: vec![
                    GeneratedOwnerPatch {
                        local_id: shared_examples_owner_local_id(&name),
                        scope: GeneratedOwnerScope::Project,
                        declaration_kind: NamespaceDeclarationKind::Module,
                        owner_kind: NamespaceKind::Instance,
                        parent: None,
                    },
                    GeneratedOwnerPatch {
                        local_id: runtime_id,
                        scope: GeneratedOwnerScope::Project,
                        declaration_kind: NamespaceDeclarationKind::Class,
                        owner_kind: NamespaceKind::Singleton,
                        parent: Some(ExecutionContextTarget::ProjectGeneratedOwner {
                            local_id: shared_examples_owner_local_id(&name),
                            owner_kind: Some(NamespaceKind::Instance),
                        }),
                    },
                ],
                implicit_receiver: runtime_target.clone(),
                method_definition_owner: runtime_target,
                lexical_scope: LexicalScopeMode::Preserve,
                local_scope: LocalScopeMode::Preserve,
                source: PatchSource {
                    extension_id: self.id().to_string(),
                    macro_name: ctx.method_name.clone(),
                },
            });
        }
        let (mut owners, group_target) = self.generated_group_chain(ctx);
        let group_target = group_target?;
        let group_call = ctx
            .enclosing_calls
            .iter()
            .rev()
            .find(|call| is_rspec_group_call(call))?;
        let shared_runtime_id = shared_runtime_owner_local_id(group_call.call_range);
        owners.push(GeneratedOwnerPatch {
            local_id: shared_runtime_id.clone(),
            scope: GeneratedOwnerScope::Source,
            declaration_kind: NamespaceDeclarationKind::Class,
            owner_kind: NamespaceKind::Singleton,
            parent: Some(group_target),
        });
        let shared_runtime_target = ExecutionContextTarget::GeneratedOwner {
            local_id: shared_runtime_id,
            owner_kind: Some(NamespaceKind::Singleton),
        };
        let runtime_target = if matches!(ctx.method_name.as_str(), "before" | "after" | "around") {
            shared_runtime_target
        } else {
            let runtime_id = runtime_owner_local_id(ctx.method_name.as_str(), ctx.call_range);
            owners.push(GeneratedOwnerPatch {
                local_id: runtime_id.clone(),
                scope: GeneratedOwnerScope::Source,
                declaration_kind: NamespaceDeclarationKind::Class,
                owner_kind: NamespaceKind::Singleton,
                parent: Some(shared_runtime_target),
            });
            ExecutionContextTarget::GeneratedOwner {
                local_id: runtime_id,
                owner_kind: Some(NamespaceKind::Singleton),
            }
        };
        Some(BlockExecutionContextPatch {
            call_range: ctx.call_range,
            block_range,
            generated_owners: owners,
            implicit_receiver: runtime_target.clone(),
            method_definition_owner: runtime_target,
            lexical_scope: LexicalScopeMode::Preserve,
            local_scope: LocalScopeMode::Preserve,
            source: PatchSource {
                extension_id: self.id().to_string(),
                macro_name: ctx.method_name.clone(),
            },
        })
    }

    fn define_dsl_macro(
        &self,
        ctx: &CallContext,
        namespace: Vec<String>,
        owner_kind: NamespaceKind,
    ) -> IndexPatch {
        IndexPatch::DefineMethod(DefineMethodPatch {
            name: ctx.method_name.clone(),
            namespace,
            owner_target: current_example_group_target(ctx),
            owner_kind,
            visibility: MethodVisibility::Public,
            location: ctx.message_range,
            params: dsl_params(ctx.method_name.as_str()),
            return_type: None,
            return_type_source: None,
            source: PatchSource {
                extension_id: self.id().to_string(),
                macro_name: ctx.method_name.clone(),
            },
        })
    }

    fn define_named_helper(&self, ctx: &CallContext) -> Vec<IndexPatch> {
        let Some((name, location)) = first_symbol_or_string(ctx) else {
            return Vec::new();
        };

        vec![IndexPatch::DefineMethod(DefineMethodPatch {
            name,
            namespace: ctx.current_namespace.clone(),
            owner_target: current_example_group_target(ctx),
            owner_kind: NamespaceKind::Instance,
            visibility: MethodVisibility::Public,
            location,
            params: Vec::new(),
            return_type: None,
            return_type_source: Some(MethodReturnTypeSource::Block),
            source: PatchSource {
                extension_id: self.id().to_string(),
                macro_name: ctx.method_name.clone(),
            },
        })]
    }

    fn define_subject_helper(&self, ctx: &CallContext) -> Vec<IndexPatch> {
        if let Some((name, location)) = first_symbol_or_string(ctx) {
            return vec![IndexPatch::DefineMethod(DefineMethodPatch {
                name,
                namespace: ctx.current_namespace.clone(),
                owner_target: current_example_group_target(ctx),
                owner_kind: NamespaceKind::Instance,
                visibility: MethodVisibility::Public,
                location,
                params: Vec::new(),
                return_type: None,
                return_type_source: Some(MethodReturnTypeSource::Block),
                source: PatchSource {
                    extension_id: self.id().to_string(),
                    macro_name: ctx.method_name.clone(),
                },
            })];
        }

        vec![IndexPatch::DefineMethod(DefineMethodPatch {
            name: "subject".to_string(),
            namespace: ctx.current_namespace.clone(),
            owner_target: current_example_group_target(ctx),
            owner_kind: NamespaceKind::Instance,
            visibility: MethodVisibility::Public,
            location: ctx.call_range,
            params: Vec::new(),
            return_type: None,
            return_type_source: Some(MethodReturnTypeSource::Block),
            source: PatchSource {
                extension_id: self.id().to_string(),
                macro_name: ctx.method_name.clone(),
            },
        })]
    }

    fn apply_mixin(&self, ctx: &CallContext, kind: MixinKind) -> Vec<IndexPatch> {
        ctx.arguments
            .iter()
            .flat_map(|arg| match &arg.value {
                ArgumentValue::Constant(parts) => {
                    self.mixin_patches(ctx, parts.clone(), kind, arg.range)
                }
                ArgumentValue::Symbol(_)
                | ArgumentValue::String(_)
                | ArgumentValue::Boolean(_)
                | ArgumentValue::Nil
                | ArgumentValue::Unsupported => Vec::new(),
            })
            .collect()
    }

    fn include_shared_context(&self, ctx: &CallContext) -> Vec<IndexPatch> {
        let Some((name, location)) = first_symbol_or_string(ctx) else {
            return Vec::new();
        };
        vec![
            self.define_dsl_macro(ctx, ctx.current_namespace.clone(), ctx.namespace_kind),
            IndexPatch::ApplyMixin(ApplyMixinPatch {
                namespace: ctx.current_namespace.clone(),
                owner_target: current_example_group_target(ctx),
                target_kind: NamespaceKind::Instance,
                mixin_target: Some(ExecutionContextTarget::ProjectGeneratedOwner {
                    local_id: shared_context_owner_local_id(&name),
                    owner_kind: Some(NamespaceKind::Instance),
                }),
                mixin: Vec::new(),
                absolute: false,
                kind: MixinKind::Include,
                location,
                source: PatchSource {
                    extension_id: self.id().to_string(),
                    macro_name: ctx.method_name.clone(),
                },
            }),
        ]
    }

    fn apply_shared_examples(&self, ctx: &CallContext) -> Vec<IndexPatch> {
        let Some((name, location)) = first_symbol_or_string(ctx) else {
            return Vec::new();
        };
        let Some(group_target) =
            current_example_group_target_with_kind(ctx, Some(NamespaceKind::Instance))
        else {
            return Vec::new();
        };
        vec![
            IndexPatch::DefineMethod(DefineMethodPatch {
                name: ctx.method_name.clone(),
                namespace: ctx.current_namespace.clone(),
                owner_target: Some(group_target.clone()),
                owner_kind: NamespaceKind::Instance,
                visibility: MethodVisibility::Public,
                location: ctx.message_range,
                params: dsl_params(ctx.method_name.as_str()),
                return_type: None,
                return_type_source: None,
                source: PatchSource {
                    extension_id: self.id().to_string(),
                    macro_name: ctx.method_name.clone(),
                },
            }),
            IndexPatch::ApplyMixin(ApplyMixinPatch {
                namespace: ctx.current_namespace.clone(),
                owner_target: Some(group_target.clone()),
                target_kind: NamespaceKind::Instance,
                mixin_target: Some(ExecutionContextTarget::ProjectGeneratedOwner {
                    local_id: shared_examples_owner_local_id(&name),
                    owner_kind: Some(NamespaceKind::Instance),
                }),
                mixin: Vec::new(),
                absolute: false,
                kind: MixinKind::Include,
                location,
                source: PatchSource {
                    extension_id: self.id().to_string(),
                    macro_name: ctx.method_name.clone(),
                },
            }),
            IndexPatch::ConnectExecutionContext(ConnectExecutionContextPatch {
                template: ExecutionContextTarget::ProjectGeneratedOwner {
                    local_id: shared_examples_runtime_owner_local_id(&name),
                    owner_kind: Some(NamespaceKind::Singleton),
                },
                application: group_target,
                location,
                source: PatchSource {
                    extension_id: self.id().to_string(),
                    macro_name: ctx.method_name.clone(),
                },
            }),
        ]
    }

    fn mixin_patches(
        &self,
        ctx: &CallContext,
        parts: Vec<String>,
        kind: MixinKind,
        location: ruby_fast_lsp_extension_api::SourceRange,
    ) -> Vec<IndexPatch> {
        let instance_kind = match kind {
            MixinKind::Include => MixinKind::Include,
            MixinKind::Prepend => MixinKind::Prepend,
            MixinKind::Extend => MixinKind::Include,
        };
        let mut patches = vec![self.mixin_patch(
            ctx,
            parts.clone(),
            instance_kind,
            NamespaceKind::Singleton,
            location,
        )];

        if !matches!(kind, MixinKind::Extend) {
            patches.push(self.mixin_patch(ctx, parts, kind, NamespaceKind::Instance, location));
        }

        patches
    }

    fn mixin_patch(
        &self,
        ctx: &CallContext,
        parts: Vec<String>,
        kind: MixinKind,
        target_kind: NamespaceKind,
        location: ruby_fast_lsp_extension_api::SourceRange,
    ) -> IndexPatch {
        IndexPatch::ApplyMixin(ApplyMixinPatch {
            namespace: ctx.current_namespace.clone(),
            owner_target: current_example_group_target(ctx),
            target_kind,
            mixin_target: None,
            mixin: parts,
            absolute: false,
            kind,
            location,
            source: PatchSource {
                extension_id: self.id().to_string(),
                macro_name: ctx.method_name.clone(),
            },
        })
    }
}

fn is_rspec_group_call(call: &ResolvedCall) -> bool {
    matches!(call.method_name.as_str(), "describe" | "context")
        && (matches!(call.receiver, Receiver::None)
            || call
                .resolved_callees
                .iter()
                .any(|callee| callee.owner == ["RSpec".to_string()]))
}

fn group_owner_local_id(range: SourceRange) -> String {
    format!(
        "example-group:{}:{}-{}:{}",
        range.start.line, range.start.character, range.end.line, range.end.character
    )
}

fn runtime_owner_local_id(method_name: &str, range: SourceRange) -> String {
    format!(
        "runtime-{method_name}:{}:{}-{}:{}",
        range.start.line, range.start.character, range.end.line, range.end.character
    )
}

fn shared_runtime_owner_local_id(group_range: SourceRange) -> String {
    format!("group-runtime:{}", group_owner_local_id(group_range))
}

fn shared_context_owner_local_id(name: &str) -> String {
    format!("shared-context:{name}")
}

fn shared_examples_owner_local_id(name: &str) -> String {
    format!("shared-examples:{name}")
}

fn shared_examples_runtime_owner_local_id(name: &str) -> String {
    format!("shared-examples-runtime:{name}")
}

fn current_example_group_target(ctx: &CallContext) -> Option<ExecutionContextTarget> {
    current_example_group_target_with_kind(ctx, None)
}

fn current_example_group_target_with_kind(
    ctx: &CallContext,
    owner_kind: Option<NamespaceKind>,
) -> Option<ExecutionContextTarget> {
    ctx.enclosing_calls
        .iter()
        .rev()
        .find(|call| {
            is_rspec_group_call(call)
                || is_rspec_shared_context_call(call)
                || is_rspec_shared_examples_call(call)
        })
        .and_then(|call| {
            if is_rspec_shared_context_call(call) {
                let name = resolved_call_symbol_or_string(call)?;
                return Some(ExecutionContextTarget::ProjectGeneratedOwner {
                    local_id: shared_context_owner_local_id(&name),
                    owner_kind,
                });
            }
            if is_rspec_shared_examples_call(call) {
                let name = resolved_call_symbol_or_string(call)?;
                return Some(ExecutionContextTarget::ProjectGeneratedOwner {
                    local_id: shared_examples_owner_local_id(&name),
                    owner_kind,
                });
            }
            Some(ExecutionContextTarget::GeneratedOwner {
                local_id: group_owner_local_id(call.call_range),
                owner_kind,
            })
        })
}

fn is_rspec_shared_context_call(call: &ResolvedCall) -> bool {
    call.method_name == "shared_context"
        && call.receiver == Receiver::Constant(vec!["RSpec".to_string()])
        && call.resolved_callees.iter().any(|callee| {
            callee.owner == ["RSpec".to_string()] && callee.method == "shared_context"
        })
}

fn is_rspec_shared_examples_call(call: &ResolvedCall) -> bool {
    is_shared_examples_declaration(call.method_name.as_str())
        && call.receiver == Receiver::Constant(vec!["RSpec".to_string()])
        && call.resolved_callees.iter().any(|callee| {
            callee.owner == ["RSpec".to_string()]
                && is_shared_examples_declaration(callee.method.as_str())
        })
}

fn enclosing_shared_examples_name(ctx: &CallContext) -> Option<String> {
    ctx.enclosing_calls
        .iter()
        .rev()
        .find(|call| is_rspec_shared_examples_call(call))
        .and_then(resolved_call_symbol_or_string)
}

fn resolved_call_symbol_or_string(call: &ResolvedCall) -> Option<String> {
    call.arguments
        .first()
        .and_then(|argument| match &argument.value {
            ArgumentValue::Symbol(name) | ArgumentValue::String(name) => Some(name.clone()),
            ArgumentValue::Constant(_)
            | ArgumentValue::Boolean(_)
            | ArgumentValue::Nil
            | ArgumentValue::Unsupported => None,
        })
}

fn dsl_params(method_name: &str) -> Vec<MethodParamPatch> {
    match method_name {
        "let" | "let!" => vec![
            MethodParamPatch {
                name: "name".to_string(),
                kind: MethodParamKind::Required,
            },
            MethodParamPatch {
                name: "block".to_string(),
                kind: MethodParamKind::Block,
            },
        ],
        "subject" | "subject!" => vec![
            MethodParamPatch {
                name: "name".to_string(),
                kind: MethodParamKind::Optional,
            },
            MethodParamPatch {
                name: "block".to_string(),
                kind: MethodParamKind::Block,
            },
        ],
        "describe"
        | "context"
        | "shared_context"
        | "include_context"
        | "shared_examples"
        | "shared_examples_for"
        | "include_examples"
        | "it_behaves_like"
        | "it_should_behave_like"
        | "it"
        | "example"
        | "specify"
        | "before"
        | "after"
        | "around" => {
            vec![
                MethodParamPatch {
                    name: "args".to_string(),
                    kind: MethodParamKind::Rest,
                },
                MethodParamPatch {
                    name: "block".to_string(),
                    kind: MethodParamKind::Block,
                },
            ]
        }
        "include" | "prepend" | "extend" => Vec::new(),
        other => panic!(
            "INVARIANT VIOLATED: unknown RSpec DSL method `{other}` reached signature builder. \
             This is a bug because indexed_call_names and index_call must stay in sync. \
             Fix: add explicit signature handling for the DSL macro."
        ),
    }
}

fn first_symbol_or_string(
    ctx: &CallContext,
) -> Option<(String, ruby_fast_lsp_extension_api::SourceRange)> {
    let first = ctx.arguments.first()?;
    match &first.value {
        ArgumentValue::Symbol(name) | ArgumentValue::String(name) => {
            Some((name.clone(), first.range))
        }
        ArgumentValue::Constant(_)
        | ArgumentValue::Boolean(_)
        | ArgumentValue::Nil
        | ArgumentValue::Unsupported => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruby_fast_lsp_extension_api::{
        Argument, CalleeResolution, LockedGem, LockedGemSource, ProjectContext, ProjectSourceKind,
        ResolvedCallee, SourcePosition,
    };

    fn range(
        start_line: u32,
        start_character: u32,
        end_line: u32,
        end_character: u32,
    ) -> SourceRange {
        SourceRange {
            start: SourcePosition {
                line: start_line,
                character: start_character,
            },
            end: SourcePosition {
                line: end_line,
                character: end_character,
            },
        }
    }

    fn rspec_describe_callee() -> ResolvedCallee {
        ResolvedCallee {
            owner: vec!["RSpec".to_string()],
            owner_kind: NamespaceKind::Singleton,
            method: "describe".to_string(),
            resolution: CalleeResolution::Exact,
        }
    }

    fn root_describe_context() -> CallContext {
        CallContext {
            project: None,
            method_name: "describe".to_string(),
            receiver: Receiver::Constant(vec!["RSpec".to_string()]),
            arguments: Vec::new(),
            current_namespace: Vec::new(),
            namespace_kind: NamespaceKind::Singleton,
            call_range: range(1, 0, 8, 3),
            block_range: Some(range(1, 22, 8, 3)),
            message_range: range(1, 6, 1, 14),
            resolved_callees: vec![rspec_describe_callee()],
            enclosing_calls: Vec::new(),
        }
    }

    fn enclosing_describe() -> ResolvedCall {
        let root = root_describe_context();
        ResolvedCall {
            method_name: root.method_name,
            receiver: root.receiver,
            arguments: root.arguments,
            resolved_callees: root.resolved_callees,
            call_range: root.call_range,
            message_range: root.message_range,
            frame_extension_ids: vec!["rspec-ruby".to_string()],
        }
    }

    fn project_context() -> ProjectContext {
        ProjectContext {
            project_uri: "file:///workspace".to_string(),
            source_uri: "file:///workspace/spec/example_spec.rb".to_string(),
            source_kind: ProjectSourceKind::Project,
            workspace_trusted: true,
            ruby_version: Some("3.3".to_string()),
            lockfile_present: true,
            locked_gems_complete: true,
            locked_gems: vec![LockedGem {
                name: "rspec-core".to_string(),
                version: "3.13.1".to_string(),
                source: LockedGemSource::Registry,
            }],
        }
    }

    fn string_argument(value: &str) -> Argument {
        Argument {
            value: ArgumentValue::String(value.to_string()),
            range: range(1, 21, 1, 36),
            keyword: None,
        }
    }

    #[test]
    fn root_describe_emits_generated_example_group_context() {
        let output = extension().index_call_output(&root_describe_context());

        assert!(output.index_patches.is_empty());
        assert_eq!(output.execution_contexts.len(), 1);
        let context = &output.execution_contexts[0];
        assert_eq!(context.generated_owners.len(), 1);
        let owner = &context.generated_owners[0];
        assert_eq!(owner.local_id, "example-group:1:0-8:3");
        assert_eq!(owner.declaration_kind, NamespaceDeclarationKind::Class);
        assert_eq!(owner.owner_kind, NamespaceKind::Instance);
        assert_eq!(
            owner.parent,
            Some(ExecutionContextTarget::Namespace {
                namespace: vec![
                    "RSpec".to_string(),
                    "Core".to_string(),
                    "ExampleGroup".to_string(),
                ],
                owner_kind: NamespaceKind::Instance,
            })
        );
        let implicit_target = ExecutionContextTarget::GeneratedOwner {
            local_id: "example-group:1:0-8:3".to_string(),
            owner_kind: Some(NamespaceKind::Singleton),
        };
        let definition_target = ExecutionContextTarget::GeneratedOwner {
            local_id: "example-group:1:0-8:3".to_string(),
            owner_kind: Some(NamespaceKind::Instance),
        };
        assert_eq!(context.implicit_receiver, implicit_target);
        assert_eq!(context.method_definition_owner, definition_target);
    }

    #[test]
    fn nested_context_inherits_the_enclosing_generated_group() {
        let ctx = CallContext {
            project: None,
            method_name: "context".to_string(),
            receiver: Receiver::None,
            arguments: Vec::new(),
            current_namespace: Vec::new(),
            namespace_kind: NamespaceKind::Singleton,
            call_range: range(3, 2, 7, 5),
            block_range: Some(range(3, 20, 7, 5)),
            message_range: range(3, 2, 3, 9),
            resolved_callees: Vec::new(),
            enclosing_calls: vec![enclosing_describe()],
        };

        let output = extension().index_call_output(&ctx);
        let context = output.execution_contexts.first().expect(
            "INVARIANT VIOLATED: nested RSpec groups must emit an execution context. This is a bug because nested helper lookup depends on generated-owner inheritance. Fix: preserve the enclosing group chain in the RSpec adapter.",
        );
        assert_eq!(context.generated_owners.len(), 2);
        let outer_id = "example-group:1:0-8:3";
        let nested_id = "example-group:3:2-7:5";
        assert_eq!(context.generated_owners[0].local_id, outer_id);
        assert_eq!(context.generated_owners[1].local_id, nested_id);
        assert_eq!(
            context.generated_owners[1].parent,
            Some(ExecutionContextTarget::GeneratedOwner {
                local_id: outer_id.to_string(),
                owner_kind: Some(NamespaceKind::Instance),
            })
        );
        assert_eq!(
            context.implicit_receiver,
            ExecutionContextTarget::GeneratedOwner {
                local_id: nested_id.to_string(),
                owner_kind: Some(NamespaceKind::Singleton),
            }
        );
        let method = output
            .index_patches
            .first()
            .and_then(|patch| match patch {
                IndexPatch::DefineMethod(method) => Some(method),
                IndexPatch::ApplyMixin(_)
                | IndexPatch::DefineNamespace(_)
                | IndexPatch::DefineConstant(_)
                | IndexPatch::AddReference(_)
                | IndexPatch::SetSuperclass(_)
                | IndexPatch::ConnectExecutionContext(_) => None,
            })
            .expect(
                "INVARIANT VIOLATED: nested RSpec context must retain its DSL method patch. This is a bug because execution contexts augment rather than replace semantic patches. Fix: return both outputs from index_call_output.",
            );
        assert_eq!(
            method.owner_target,
            Some(ExecutionContextTarget::GeneratedOwner {
                local_id: outer_id.to_string(),
                owner_kind: None,
            })
        );
    }

    #[test]
    fn shared_context_uses_project_scoped_owner_and_exact_mixin_target() {
        let mut shared = root_describe_context();
        shared.project = Some(project_context());
        shared.method_name = "shared_context".to_string();
        shared.arguments = vec![string_argument("authenticated")];
        shared.resolved_callees = vec![ResolvedCallee {
            owner: vec!["RSpec".to_string()],
            owner_kind: NamespaceKind::Singleton,
            method: "shared_context".to_string(),
            resolution: CalleeResolution::Exact,
        }];

        let output = extension().index_call_output(&shared);
        let context = output.execution_contexts.first().expect(
            "INVARIANT VIOLATED: shared context must emit an execution context. This is a fixture bug because project-scoped helper ownership requires the context. Fix: preserve the shared_context adapter branch.",
        );
        assert_eq!(context.generated_owners.len(), 1);
        assert_eq!(
            context.generated_owners[0].scope,
            GeneratedOwnerScope::Project
        );
        assert_eq!(
            context.method_definition_owner,
            ExecutionContextTarget::ProjectGeneratedOwner {
                local_id: "shared-context:authenticated".to_string(),
                owner_kind: Some(NamespaceKind::Instance),
            }
        );

        let include = CallContext {
            project: Some(project_context()),
            method_name: "include_context".to_string(),
            receiver: Receiver::None,
            arguments: vec![string_argument("authenticated")],
            current_namespace: Vec::new(),
            namespace_kind: NamespaceKind::Singleton,
            call_range: range(3, 2, 3, 33),
            block_range: None,
            message_range: range(3, 2, 3, 17),
            resolved_callees: Vec::new(),
            enclosing_calls: vec![enclosing_describe()],
        };
        let patches = extension().index_call(&include);
        let mixin = patches
            .iter()
            .find_map(|patch| match patch {
                IndexPatch::ApplyMixin(mixin) => Some(mixin),
                IndexPatch::DefineMethod(_) => None,
                IndexPatch::DefineNamespace(_)
                | IndexPatch::DefineConstant(_)
                | IndexPatch::AddReference(_)
                | IndexPatch::SetSuperclass(_)
                | IndexPatch::ConnectExecutionContext(_) => None,
            })
            .expect("include_context must emit a semantic mixin patch");
        assert_eq!(mixin.mixin, Vec::<String>::new());
        assert_eq!(
            mixin.mixin_target,
            Some(ExecutionContextTarget::ProjectGeneratedOwner {
                local_id: "shared-context:authenticated".to_string(),
                owner_kind: Some(NamespaceKind::Instance),
            })
        );
    }

    #[test]
    fn shared_examples_connect_project_template_runtime_and_consuming_group() {
        let mut shared = root_describe_context();
        shared.project = Some(project_context());
        shared.method_name = "shared_examples".to_string();
        shared.arguments = vec![string_argument("auditable")];
        shared.resolved_callees = vec![ResolvedCallee {
            owner: vec!["RSpec".to_string()],
            owner_kind: NamespaceKind::Singleton,
            method: "shared_examples".to_string(),
            resolution: CalleeResolution::Exact,
        }];

        let declaration = extension().index_call_output(&shared);
        let context = declaration.execution_contexts.first().expect(
            "INVARIANT VIOLATED: shared examples must emit a project template context. This is a fixture bug because reusable example semantics require a stable owner. Fix: preserve the shared_examples adapter branch.",
        );
        assert_eq!(context.generated_owners.len(), 1);
        assert_eq!(
            context.generated_owners[0].scope,
            GeneratedOwnerScope::Project
        );
        assert_eq!(
            context.method_definition_owner,
            ExecutionContextTarget::ProjectGeneratedOwner {
                local_id: "shared-examples:auditable".to_string(),
                owner_kind: Some(NamespaceKind::Instance),
            }
        );

        let application = CallContext {
            project: Some(project_context()),
            method_name: "it_behaves_like".to_string(),
            receiver: Receiver::None,
            arguments: vec![string_argument("auditable")],
            current_namespace: Vec::new(),
            namespace_kind: NamespaceKind::Singleton,
            call_range: range(3, 2, 3, 29),
            block_range: None,
            message_range: range(3, 2, 3, 17),
            resolved_callees: Vec::new(),
            enclosing_calls: vec![enclosing_describe()],
        };
        let patches = extension().index_call(&application);
        assert_eq!(patches.len(), 3);
        let mixins = patches
            .iter()
            .filter_map(|patch| match patch {
                IndexPatch::ApplyMixin(mixin) => Some(mixin),
                IndexPatch::DefineMethod(_)
                | IndexPatch::DefineNamespace(_)
                | IndexPatch::DefineConstant(_)
                | IndexPatch::AddReference(_)
                | IndexPatch::SetSuperclass(_)
                | IndexPatch::ConnectExecutionContext(_) => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(mixins.len(), 1);
        assert_eq!(
            mixins[0].mixin_target,
            Some(ExecutionContextTarget::ProjectGeneratedOwner {
                local_id: "shared-examples:auditable".to_string(),
                owner_kind: Some(NamespaceKind::Instance),
            })
        );
        assert_eq!(
            patches[2],
            IndexPatch::ConnectExecutionContext(ConnectExecutionContextPatch {
                template: ExecutionContextTarget::ProjectGeneratedOwner {
                    local_id: "shared-examples-runtime:auditable".to_string(),
                    owner_kind: Some(NamespaceKind::Singleton),
                },
                application: ExecutionContextTarget::GeneratedOwner {
                    local_id: "example-group:1:0-8:3".to_string(),
                    owner_kind: Some(NamespaceKind::Instance),
                },
                location: string_argument("auditable").range,
                source: PatchSource {
                    extension_id: "rspec-ruby".to_string(),
                    macro_name: "it_behaves_like".to_string(),
                },
            })
        );
    }
}
