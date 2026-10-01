use super::*;
use ruby_fast_lsp_extension_api::{
    Argument, ArgumentValue, CalleeResolution, DocumentContext, ExecutionContextTarget, LockedGem,
    LockedGemSource, NamespaceKind, ProjectContext, ProjectSourceKind, Receiver, ResolvedCall,
    ResolvedCallee, SourcePosition, SourceRange,
};

#[test]
fn wasm_extension_returns_call_names_and_patches() {
    let wasm = wat::parse_str(test_extension_wat()).unwrap();
    let mut ext = WasmExtension::from_bytes("test", &wasm).unwrap();

    assert_eq!(ext.abi_version().unwrap(), ABI_VERSION);
    assert_eq!(ext.indexed_call_names(), &["let".to_string()]);

    let ctx = let_context();
    let patches = ext.index_call(&ctx).unwrap();
    assert_eq!(patches.len(), 1);
}

#[test]
fn compiled_wasm_module_instantiates_independent_project_guests() {
    let wasm = wat::parse_str(test_extension_wat()).unwrap();
    let compiled = CompiledWasmExtension::from_bytes(&wasm).unwrap();
    let mut first = WasmExtension::from_compiled("test", compiled.clone()).unwrap();
    let mut second = WasmExtension::from_compiled("test", compiled).unwrap();

    first.memory.write(&mut first.store, 0, &[42]).unwrap();
    let mut second_byte = [0];
    second
        .memory
        .read(&second.store, 0, &mut second_byte)
        .unwrap();
    assert_eq!(
        second_byte,
        [0],
        "shared compiled code must not share mutable guest memory across project instances"
    );
    assert_eq!(first.index_call(&let_context()).unwrap().len(), 1);
    assert_eq!(second.index_call(&let_context()).unwrap().len(), 1);
    assert_eq!(first.abi_version().unwrap(), ABI_VERSION);
    assert_eq!(second.abi_version().unwrap(), ABI_VERSION);
}

#[test]
fn serialized_compiled_module_round_trips_under_exact_engine_identity() {
    let wasm = wat::parse_str(test_extension_wat()).unwrap();
    let compiler = WasmExtensionCompiler::new().unwrap();
    let identity = compiler.cache_identity();
    let (compiled, serialized) = compiler.compile_and_serialize(&wasm).unwrap();
    let mut first = WasmExtension::from_compiled("first", compiled).unwrap();
    assert_eq!(first.index_call(&let_context()).unwrap().len(), 1);
    drop(first);

    let restoring_compiler = WasmExtensionCompiler::new().unwrap();
    assert_eq!(restoring_compiler.cache_identity(), identity);
    // SAFETY: `serialized` is the byte-exact output of
    // `compile_and_serialize` above and has not crossed a trust boundary.
    let restored = unsafe {
        restoring_compiler
            .deserialize_verified(&serialized)
            .unwrap()
    };
    let mut second = WasmExtension::from_compiled("second", restored).unwrap();
    assert_eq!(second.abi_version().unwrap(), ABI_VERSION);
    assert_eq!(second.index_call(&let_context()).unwrap().len(), 1);
}

#[test]
fn wasm_extension_handle_event_returns_output() {
    let wasm = wat::parse_str(test_extension_event_wat()).unwrap();
    let mut ext = WasmExtension::from_bytes("test", &wasm).unwrap();

    let output = ext
        .handle_event(&ExtensionEvent {
            event: "index.call.enter".to_string(),
            call: Some(let_context()),
            document: None,
            project: None,
            settings: None,
            files: None,
            process_results: None,
        })
        .unwrap();
    assert_eq!(output.index_patches.len(), 1);
    assert_eq!(output.response_patches.len(), 0);
    assert_eq!(output.command_patches.len(), 0);

    let patches = ext.index_call(&let_context()).unwrap();
    assert_eq!(patches.len(), 1);
}

#[test]
fn rspec_ruby_mruby_wasm_extension_works() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../extensions/rspec-ruby/target/wasm32-wasip1/release/rspec-ruby.wasm");
    if !path.exists() {
        eprintln!(
                "skipping real mruby Wasm test; build first with extensions/rspec-ruby/scripts/build-wasm-docker.sh"
            );
        return;
    }

    let mut ext = WasmExtension::from_file("rspec-ruby", &path).unwrap();
    assert_eq!(ext.abi_version().unwrap(), ABI_VERSION);
    assert_eq!(
        ext.indexed_call_names(),
        &[
            "shared_context".to_string(),
            "shared_examples".to_string(),
            "shared_examples_for".to_string(),
            "include_context".to_string(),
            "include_examples".to_string(),
            "it_behaves_like".to_string(),
            "it_should_behave_like".to_string(),
            "describe".to_string(),
            "context".to_string(),
            "it".to_string(),
            "example".to_string(),
            "specify".to_string(),
            "before".to_string(),
            "after".to_string(),
            "around".to_string(),
            "let".to_string(),
            "let!".to_string(),
            "subject".to_string(),
            "subject!".to_string(),
            "include".to_string(),
            "prepend".to_string(),
            "extend".to_string()
        ]
    );

    let patches = ext.index_call(&let_context()).unwrap();
    assert_eq!(patches.len(), 2);
    let method = patches
        .iter()
        .find_map(|patch| match patch {
            IndexPatch::DefineMethod(method) if method.name == "user" => Some(method),
            IndexPatch::DefineNamespace(_)
            | IndexPatch::DefineConstant(_)
            | IndexPatch::AddReference(_)
            | IndexPatch::DefineMethod(_)
            | IndexPatch::SetSuperclass(_)
            | IndexPatch::ApplyMixin(_)
            | IndexPatch::ConnectExecutionContext(_) => None,
        })
        .expect(
            "INVARIANT VIOLATED: rspec let did not emit user helper DefineMethod. \
                 This is a bug because let(:user) must define a generated helper method. \
                 Fix: keep rspec-ruby let handler mapped to DefineMethod.",
        );
    assert_eq!(method.name, "user");
    assert_eq!(method.namespace, &["User".to_string()]);
    assert_eq!(method.source.extension_id, "rspec-ruby");
    assert_eq!(method.source.macro_name, "let");
    assert_eq!(
        method.return_type_source,
        Some(ruby_fast_lsp_extension_api::MethodReturnTypeSource::Block)
    );

    let output = ext.index_call_output(&root_describe_context()).expect(
            "INVARIANT VIOLATED: actual RSpec Wasm failed to return its execution context. This is a bug because the bundled artifact must exercise the same public event contract as the Ruby source. Fix: rebuild the mruby Wasm after SDK or guest changes.",
        );
    let context = output.execution_contexts.first().expect(
            "INVARIANT VIOLATED: actual RSpec Wasm omitted the describe execution context. This is a bug because source-only tests cannot prove packaged generated-owner behavior. Fix: keep handle_event returning index_call_output.",
        );
    assert!(matches!(
        context.implicit_receiver,
        ExecutionContextTarget::GeneratedOwner {
            owner_kind: Some(NamespaceKind::Singleton),
            ..
        }
    ));
    assert!(matches!(
        context.method_definition_owner,
        ExecutionContextTarget::GeneratedOwner {
            owner_kind: Some(NamespaceKind::Instance),
            ..
        }
    ));

    let shared_output = ext.index_call_output(&root_shared_context()).expect(
            "INVARIANT VIOLATED: actual RSpec Wasm failed to return its project-scoped shared context. This is a packaged guest bug because shared contexts require stable cross-file identity. Fix: rebuild the mruby Wasm after SDK or guest changes.",
        );
    let shared = shared_output
        .execution_contexts
        .first()
        .expect("actual RSpec Wasm must emit a shared_context execution owner");
    assert_eq!(
        shared.generated_owners[0].scope,
        ruby_fast_lsp_extension_api::GeneratedOwnerScope::Project
    );
    assert_eq!(
        shared.method_definition_owner,
        ExecutionContextTarget::ProjectGeneratedOwner {
            local_id: "shared-context:authenticated".to_string(),
            owner_kind: None,
        }
    );

    for _ in 0..64 {
        let repeated = ext.index_call(&let_context()).expect(
                "INVARIANT VIOLATED: repeated mruby calls corrupted guest allocation state. This is a bug because the host owns and frees every returned output buffer. Fix: clear the shim's retained output pointer when dealloc receives it.",
            );
        assert_eq!(repeated.len(), 2);
    }
}

#[test]
fn rspec_ruby_mruby_wasm_document_events_work() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../extensions/rspec-ruby/target/wasm32-wasip1/release/rspec-ruby.wasm");
    if !path.exists() {
        eprintln!(
                "skipping real mruby Wasm test; build first with extensions/rspec-ruby/scripts/build-wasm-docker.sh"
            );
        return;
    }

    let mut ext = WasmExtension::from_file("rspec-ruby", &path).unwrap();
    let output = ext
        .handle_event(&ExtensionEvent {
            event: "request.document_symbol".to_string(),
            call: None,
            document: Some(DocumentContext {
                uri: "file:///spec/user_spec.rb".to_string(),
                text: "\nRSpec.describe User do\n  it \"returns name\" do\n  end\nend\n"
                    .to_string(),
                project: None,
            }),
            project: None,
            settings: None,
            files: None,
            process_results: None,
        })
        .unwrap();

    assert_eq!(output.response_patches.len(), 2);

    let repeated = ext
            .handle_event(&ExtensionEvent {
                event: "request.document_symbol".to_string(),
                call: None,
                document: Some(DocumentContext {
                    uri: "file:///spec/other_spec.rb".to_string(),
                    text: "RSpec.describe Other do\nend\n".to_string(),
                    project: None,
                }),
                project: None,
                settings: None,
                files: None,
                process_results: None,
            })
            .expect(
                "INVARIANT VIOLATED: repeated mruby events corrupted guest allocation state. This is a bug because response hooks execute for every matching document. Fix: keep host/guest output ownership single-sourced.",
            );
    assert_eq!(repeated.response_patches.len(), 1);
}

#[test]
fn typed_rust_wasm_guest_returns_execution_context_and_generated_method() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(
            "../../extensions/example-rust/target/wasm32-wasip1/release/ruby_fast_lsp_example_rust_extension.wasm",
        );
    if !path.exists() {
        eprintln!(
                "skipping real Rust Wasm test; build first with the command in extensions/example-rust/README.md"
            );
        return;
    }

    let mut extension = WasmExtension::from_file("example-rust", &path).expect(
            "INVARIANT VIOLATED: typed Rust SDK artifact failed to load through Wasmtime. This is an SDK bug because generated exports must match the public host ABI. Fix: keep export_extension! synchronized with WasmExtension.",
        );
    assert_eq!(
        extension.indexed_call_names(),
        &[
            "scope".to_string(),
            "property".to_string(),
            "isolation_probe".to_string()
        ]
    );

    let scope = example_scope_context();
    let scope_output = extension.index_call_output(&scope).expect(
            "INVARIANT VIOLATED: typed Rust guest failed to return scope output. This is an SDK bug because handle_event must decode and encode ExtensionOutput. Fix: keep typed event dispatch synchronized with extension-api.",
        );
    assert_eq!(scope_output.execution_contexts.len(), 1);

    let property_output = extension
            .index_call_output(&example_property_context(&scope))
            .expect(
                "INVARIANT VIOLATED: typed Rust guest failed to return property output. This is an SDK bug because nested CallContext values must survive Wasm serialization. Fix: preserve enclosing calls in typed decoding.",
            );
    assert_eq!(property_output.index_patches.len(), 1);
}

#[test]
fn abi_mismatch_is_recoverable_error() {
    let wasm = wat::parse_str(
        r#"
            (module
              (memory (export "memory") 1)
              (func (export "alloc") (param $len i32) (result i32)
                i32.const 4096)
              (func (export "dealloc") (param $ptr i32) (param $len i32))
              (func (export "abi_version") (result i32)
                i32.const 999)
              (func (export "indexed_call_names") (result i64)
                i64.const 0)
              (func (export "index_call") (param $ptr i32) (param $len i32) (result i64)
                i64.const 0)
            )
            "#,
    )
    .unwrap();

    let err = match WasmExtension::from_bytes("bad-abi", &wasm) {
            Ok(_) => panic!(
                "INVARIANT VIOLATED: ABI mismatch loaded successfully. \
                 This is a bug because bad external extensions must not cross the host ABI boundary. \
                 Fix: keep ABI validation before returning WasmExtension."
            ),
            Err(err) => err,
        };
    assert!(
        err.to_string().contains("ABI version"),
        "INVARIANT VIOLATED: ABI mismatch did not return a clear error. \
             This is a bug because bad external extensions must not panic the server. \
             Fix: keep ABI validation on the recoverable error path."
    );
}

#[test]
fn oversized_output_is_recoverable_error() {
    let wasm = wat::parse_str(test_extension_wat()).unwrap();
    let mut ext = WasmExtension::from_bytes_with_config(
        "test",
        &wasm,
        WasmExtensionConfig {
            max_output_bytes: 8,
            ..WasmExtensionConfig::default()
        },
    )
    .unwrap();
    let err = ext.index_call(&let_context()).unwrap_err();
    assert!(
        err.to_string().contains("output payload"),
        "INVARIANT VIOLATED: oversized extension output did not return a clear error. \
             This is a bug because bad external extensions must be disabled without crashing. \
            Fix: keep output size validation on the recoverable error path."
    );
}

#[test]
fn oversized_input_is_recoverable_error() {
    let wasm = wat::parse_str(test_extension_wat()).unwrap();
    let mut ext = WasmExtension::from_bytes_with_config(
        "test",
        &wasm,
        WasmExtensionConfig {
            max_input_bytes: 8,
            ..WasmExtensionConfig::default()
        },
    )
    .unwrap();
    let err = ext.index_call(&let_context()).unwrap_err();
    assert!(
        err.to_string().contains("input payload"),
        "INVARIANT VIOLATED: oversized extension input did not return a clear error. \
             This is a bug because host payload budgets must fail before guest execution. \
             Fix: keep input size validation before alloc/call."
    );
}

#[test]
fn fuel_exhaustion_is_recoverable_error() {
    let wasm = wat::parse_str(fuel_hog_extension_wat()).unwrap();
    let mut ext = WasmExtension::from_bytes_with_config(
        "fuel-hog",
        &wasm,
        WasmExtensionConfig {
            fuel_per_call: 10_000,
            ..WasmExtensionConfig::default()
        },
    )
    .unwrap();
    let err = ext.index_call(&let_context()).unwrap_err();
    assert!(
        err.to_string().contains("fuel"),
        "INVARIANT VIOLATED: fuel exhaustion did not return a clear recoverable error: {err}. \
             This is a bug because runaway extensions must not freeze indexing. \
             Fix: keep consume_fuel enabled and refuel each guest call."
    );
}

#[test]
fn wall_clock_deadline_interrupts_runaway_guest() {
    let wasm = wat::parse_str(fuel_hog_extension_wat()).unwrap();
    let mut ext = WasmExtension::from_bytes_with_config(
        "deadline-hog",
        &wasm,
        WasmExtensionConfig {
            fuel_per_call: 1_000_000_000,
            wall_timeout: std::time::Duration::from_millis(10),
            ..WasmExtensionConfig::default()
        },
    )
    .unwrap();

    let started = std::time::Instant::now();
    let error = ext.index_call(&let_context()).unwrap_err();

    assert!(
        error.to_string().contains("wall-clock deadline"),
        "expected wall-clock deadline error, got: {error:#}"
    );
    assert!(started.elapsed() < std::time::Duration::from_secs(1));
}

#[test]
fn memory_growth_limit_is_recoverable_error() {
    let wasm = wat::parse_str(memory_hog_extension_wat()).unwrap();
    let mut ext = WasmExtension::from_bytes_with_config(
        "memory-hog",
        &wasm,
        WasmExtensionConfig {
            max_memory_bytes: 64 * 1024,
            ..WasmExtensionConfig::default()
        },
    )
    .unwrap();
    let err = ext.index_call(&let_context()).unwrap_err();
    assert!(
        err.to_string().contains("memory") || err.to_string().contains("grow"),
        "INVARIANT VIOLATED: memory growth limit did not return a clear recoverable error: {err}. \
             This is a bug because memory budgets must stop extension heap growth. \
             Fix: keep StoreLimits memory_size + trap_on_grow_failure wired."
    );
}

fn let_context() -> CallContext {
    CallContext {
        project: None,
        method_name: "let".to_string(),
        receiver: Receiver::None,
        arguments: vec![Argument {
            keyword: None,
            value: ArgumentValue::Symbol("user".to_string()),
            range: range(),
        }],
        current_namespace: vec!["User".to_string()],
        namespace_kind: NamespaceKind::Singleton,
        call_range: range(),
        block_range: Some(range()),
        message_range: range(),
        resolved_callees: Vec::new(),
        enclosing_calls: vec![ruby_fast_lsp_extension_api::ResolvedCall {
            method_name: "describe".to_string(),
            receiver: Receiver::Constant(vec!["RSpec".to_string()]),
            arguments: Vec::new(),
            resolved_callees: vec![ruby_fast_lsp_extension_api::ResolvedCallee {
                owner: vec!["RSpec".to_string()],
                owner_kind: NamespaceKind::Singleton,
                method: "describe".to_string(),
                resolution: ruby_fast_lsp_extension_api::CalleeResolution::ReceiverOnly,
            }],
            call_range: range(),
            message_range: range(),
            frame_extension_ids: vec!["rspec-ruby".to_string()],
        }],
    }
}

fn root_describe_context() -> CallContext {
    CallContext {
        project: None,
        method_name: "describe".to_string(),
        receiver: Receiver::Constant(vec!["RSpec".to_string()]),
        arguments: Vec::new(),
        current_namespace: vec!["Lexical".to_string()],
        namespace_kind: NamespaceKind::Singleton,
        call_range: range(),
        block_range: Some(range()),
        message_range: range(),
        resolved_callees: vec![ruby_fast_lsp_extension_api::ResolvedCallee {
            owner: vec!["RSpec".to_string()],
            owner_kind: NamespaceKind::Singleton,
            method: "describe".to_string(),
            resolution: ruby_fast_lsp_extension_api::CalleeResolution::Exact,
        }],
        enclosing_calls: Vec::new(),
    }
}

fn root_shared_context() -> CallContext {
    let mut context = root_describe_context();
    context.project = Some(ProjectContext {
        project_uri: "file:///workspace".to_string(),
        source_uri: "file:///workspace/spec/support/shared.rb".to_string(),
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
    });
    context.method_name = "shared_context".to_string();
    context.arguments = vec![Argument {
        keyword: None,
        value: ArgumentValue::String("authenticated".to_string()),
        range: range(),
    }];
    context.resolved_callees = vec![ResolvedCallee {
        owner: vec!["RSpec".to_string()],
        owner_kind: NamespaceKind::Singleton,
        method: "shared_context".to_string(),
        resolution: CalleeResolution::Exact,
    }];
    context
}

fn example_scope_context() -> CallContext {
    CallContext {
        project: Some(ProjectContext {
            project_uri: "file:///workspace".to_string(),
            source_uri: "file:///workspace/example.rb".to_string(),
            source_kind: ProjectSourceKind::Project,
            workspace_trusted: true,
            ruby_version: Some("3.3".to_string()),
            lockfile_present: true,
            locked_gems_complete: true,
            locked_gems: vec![LockedGem {
                name: "example-framework".to_string(),
                version: "1.0.0".to_string(),
                source: LockedGemSource::Registry,
            }],
        }),
        method_name: "scope".to_string(),
        receiver: Receiver::Constant(vec!["ExampleDsl".to_string()]),
        arguments: Vec::new(),
        current_namespace: Vec::new(),
        namespace_kind: NamespaceKind::Instance,
        call_range: range(),
        block_range: Some(range()),
        message_range: range(),
        resolved_callees: vec![ResolvedCallee {
            owner: vec!["ExampleDsl".to_string()],
            owner_kind: NamespaceKind::Singleton,
            method: "scope".to_string(),
            resolution: CalleeResolution::Exact,
        }],
        enclosing_calls: Vec::new(),
    }
}

fn example_property_context(scope: &CallContext) -> CallContext {
    CallContext {
        project: scope.project.clone(),
        method_name: "property".to_string(),
        receiver: Receiver::None,
        arguments: vec![Argument {
            keyword: None,
            value: ArgumentValue::Symbol("generated_name".to_string()),
            range: range(),
        }],
        current_namespace: Vec::new(),
        namespace_kind: NamespaceKind::Instance,
        call_range: range(),
        block_range: None,
        message_range: range(),
        resolved_callees: Vec::new(),
        enclosing_calls: vec![ResolvedCall {
            method_name: scope.method_name.clone(),
            receiver: scope.receiver.clone(),
            arguments: scope.arguments.clone(),
            resolved_callees: scope.resolved_callees.clone(),
            call_range: scope.call_range,
            message_range: scope.message_range,
            frame_extension_ids: vec!["rspec-ruby".to_string()],
        }],
    }
}

fn range() -> SourceRange {
    SourceRange {
        start: SourcePosition {
            line: 2,
            character: 6,
        },
        end: SourcePosition {
            line: 2,
            character: 11,
        },
    }
}

fn test_extension_wat() -> &'static str {
    r#"
        (module
          (memory (export "memory") 1)
          (data (i32.const 1024) "[\"let\"]")
          (data (i32.const 2048) "[{\"DefineMethod\":{\"name\":\"user\",\"namespace\":[\"User\"],\"owner_kind\":\"Instance\",\"visibility\":\"Public\",\"location\":{\"start\":{\"line\":2,\"character\":6},\"end\":{\"line\":2,\"character\":11}},\"return_type\":null,\"source\":{\"extension_id\":\"test\",\"macro_name\":\"let\"}}}]")

          (func (export "alloc") (param $len i32) (result i32)
            i32.const 4096)

          (func (export "dealloc") (param $ptr i32) (param $len i32))

          (func (export "abi_version") (result i32)
            i32.const 1)

          (func (export "indexed_call_names") (result i64)
            (i64.or
              (i64.shl
                (i64.extend_i32_u (i32.const 1024))
                (i64.const 32))
              (i64.extend_i32_u (i32.const 7))))

          (func (export "index_call") (param $ptr i32) (param $len i32) (result i64)
            (i64.or
              (i64.shl
                (i64.extend_i32_u (i32.const 2048))
                (i64.const 32))
              (i64.extend_i32_u (i32.const 250))))
        )
        "#
}

fn test_extension_event_wat() -> &'static str {
    r#"
        (module
          (memory (export "memory") 1)
          (data (i32.const 1024) "[\"let\"]")
          (data (i32.const 2048) "{\"index_patches\":[{\"DefineMethod\":{\"name\":\"user\",\"namespace\":[\"User\"],\"owner_kind\":\"Instance\",\"visibility\":\"Public\",\"location\":{\"start\":{\"line\":2,\"character\":6},\"end\":{\"line\":2,\"character\":11}},\"return_type\":null,\"source\":{\"extension_id\":\"test\",\"macro_name\":\"let\"}}}],\"response_patches\":[],\"command_patches\":[]}")

          (func (export "alloc") (param $len i32) (result i32)
            i32.const 4096)

          (func (export "dealloc") (param $ptr i32) (param $len i32))

          (func (export "abi_version") (result i32)
            i32.const 1)

          (func (export "indexed_call_names") (result i64)
            (i64.or
              (i64.shl
                (i64.extend_i32_u (i32.const 1024))
                (i64.const 32))
              (i64.extend_i32_u (i32.const 7))))

          (func (export "index_call") (param $ptr i32) (param $len i32) (result i64)
            (i64.const 0))

          (func (export "handle_event") (param $ptr i32) (param $len i32) (result i64)
            (i64.or
              (i64.shl
                (i64.extend_i32_u (i32.const 2048))
                (i64.const 32))
              (i64.extend_i32_u (i32.const 311))))
        )
        "#
}

fn fuel_hog_extension_wat() -> &'static str {
    r#"
        (module
          (memory (export "memory") 1)
          (data (i32.const 1024) "[\"let\"]")

          (func (export "alloc") (param $len i32) (result i32)
            i32.const 4096)

          (func (export "dealloc") (param $ptr i32) (param $len i32))

          (func (export "abi_version") (result i32)
            i32.const 1)

          (func (export "indexed_call_names") (result i64)
            (i64.or
              (i64.shl
                (i64.extend_i32_u (i32.const 1024))
                (i64.const 32))
              (i64.extend_i32_u (i32.const 7))))

          (func (export "index_call") (param $ptr i32) (param $len i32) (result i64)
            (loop $again
              br $again)
            i64.const 0)
        )
        "#
}

fn memory_hog_extension_wat() -> &'static str {
    r#"
        (module
          (memory (export "memory") 1)
          (data (i32.const 1024) "[\"let\"]")

          (func (export "alloc") (param $len i32) (result i32)
            i32.const 4096)

          (func (export "dealloc") (param $ptr i32) (param $len i32))

          (func (export "abi_version") (result i32)
            i32.const 1)

          (func (export "indexed_call_names") (result i64)
            (i64.or
              (i64.shl
                (i64.extend_i32_u (i32.const 1024))
                (i64.const 32))
              (i64.extend_i32_u (i32.const 7))))

          (func (export "index_call") (param $ptr i32) (param $len i32) (result i64)
            i32.const 1
            memory.grow
            drop
            i64.const 0)
        )
        "#
}
