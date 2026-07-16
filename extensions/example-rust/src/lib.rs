#![cfg(target_arch = "wasm32")]

use ruby_fast_lsp_extension_api::{
    ArgumentValue, BlockExecutionContextPatch, CallContext, CodeLensPatch, DefineMethodPatch,
    DocumentSymbolPatch, ExecutionContextTarget, ExtensionEvent, ExtensionOutput,
    GeneratedOwnerPatch, GeneratedOwnerScope, IndexPatch, LexicalScopeMode, LocalScopeMode,
    MethodVisibility, NamespaceDeclarationKind, NamespaceKind, PatchSource, ProjectContext,
    ResolvedCall, ResponsePatch, SourcePosition, SourceRange,
};
use ruby_fast_lsp_extension_guest_sdk::{export_extension, GuestExtension};

const EXTENSION_ID: &str = "example-rust";

struct ExampleRustExtension {
    isolation_probe_calls: u32,
}

impl GuestExtension for ExampleRustExtension {
    fn indexed_call_names(&self) -> &'static [&'static str] {
        &["scope", "property", "isolation_probe"]
    }

    fn index_call(&mut self, context: &CallContext) -> ExtensionOutput {
        if !supports_call_project(context) {
            return ExtensionOutput::index_patches(Vec::new());
        }
        match context.method_name.as_str() {
            "scope" if is_root_scope(context) => scope_output(context),
            "property" => property_output(context),
            "isolation_probe" => {
                self.isolation_probe_calls += 1;
                if self.isolation_probe_calls == 1 {
                    isolation_probe_output(context)
                } else {
                    ExtensionOutput::index_patches(Vec::new())
                }
            }
            _ => ExtensionOutput::index_patches(Vec::new()),
        }
    }

    fn handle_event(&mut self, event: &ExtensionEvent) -> ExtensionOutput {
        if event.event == "index.call.enter" {
            let context = event.call.as_ref().expect(
                "INVARIANT VIOLATED: index.call.enter omitted CallContext. This is a host/guest ABI bug because call events require their typed call payload. Fix: encode CallContext on every index.call.enter event.",
            );
            return self.index_call(context);
        }
        if matches!(
            event.event.as_str(),
            "request.document_symbol" | "request.code_lens"
        ) {
            return self.response_output(event);
        }
        ExtensionOutput::index_patches(Vec::new())
    }
}

impl ExampleRustExtension {
    fn response_output(&self, event: &ExtensionEvent) -> ExtensionOutput {
        let Some(document) = event.document.as_ref() else {
            return ExtensionOutput::index_patches(Vec::new());
        };
        if !document.project.as_ref().is_some_and(supports_project)
            || self.isolation_probe_calls == 0
        {
            return ExtensionOutput::index_patches(Vec::new());
        }

        let zero = SourcePosition {
            line: 0,
            character: 0,
        };
        let range = SourceRange {
            start: zero,
            end: zero,
        };
        let response_patch = match event.event.as_str() {
            "request.document_symbol" => ResponsePatch::DocumentSymbol(DocumentSymbolPatch {
                name: "project-isolated-symbol".to_string(),
                detail: Some("typed Rust guest project state".to_string()),
                kind: "Method".to_string(),
                range,
                selection_range: range,
                source: source("isolation_probe"),
            }),
            "request.code_lens" => ResponsePatch::CodeLens(CodeLensPatch {
                title: "Project-isolated lens".to_string(),
                command: "ruby-fast-lsp.example.projectIsolated".to_string(),
                range,
                arguments: vec![document.uri.clone()],
                source: source("isolation_probe"),
            }),
            other => panic!(
                "INVARIANT VIOLATED: response_output received unsupported event `{other}`. This is a guest bug because handle_event must route only declared response events. Fix: add an explicit response variant or stop routing the event."
            ),
        };
        ExtensionOutput {
            index_patches: Vec::new(),
            execution_contexts: Vec::new(),
            response_patches: vec![response_patch],
            command_patches: Vec::new(),
            process_requests: Vec::new(),
            reindex_files: Vec::new(),
        }
    }
}

fn isolation_probe_output(context: &CallContext) -> ExtensionOutput {
    ExtensionOutput::index_patches(vec![IndexPatch::DefineMethod(DefineMethodPatch {
        name: "project_isolated".to_string(),
        namespace: context.current_namespace.clone(),
        owner_target: None,
        owner_kind: context.namespace_kind,
        visibility: MethodVisibility::Public,
        location: context.message_range,
        params: Vec::new(),
        return_type: None,
        return_type_source: None,
        source: source("isolation_probe"),
    })])
}

fn supports_call_project(context: &CallContext) -> bool {
    context.project.as_ref().is_some_and(supports_project)
}

fn supports_project(project: &ProjectContext) -> bool {
    project.lockfile_present
        && project.locked_gems_complete
        && project
            .locked_gems
            .iter()
            .any(|gem| gem.name == "example-framework" && gem.version == "1.0.0")
}

fn is_root_scope(context: &CallContext) -> bool {
    context
        .resolved_callees
        .iter()
        .any(|callee| callee.owner == ["ExampleDsl".to_string()] && callee.method == "scope")
}

fn is_scope_call(call: &ResolvedCall) -> bool {
    call.resolved_callees
        .iter()
        .any(|callee| callee.owner == ["ExampleDsl".to_string()] && callee.method == "scope")
}

fn owner_id(range: SourceRange) -> String {
    format!(
        "scope:{}:{}-{}:{}",
        range.start.line, range.start.character, range.end.line, range.end.character
    )
}

fn owner_target(range: SourceRange) -> ExecutionContextTarget {
    ExecutionContextTarget::GeneratedOwner {
        local_id: owner_id(range),
        owner_kind: Some(NamespaceKind::Instance),
    }
}

fn source(name: &str) -> PatchSource {
    PatchSource {
        extension_id: EXTENSION_ID.to_string(),
        macro_name: name.to_string(),
    }
}

fn scope_output(context: &CallContext) -> ExtensionOutput {
    let Some(block_range) = context.block_range else {
        return ExtensionOutput::index_patches(Vec::new());
    };
    let local_id = owner_id(context.call_range);
    ExtensionOutput {
        index_patches: Vec::new(),
        execution_contexts: vec![BlockExecutionContextPatch {
            call_range: context.call_range,
            block_range,
            generated_owners: vec![GeneratedOwnerPatch {
                local_id: local_id.clone(),
                scope: GeneratedOwnerScope::Source,
                declaration_kind: NamespaceDeclarationKind::Class,
                owner_kind: NamespaceKind::Instance,
                parent: Some(ExecutionContextTarget::Namespace {
                    namespace: vec!["Object".to_string()],
                    owner_kind: NamespaceKind::Instance,
                }),
            }],
            implicit_receiver: ExecutionContextTarget::GeneratedOwner {
                local_id: local_id.clone(),
                owner_kind: Some(NamespaceKind::Instance),
            },
            method_definition_owner: ExecutionContextTarget::GeneratedOwner {
                local_id,
                owner_kind: Some(NamespaceKind::Instance),
            },
            lexical_scope: LexicalScopeMode::Preserve,
            local_scope: LocalScopeMode::Preserve,
            source: source("scope"),
        }],
        response_patches: Vec::new(),
        command_patches: Vec::new(),
        process_requests: Vec::new(),
        reindex_files: Vec::new(),
    }
}

fn property_output(context: &CallContext) -> ExtensionOutput {
    let Some(scope) = context
        .enclosing_calls
        .iter()
        .rev()
        .find(|call| is_scope_call(call))
    else {
        return ExtensionOutput::index_patches(Vec::new());
    };
    let Some(argument) = context.arguments.first() else {
        return ExtensionOutput::index_patches(Vec::new());
    };
    let name = match &argument.value {
        ArgumentValue::Symbol(name) | ArgumentValue::String(name) => name.clone(),
        ArgumentValue::Constant(_)
        | ArgumentValue::Boolean(_)
        | ArgumentValue::Nil
        | ArgumentValue::Unsupported => return ExtensionOutput::index_patches(Vec::new()),
    };
    ExtensionOutput::index_patches(vec![IndexPatch::DefineMethod(DefineMethodPatch {
        name,
        namespace: context.current_namespace.clone(),
        owner_target: Some(owner_target(scope.call_range)),
        owner_kind: NamespaceKind::Instance,
        visibility: MethodVisibility::Public,
        location: argument.range,
        params: Vec::new(),
        return_type: None,
        return_type_source: None,
        source: source("property"),
    })])
}

fn extension() -> ExampleRustExtension {
    ExampleRustExtension {
        isolation_probe_calls: 0,
    }
}

export_extension!(extension);
