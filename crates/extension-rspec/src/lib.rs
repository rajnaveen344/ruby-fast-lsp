use ruby_fast_lsp_extension_api::{
    ApplyMixinPatch, ArgumentValue, CallContext, DefineMethodPatch, Extension, IndexPatch,
    MethodParamKind, MethodParamPatch, MethodVisibility, MixinKind, NamespaceKind, PatchSource,
    Receiver,
};

pub fn extension() -> RSpecExtension {
    RSpecExtension
}

pub struct RSpecExtension;

impl Extension for RSpecExtension {
    fn id(&self) -> &'static str {
        "rspec"
    }

    fn indexed_call_names(&self) -> &'static [&'static str] {
        &[
            "describe", "context", "it", "example", "specify", "before", "after", "around", "let",
            "let!", "subject", "subject!", "include", "prepend", "extend",
        ]
    }

    fn index_call(&self, ctx: &CallContext) -> Vec<IndexPatch> {
        if is_rspec_root_describe(ctx) {
            return vec![self.define_dsl_macro(
                ctx,
                vec!["RSpec".to_string()],
                NamespaceKind::Singleton,
            )];
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
                let mut patches = vec![self.define_dsl_macro(
                    ctx,
                    ctx.current_namespace.clone(),
                    ctx.namespace_kind,
                )];
                patches.extend(self.define_subject_helper(ctx));
                patches
            }
            "include" => self.apply_mixin(ctx, MixinKind::Include),
            "prepend" => self.apply_mixin(ctx, MixinKind::Prepend),
            "extend" => self.apply_mixin(ctx, MixinKind::Extend),
            _ => Vec::new(),
        }
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

fn inside_rspec_scope(ctx: &CallContext) -> bool {
    ctx.enclosing_calls.iter().any(|call| {
        call.resolved_callees.iter().any(|callee| {
            callee.owner == ["RSpec".to_string()]
                && matches!(
                    callee.method.as_str(),
                    "describe" | "context" | "shared_examples" | "shared_context"
                )
        })
    })
}

impl RSpecExtension {
    fn define_dsl_macro(
        &self,
        ctx: &CallContext,
        namespace: Vec<String>,
        owner_kind: NamespaceKind,
    ) -> IndexPatch {
        IndexPatch::DefineMethod(DefineMethodPatch {
            name: ctx.method_name.clone(),
            namespace,
            owner_kind,
            visibility: MethodVisibility::Public,
            location: ctx.message_range,
            params: dsl_params(ctx.method_name.as_str()),
            return_type: None,
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
            owner_kind: NamespaceKind::Instance,
            visibility: MethodVisibility::Public,
            location,
            params: Vec::new(),
            return_type: None,
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
                owner_kind: NamespaceKind::Instance,
                visibility: MethodVisibility::Public,
                location,
                params: Vec::new(),
                return_type: None,
                source: PatchSource {
                    extension_id: self.id().to_string(),
                    macro_name: ctx.method_name.clone(),
                },
            })];
        }

        vec![IndexPatch::DefineMethod(DefineMethodPatch {
            name: "subject".to_string(),
            namespace: ctx.current_namespace.clone(),
            owner_kind: NamespaceKind::Instance,
            visibility: MethodVisibility::Public,
            location: ctx.call_range,
            params: Vec::new(),
            return_type: None,
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
            target_kind,
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
        "describe" | "context" | "it" | "example" | "specify" | "before" | "after" | "around" => {
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
