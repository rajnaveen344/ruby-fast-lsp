use super::*;
use ruby_fast_lsp_extension_api::{
    Argument, CalleeResolution, LockedGem, LockedGemSource, ProjectContext, ProjectSourceKind,
    ResolvedCallee, SourcePosition,
};

fn range(start_line: u32, start_character: u32, end_line: u32, end_character: u32) -> SourceRange {
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

fn subject_call(block_range: Option<SourceRange>) -> CallContext {
    CallContext {
        project: None,
        method_name: "subject".to_string(),
        receiver: Receiver::None,
        arguments: Vec::new(),
        current_namespace: Vec::new(),
        namespace_kind: NamespaceKind::Instance,
        call_range: range(4, 4, 4, 11),
        block_range,
        message_range: range(4, 4, 4, 11),
        resolved_callees: Vec::new(),
        enclosing_calls: vec![enclosing_describe()],
    }
}

#[test]
fn subject_with_block_declares_the_subject_helper() {
    let patches = extension().index_call(&subject_call(Some(range(4, 12, 4, 24))));
    let names: Vec<&str> = patches
        .iter()
        .filter_map(|patch| match patch {
            IndexPatch::DefineMethod(method) => Some(method.name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(names, vec!["subject"]);
}

#[test]
fn bare_subject_reads_without_declaring() {
    assert!(extension().index_call(&subject_call(None)).is_empty());
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
