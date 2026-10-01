use super::*;

#[test]
fn guest_call_context_delivers_project_only_for_legacy_per_call_guests() {
    let complete = rust_isolation_probe_context("file:///workspace/a");
    let project = complete
        .project
        .as_ref()
        .expect("fixture must carry an owning project");
    let mut compact = complete.clone();
    compact.project = None;

    let activation = guest_call_context(
        ExtensionProjectContextDelivery::Activation,
        project,
        &compact,
    );
    assert!(matches!(&activation, Cow::Borrowed(_)));
    assert!(activation.project.is_none());

    let per_call = guest_call_context(ExtensionProjectContextDelivery::PerCall, project, &compact);
    assert!(matches!(&per_call, Cow::Owned(_)));
    assert_eq!(
        per_call
            .project
            .as_ref()
            .map(|project| project.project_uri.as_str()),
        Some("file:///workspace/a")
    );

    let already_complete =
        guest_call_context(ExtensionProjectContextDelivery::PerCall, project, &complete);
    assert!(matches!(&already_complete, Cow::Borrowed(_)));
}

#[test]
fn wasm_private_state_is_isolated_per_project_uri() {
    let package_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("extensions/example-rust");
    let artifact =
        package_path.join("target/wasm32-wasip1/release/ruby_fast_lsp_example_rust_extension.wasm");
    if !artifact.is_file() {
        eprintln!(
            "skipping project-state Wasm isolation test; run extensions/example-rust/build-and-test.sh"
        );
        return;
    }
    let registry = ExtensionRegistry::load(&ExtensionLoadConfig {
        package_paths: vec![ConfiguredExtensionPath {
            path: package_path,
            source: ExtensionPathSource::InitializationOptions,
        }],
        ..ExtensionLoadConfig::default()
    });
    let registry = ExtensionRegistryHandle {
        inner: Arc::new(RwLock::new(registry)),
        reconfiguration: Arc::new(tokio::sync::Mutex::new(())),
        persistent_cache: None,
    };
    let extension = registry
        .extensions()
        .iter()
        .find(|extension| extension.metadata.id == "example-rust")
        .expect("typed Rust acceptance extension must load")
        .clone();

    let project_a = rust_isolation_probe_context("file:///workspace/a");
    let project_b = rust_isolation_probe_context("file:///workspace/b");
    assert_eq!(
        extension
            .index_call_output(&project_a)
            .expect("first project A probe must run")
            .index_patches
            .len(),
        1
    );
    let project_a_context = project_a
        .project
        .clone()
        .expect("project A probe must carry project context");
    let project_b_context = project_b
        .project
        .clone()
        .expect("project B probe must carry project context");
    let symbols_a = registry.document_symbols(
        "file:///workspace/a/probe.rb",
        "isolation_probe\n",
        Some(project_a_context.clone()),
    );
    assert_eq!(
        symbols_a
            .iter()
            .filter(|symbol| symbol.name == "project-isolated-symbol")
            .count(),
        1,
        "document responses must observe the same private project A Wasm instance as call hooks"
    );
    assert!(
        registry
            .document_symbols(
                "file:///workspace/b/probe.rb",
                "isolation_probe\n",
                Some(project_b_context.clone()),
            )
            .iter()
            .all(|symbol| symbol.name != "project-isolated-symbol"),
        "an untouched project B response must not observe project A guest state"
    );
    assert!(
        extension
            .index_call_output(&project_a)
            .expect("second project A probe must run")
            .index_patches
            .is_empty(),
        "the same project must observe its own guest state"
    );
    assert_eq!(
        extension
            .index_call_output(&project_b)
            .expect("first project B probe must run")
            .index_patches
            .len(),
        1,
        "project B must receive a fresh Wasm heap instead of project A state"
    );
    assert!(
        registry
            .document_symbols(
                "file:///workspace/b/probe.rb",
                "isolation_probe\n",
                Some(project_b_context.clone()),
            )
            .iter()
            .any(|symbol| symbol.name == "project-isolated-symbol"),
        "project B document responses must use project B's now-initialized Wasm instance"
    );
    assert!(
        registry
            .code_lenses(
                "file:///workspace/b/probe.rb",
                "isolation_probe\n",
                Some(project_b_context),
            )
            .iter()
            .any(|lens| lens
                .command
                .as_ref()
                .is_some_and(|command| command.title == "Project-isolated lens")),
        "code lenses must use the same project-aware response dispatch as document symbols"
    );
    let telemetry = extension.status_report().telemetry;
    assert_eq!(telemetry.project_instances, 2);
    assert_eq!(telemetry.project_instance_creations, 2);
    assert_eq!(telemetry.project_instance_failures, 0);
    assert!(telemetry.max_project_instance_time_ns <= telemetry.total_project_instance_time_ns);
    assert_eq!(
        telemetry.guest_calls, 8,
        "activation, three call hooks, three symbol requests, and one lens request must all be observed: {telemetry:?}"
    );
    assert_eq!(telemetry.lifecycle_calls, 1);
    assert_eq!(telemetry.index_calls, 3);
    assert_eq!(telemetry.event_calls, 4);
    assert!(telemetry.emitted_index_patches >= 2);
    assert!(telemetry.emitted_response_patches >= 3);
    assert_eq!(telemetry.guest_failures, 0);
    assert_eq!(telemetry.guest_traps, 0);
    assert_eq!(telemetry.resource_limit_failures, 0);
    assert_eq!(telemetry.disablements, 0);
    assert_eq!(telemetry.rejected_outputs, 0);
    assert_eq!(telemetry.patch_conflicts, 0);
    assert!(telemetry.max_guest_time_ns <= telemetry.total_guest_time_ns);
}

#[test]
fn execution_context_validation_rejects_spoofed_ranges_and_undeclared_targets() {
    let call = execution_call_fixture();
    let valid = execution_context_fixture("rspec-ruby");
    validate_execution_contexts("rspec-ruby", &call, std::slice::from_ref(&valid))
        .expect("valid execution context must pass the guest boundary");

    let mut spoofed = valid.clone();
    spoofed.source.extension_id = "other-extension".to_string();
    assert!(validate_execution_contexts("rspec-ruby", &call, &[spoofed])
        .expect_err("spoofed provenance must be rejected")
        .contains("provenance"));

    let mut wrong_block = valid.clone();
    wrong_block.block_range.start.character += 1;
    assert!(
        validate_execution_contexts("rspec-ruby", &call, &[wrong_block])
            .expect_err("a guest must not redirect semantics to another block")
            .contains("block_range")
    );

    let mut undeclared = valid;
    undeclared.method_definition_owner = ExecutionContextTarget::GeneratedOwner {
        local_id: "missing".to_string(),
        owner_kind: None,
    };
    assert!(
        validate_execution_contexts("rspec-ruby", &call, &[undeclared])
            .expect_err("undeclared generated targets must be rejected")
            .contains("undeclared")
    );
}

#[test]
fn execution_context_validation_accepts_exact_namespace_targets_without_generated_owners() {
    let call = execution_call_fixture();
    let mut context = execution_context_fixture("sinatra-rust");
    context.generated_owners.clear();
    context.implicit_receiver = ExecutionContextTarget::Namespace {
        namespace: vec!["Sinatra".to_string(), "Application".to_string()],
        owner_kind: AbiNamespaceKind::Instance,
    };
    context.method_definition_owner = ExecutionContextTarget::Namespace {
        namespace: vec!["Object".to_string()],
        owner_kind: AbiNamespaceKind::Instance,
    };
    context.source.extension_id = "sinatra-rust".to_string();

    validate_execution_contexts("sinatra-rust", &call, &[context])
        .expect("exact existing namespaces must not require an unrelated hidden-owner declaration");
}

#[test]
fn project_generated_execution_context_requires_project_and_validates_scope() {
    let mut call = execution_call_fixture();
    let mut context = execution_context_fixture("rspec-ruby");
    context.generated_owners[0].scope = ruby_fast_lsp_extension_api::GeneratedOwnerScope::Project;
    context.implicit_receiver = ExecutionContextTarget::ProjectGeneratedOwner {
        local_id: "group:2:2".to_string(),
        owner_kind: Some(AbiNamespaceKind::Singleton),
    };
    context.method_definition_owner = ExecutionContextTarget::ProjectGeneratedOwner {
        local_id: "group:2:2".to_string(),
        owner_kind: Some(AbiNamespaceKind::Instance),
    };

    let error = validate_execution_contexts("rspec-ruby", &call, &[context.clone()])
        .expect_err("project-generated context without project metadata must fail closed");
    assert!(error.contains("ProjectContext"), "got: {error}");

    call.project = rust_isolation_probe_context("file:///workspace/project").project;
    validate_execution_contexts("rspec-ruby", &call, &[context])
        .expect("project-generated context with matching declaration must validate");
}

#[test]
fn semantic_mixin_target_is_exclusive_with_ruby_namespace_target() {
    let zero = SourcePosition {
        line: 0,
        character: 0,
    };
    let patch = |mixin: Vec<String>, mixin_target: Option<ExecutionContextTarget>| {
        IndexPatch::ApplyMixin(ruby_fast_lsp_extension_api::ApplyMixinPatch {
            namespace: Vec::new(),
            owner_target: Some(ExecutionContextTarget::ProjectGeneratedOwner {
                local_id: "consumer".to_string(),
                owner_kind: Some(AbiNamespaceKind::Instance),
            }),
            target_kind: AbiNamespaceKind::Instance,
            mixin_target,
            mixin,
            absolute: false,
            kind: ruby_fast_lsp_extension_api::MixinKind::Include,
            location: SourceRange {
                start: zero,
                end: zero,
            },
            source: ruby_fast_lsp_extension_api::PatchSource {
                extension_id: "test".to_string(),
                macro_name: "include_context".to_string(),
            },
        })
    };
    let semantic_target = Some(ExecutionContextTarget::ProjectGeneratedOwner {
        local_id: "shared".to_string(),
        owner_kind: Some(AbiNamespaceKind::Instance),
    });

    validate_index_patch_payloads(&[patch(Vec::new(), semantic_target.clone())])
        .expect("an exact generated mixin target must validate");
    let both = validate_index_patch_payloads(&[patch(vec!["Shared".to_string()], semantic_target)])
        .expect_err("mixin target representations must be mutually exclusive");
    assert!(both.contains("either `mixin` or `mixin_target`"));
    let neither = validate_index_patch_payloads(&[patch(Vec::new(), None)])
        .expect_err("a mixin patch without a target must be rejected");
    assert!(neither.contains("must provide"));
}

#[test]
fn execution_context_connection_validates_exact_targets_and_project_requirement() {
    let location = SourceRange {
        start: SourcePosition {
            line: 4,
            character: 2,
        },
        end: SourcePosition {
            line: 4,
            character: 29,
        },
    };
    let patch = IndexPatch::ConnectExecutionContext(
        ruby_fast_lsp_extension_api::ConnectExecutionContextPatch {
            template: ExecutionContextTarget::ProjectGeneratedOwner {
                local_id: "shared-examples-runtime:auditable".to_string(),
                owner_kind: Some(AbiNamespaceKind::Singleton),
            },
            application: ExecutionContextTarget::GeneratedOwner {
                local_id: "example-group:1:0-8:3".to_string(),
                owner_kind: Some(AbiNamespaceKind::Instance),
            },
            location,
            source: ruby_fast_lsp_extension_api::PatchSource {
                extension_id: "rspec-ruby".to_string(),
                macro_name: "it_behaves_like".to_string(),
            },
        },
    );

    validate_index_patch_payloads(std::slice::from_ref(&patch))
        .expect("valid exact execution-context targets must pass the guest boundary");
    assert!(
        index_patch_requires_project_context(&patch),
        "a project-generated execution template must fail closed without ProjectContext"
    );

    let mut invalid = patch;
    let IndexPatch::ConnectExecutionContext(connection) = &mut invalid else {
        panic!(
            "INVARIANT VIOLATED: connection fixture changed variant. This is a test bug because target mutation requires ConnectExecutionContext. Fix: preserve the fixture variant."
        );
    };
    connection.application = ExecutionContextTarget::GeneratedOwner {
        local_id: "".to_string(),
        owner_kind: Some(AbiNamespaceKind::Instance),
    };
    assert!(validate_index_patch_payloads(&[invalid])
        .expect_err("empty generated application identity must be rejected")
        .contains("invalid execution context application generated owner"));
}
