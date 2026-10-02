use std::fs;

use tempfile::TempDir;
use tower_lsp::lsp_types::{
    DidChangeWatchedFilesParams, DidChangeWorkspaceFoldersParams, FileChangeType, FileEvent,
    InitializeParams, Url, WorkspaceFolder, WorkspaceFoldersChangeEvent,
};
use tower_lsp::LanguageServer;

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use ruby_analysis::core::{
    FullyQualifiedName, GraphNodeKind, NamespaceKind, RubyConstant, RubyMethod, SourceKind,
};
use ruby_fast_lsp_extension_api::{
    BlockExecutionContextPatch, CallContext, ExecutionContextTarget, IndexPatch,
    NamespaceKind as AbiNamespaceKind, ProcessRequest, ProcessResultStatus, Receiver,
    ResponsePatch, SourcePosition, SourceRange, WatchedFileChangeKind,
};

use crate::environment::config::RubyFastLspConfig;
use crate::environment::extensions::loading::config::{
    ConfiguredExtensionPath, ExtensionLoadConfig, ExtensionPathSource,
};
use crate::environment::extensions::loading::manifest::{
    build_watched_file_matcher, parse_manifest_method_targets, parse_manifest_namespace_targets,
    validate_manifest, ExtensionManifest, ExtensionMethodTarget, ExtensionNamespaceTarget,
    ExtensionProjectContextDelivery,
};
use crate::environment::extensions::loading::packages::load_wasm_extensions;
use crate::environment::extensions::loading::wasm::read_extension_wasm;
use crate::environment::extensions::patches::conflicts::{
    resolve_execution_context_conflicts, resolve_index_patch_conflicts,
};
use crate::environment::extensions::patches::types::{
    analysis_ruby_type_from_extension, extension_ruby_types_semantically_equal,
};
use crate::environment::extensions::patches::validation::{
    index_patch_extension_id, index_patch_requires_project_context, validate_execution_contexts,
    validate_index_patch_payloads, validate_index_patch_provenance,
    validate_response_patch_provenance,
};
use crate::environment::extensions::processes::{
    normalized_path, run_extension_process, validate_extension_process_request,
    validate_extension_reindex_files, watched_file_candidates,
};
use crate::environment::extensions::registry::handle::ExtensionRegistryHandle;
use crate::environment::extensions::registry::loaded::guest_call_context;
use crate::environment::extensions::registry::state::{
    extension_applicability_fingerprint, ExtensionRegistry,
};
use crate::environment::extensions::registry::status::{
    ExtensionStatus, ExtensionTelemetry, GuestCallKind,
};
use crate::environment::extensions::responses::response_patch_to_document_symbol;
use crate::environment::extensions::{ProjectContextSeed, MAX_EXTENSION_WASM_BYTES};
use crate::server::RubyLanguageServer;
use crate::utils::admission::{
    IndexingResourceGovernor, IndexingResourcePriority, IndexingWorkSpec,
};
use crate::utils::persistent_cache::{
    CompiledWasmProductKey, PersistentCompiledWasmLookup, PersistentDerivedProductCache,
};

mod execution_contexts;
mod lifecycle;
mod manifest;
mod patches;
mod processes;

fn copy_rspec_package(destination: &Path, version: &str) {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("extensions/rspec-ruby");
    let wasm_relative = Path::new("target/wasm32-wasip1/release/rspec-ruby.wasm");
    fs::create_dir_all(destination.join(wasm_relative).parent().unwrap())
        .expect("test package wasm directory must be created");
    let manifest = fs::read_to_string(source.join("extension.toml"))
        .expect("bundled RSpec manifest must be readable")
        .replace("version = \"0.1.0\"", &format!("version = \"{version}\""));
    fs::write(destination.join("extension.toml"), manifest).expect("test manifest must be written");
    fs::copy(source.join(wasm_relative), destination.join(wasm_relative))
        .expect("bundled RSpec wasm must be copied");
}

fn write_cacheable_extension_package(destination: &Path) -> Vec<u8> {
    fs::create_dir_all(destination).expect("test package directory must be created");
    let wasm = wat::parse_str(
        r#"
            (module
              (memory (export "memory") 1)
              (data (i32.const 1024) "[]")
              (data (i32.const 2048) "{\22index_patches\22:[],\22response_patches\22:[],\22command_patches\22:[]}")
              (func (export "alloc") (param $len i32) (result i32)
                i32.const 4096)
              (func (export "dealloc") (param $ptr i32) (param $len i32))
              (func (export "abi_version") (result i32)
                i32.const 1)
              (func (export "indexed_call_names") (result i64)
                i64.const 4398046511106)
              (func (export "index_call") (param $ptr i32) (param $len i32) (result i64)
                i64.const 4398046511106)
              (func (export "handle_event") (param $ptr i32) (param $len i32) (result i64)
                i64.const 8796093022271)
            )
            "#,
    )
    .expect("test cacheable Wasm must compile");
    fs::write(destination.join("extension.wasm"), &wasm)
        .expect("test cacheable Wasm must be written");
    fs::write(
        destination.join("extension.toml"),
        r#"
id = "cacheable-extension"
name = "Cacheable Extension"
version = "0.1.0"
abi_version = 1
server_version = ">=0.2.0, <0.4.0"
runtime = "mruby-wasm"
wasm = "extension.wasm"
capabilities = []
permissions = []

[indexing]
call_names = []
"#,
    )
    .expect("test cacheable manifest must be written");
    wasm
}

fn write_activation_failure_package(destination: &Path) {
    fs::create_dir_all(destination).expect("test package directory must be created");
    let wasm = wat::parse_str(
        r#"
            (module
              (memory (export "memory") 1)
              (data (i32.const 1024) "[]")
              (func (export "alloc") (param $len i32) (result i32)
                i32.const 4096)
              (func (export "dealloc") (param $ptr i32) (param $len i32))
              (func (export "abi_version") (result i32)
                i32.const 1)
              (func (export "indexed_call_names") (result i64)
                i64.const 4398046511106)
              (func (export "index_call") (param $ptr i32) (param $len i32) (result i64)
                i64.const 4398046511106)
              (func (export "handle_event") (param $ptr i32) (param $len i32) (result i64)
                i64.const 0)
            )
            "#,
    )
    .expect("test lifecycle Wasm must compile");
    fs::write(destination.join("extension.wasm"), wasm)
        .expect("test lifecycle Wasm must be written");
    fs::write(
        destination.join("extension.toml"),
        r#"
id = "activation-failure"
name = "Activation Failure"
version = "0.1.0"
abi_version = 1
server_version = ">=0.2.0, <0.4.0"
runtime = "mruby-wasm"
wasm = "extension.wasm"
capabilities = []
permissions = []

[indexing]
call_names = []
"#,
    )
    .expect("test lifecycle manifest must be written");
}

fn write_resource_limit_failure_package(destination: &Path) {
    fs::create_dir_all(destination).expect("test package directory must be created");
    let wasm = wat::parse_str(
        r#"
            (module
              (memory (export "memory") 1)
              (data (i32.const 1024) "[]")
              (func (export "alloc") (param $len i32) (result i32)
                i32.const 4096)
              (func (export "dealloc") (param $ptr i32) (param $len i32))
              (func (export "abi_version") (result i32)
                i32.const 1)
              (func (export "indexed_call_names") (result i64)
                i64.const 4398046511106)
              (func (export "index_call") (param $ptr i32) (param $len i32) (result i64)
                i64.const 4398046511106)
              (func (export "handle_event") (param $ptr i32) (param $len i32) (result i64)
                i64.const 262145)
            )
            "#,
    )
    .expect("test resource-limit Wasm must compile");
    fs::write(destination.join("extension.wasm"), wasm)
        .expect("test resource-limit Wasm must be written");
    fs::write(
        destination.join("extension.toml"),
        r#"
id = "resource-limit-failure"
name = "Resource Limit Failure"
version = "0.1.0"
abi_version = 1
server_version = ">=0.2.0, <0.4.0"
runtime = "wasm"
wasm = "extension.wasm"
capabilities = []
permissions = []

[indexing]
call_names = []
"#,
    )
    .expect("test resource-limit manifest must be written");
}

fn write_trap_failure_package(destination: &Path) {
    fs::create_dir_all(destination).expect("test package directory must be created");
    let wasm = wat::parse_str(
        r#"
            (module
              (memory (export "memory") 1)
              (data (i32.const 1024) "[]")
              (func (export "alloc") (param $len i32) (result i32)
                i32.const 4096)
              (func (export "dealloc") (param $ptr i32) (param $len i32))
              (func (export "abi_version") (result i32)
                i32.const 1)
              (func (export "indexed_call_names") (result i64)
                i64.const 4398046511106)
              (func (export "index_call") (param $ptr i32) (param $len i32) (result i64)
                i64.const 4398046511106)
              (func (export "handle_event") (param $ptr i32) (param $len i32) (result i64)
                unreachable)
            )
            "#,
    )
    .expect("test trap Wasm must compile");
    fs::write(destination.join("extension.wasm"), wasm).expect("test trap Wasm must be written");
    fs::write(
        destination.join("extension.toml"),
        r#"
id = "trap-failure"
name = "Trap Failure"
version = "0.1.0"
abi_version = 1
server_version = ">=0.2.0, <0.4.0"
runtime = "wasm"
wasm = "extension.wasm"
capabilities = []
permissions = []

[indexing]
call_names = []
"#,
    )
    .expect("test trap manifest must be written");
}

fn write_settings_failure_package(destination: &Path) {
    fs::create_dir_all(destination).expect("test package directory must be created");
    let wasm = wat::parse_str(
        r#"
            (module
              (memory (export "memory") 1)
              (data (i32.const 1024) "[]")
              (data (i32.const 2048) "{\"index_patches\":[],\"response_patches\":[],\"command_patches\":[]}")
              (func (export "alloc") (param $len i32) (result i32)
                i32.const 4096)
              (func (export "dealloc") (param $ptr i32) (param $len i32))
              (func (export "abi_version") (result i32)
                i32.const 1)
              (func (export "indexed_call_names") (result i64)
                i64.const 4398046511106)
              (func (export "index_call") (param $ptr i32) (param $len i32) (result i64)
                i64.const 4398046511106)
              (func (export "handle_event") (param $ptr i32) (param $len i32) (result i64)
                local.get $ptr
                i32.const 10
                i32.add
                i32.load8_u
                i32.const 115
                i32.eq
                if (result i64)
                  i64.const 0
                else
                  i64.const 8796093022271
                end)
            )
            "#,
    )
    .expect("test settings Wasm must compile");
    fs::write(destination.join("extension.wasm"), wasm)
        .expect("test settings Wasm must be written");
    fs::write(
        destination.join("extension.toml"),
        r#"
id = "settings-failure"
name = "Settings Failure"
version = "0.1.0"
abi_version = 1
server_version = ">=0.2.0, <0.4.0"
runtime = "mruby-wasm"
wasm = "extension.wasm"
capabilities = []
permissions = []

[indexing]
call_names = []
"#,
    )
    .expect("test settings manifest must be written");
}

fn write_watched_file_failure_package(destination: &Path) {
    fs::create_dir_all(destination).expect("test package directory must be created");
    let wasm = wat::parse_str(
        r#"
            (module
              (memory (export "memory") 1)
              (data (i32.const 1024) "[]")
              (data (i32.const 2048) "{\"index_patches\":[],\"response_patches\":[],\"command_patches\":[]}")
              (func (export "alloc") (param $len i32) (result i32)
                i32.const 4096)
              (func (export "dealloc") (param $ptr i32) (param $len i32))
              (func (export "abi_version") (result i32)
                i32.const 1)
              (func (export "indexed_call_names") (result i64)
                i64.const 4398046511106)
              (func (export "index_call") (param $ptr i32) (param $len i32) (result i64)
                i64.const 4398046511106)
              (func (export "handle_event") (param $ptr i32) (param $len i32) (result i64)
                local.get $ptr
                i32.const 10
                i32.add
                i32.load8_u
                i32.const 102
                i32.eq
                if (result i64)
                  i64.const 0
                else
                  i64.const 8796093022271
                end)
            )
            "#,
    )
    .expect("test watched-file Wasm must compile");
    fs::write(destination.join("extension.wasm"), wasm)
        .expect("test watched-file Wasm must be written");
    fs::write(
        destination.join("extension.toml"),
        r#"
id = "watched-file-failure"
name = "Watched File Failure"
version = "0.1.0"
abi_version = 1
server_version = ">=0.2.0, <0.4.0"
runtime = "mruby-wasm"
wasm = "extension.wasm"
capabilities = ["watching"]
permissions = []

[indexing]
call_names = []

[watching]
globs = ["config/routes.rb"]
"#,
    )
    .expect("test watched-file manifest must be written");
}

fn execution_context_fixture(extension_id: &str) -> BlockExecutionContextPatch {
    let call_range = SourceRange {
        start: SourcePosition {
            line: 2,
            character: 2,
        },
        end: SourcePosition {
            line: 6,
            character: 5,
        },
    };
    let block_range = SourceRange {
        start: SourcePosition {
            line: 2,
            character: 20,
        },
        end: SourcePosition {
            line: 6,
            character: 5,
        },
    };
    BlockExecutionContextPatch {
        call_range,
        block_range,
        generated_owners: vec![ruby_fast_lsp_extension_api::GeneratedOwnerPatch {
            local_id: "group:2:2".to_string(),
            scope: ruby_fast_lsp_extension_api::GeneratedOwnerScope::Source,
            declaration_kind: ruby_fast_lsp_extension_api::NamespaceDeclarationKind::Class,
            owner_kind: AbiNamespaceKind::Instance,
            parent: None,
        }],
        implicit_receiver: ExecutionContextTarget::GeneratedOwner {
            local_id: "group:2:2".to_string(),
            owner_kind: None,
        },
        method_definition_owner: ExecutionContextTarget::GeneratedOwner {
            local_id: "group:2:2".to_string(),
            owner_kind: None,
        },
        lexical_scope: ruby_fast_lsp_extension_api::LexicalScopeMode::Preserve,
        local_scope: ruby_fast_lsp_extension_api::LocalScopeMode::Preserve,
        source: ruby_fast_lsp_extension_api::PatchSource {
            extension_id: extension_id.to_string(),
            macro_name: "describe".to_string(),
        },
    }
}

fn execution_call_fixture() -> CallContext {
    let context = execution_context_fixture("rspec-ruby");
    CallContext {
        project: None,
        method_name: "describe".to_string(),
        receiver: Receiver::Constant(vec!["RSpec".to_string()]),
        arguments: Vec::new(),
        current_namespace: vec!["Lexical".to_string()],
        namespace_kind: AbiNamespaceKind::Instance,
        call_range: context.call_range,
        block_range: Some(context.block_range),
        message_range: context.call_range,
        resolved_callees: Vec::new(),
        enclosing_calls: Vec::new(),
    }
}

fn rust_isolation_probe_context(project_uri: &str) -> CallContext {
    let mut context = execution_call_fixture();
    context.project = Some(ruby_fast_lsp_extension_api::ProjectContext {
        project_uri: project_uri.to_string(),
        source_uri: format!("{project_uri}/probe.rb"),
        source_kind: ruby_fast_lsp_extension_api::ProjectSourceKind::Project,
        workspace_trusted: true,
        ruby_version: Some("3.3".to_string()),
        lockfile_present: true,
        locked_gems_complete: true,
        locked_gems: vec![ruby_fast_lsp_extension_api::LockedGem {
            name: "example-framework".to_string(),
            version: "1.0.0".to_string(),
            source: ruby_fast_lsp_extension_api::LockedGemSource::Registry,
        }],
    });
    context.method_name = "isolation_probe".to_string();
    context.receiver = Receiver::None;
    context.block_range = None;
    context.resolved_callees.clear();
    context.enclosing_calls.clear();
    context
}
