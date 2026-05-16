use std::path::Path;

use anyhow::{anyhow, Context, Result};
use ruby_fast_lsp_extension_api::{
    CallContext, ExtensionEvent, ExtensionOutput, IndexPatch, ABI_VERSION,
};
use wasmtime::{
    Config, Engine, Linker, Memory, Module, Store, StoreLimits, StoreLimitsBuilder, TypedFunc,
};
use wasmtime_wasi::{p1, WasiCtxBuilder};

const DEFAULT_MAX_INPUT_BYTES: usize = 64 * 1024;
const DEFAULT_MAX_OUTPUT_BYTES: usize = 256 * 1024;
const DEFAULT_MAX_MEMORY_BYTES: usize = 32 * 1024 * 1024;
const DEFAULT_FUEL_PER_CALL: u64 = 100_000_000;

#[derive(Clone, Copy, Debug)]
pub struct WasmExtensionConfig {
    pub max_input_bytes: usize,
    pub max_output_bytes: usize,
    pub max_memory_bytes: usize,
    pub fuel_per_call: u64,
}

impl Default for WasmExtensionConfig {
    fn default() -> Self {
        Self {
            max_input_bytes: DEFAULT_MAX_INPUT_BYTES,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            max_memory_bytes: DEFAULT_MAX_MEMORY_BYTES,
            fuel_per_call: DEFAULT_FUEL_PER_CALL,
        }
    }
}

struct ExtensionStore {
    wasi: p1::WasiP1Ctx,
    limits: StoreLimits,
}

pub struct WasmExtension {
    store: Store<ExtensionStore>,
    memory: Memory,
    alloc: TypedFunc<i32, i32>,
    dealloc: TypedFunc<(i32, i32), ()>,
    abi_version: TypedFunc<(), i32>,
    indexed_call_names: TypedFunc<(), i64>,
    index_call: TypedFunc<(i32, i32), i64>,
    handle_event: Option<TypedFunc<(i32, i32), i64>>,
    id: String,
    indexed_call_names_cache: Vec<String>,
    config: WasmExtensionConfig,
}

impl WasmExtension {
    pub fn from_file(id: impl Into<String>, path: impl AsRef<Path>) -> Result<Self> {
        Self::from_file_with_config(id, path, WasmExtensionConfig::default())
    }

    pub fn from_file_with_config(
        id: impl Into<String>,
        path: impl AsRef<Path>,
        host_config: WasmExtensionConfig,
    ) -> Result<Self> {
        let engine = engine()?;
        let module = map_wasmtime(
            Module::from_file(&engine, path.as_ref()),
            &format!(
                "failed to compile Wasm extension module at {}",
                path.as_ref().display()
            ),
        )?;
        Self::from_module(id, &engine, module, host_config)
    }

    pub fn from_bytes(id: impl Into<String>, bytes: &[u8]) -> Result<Self> {
        Self::from_bytes_with_config(id, bytes, WasmExtensionConfig::default())
    }

    pub fn from_bytes_with_config(
        id: impl Into<String>,
        bytes: &[u8],
        host_config: WasmExtensionConfig,
    ) -> Result<Self> {
        let engine = engine()?;
        let module = map_wasmtime(
            Module::from_binary(&engine, bytes),
            "failed to compile Wasm extension bytes",
        )?;
        Self::from_module(id, &engine, module, host_config)
    }

    fn from_module(
        id: impl Into<String>,
        engine: &Engine,
        module: Module,
        host_config: WasmExtensionConfig,
    ) -> Result<Self> {
        let state = ExtensionStore {
            wasi: WasiCtxBuilder::new().build_p1(),
            limits: StoreLimitsBuilder::new()
                .memory_size(host_config.max_memory_bytes)
                .instances(1)
                .tables(16)
                .memories(2)
                .trap_on_grow_failure(true)
                .build(),
        };
        let mut store = Store::new(engine, state);
        store.limiter(|state| &mut state.limits);
        map_wasmtime(
            store.set_fuel(host_config.fuel_per_call),
            "failed to set extension fuel before instantiate",
        )?;

        let mut linker = Linker::new(engine);
        map_wasmtime(
            p1::add_to_linker_sync(&mut linker, |state: &mut ExtensionStore| &mut state.wasi),
            "failed to add WASI preview1 imports to extension linker",
        )?;
        let instance = map_wasmtime(
            linker.instantiate(&mut store, &module),
            "failed to instantiate extension",
        )?;

        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| anyhow!("extension missing exported memory named `memory`"))?;
        let alloc = map_wasmtime(
            instance.get_typed_func::<i32, i32>(&mut store, "alloc"),
            "extension missing `alloc(len) -> ptr` export",
        )?;
        let dealloc = map_wasmtime(
            instance.get_typed_func::<(i32, i32), ()>(&mut store, "dealloc"),
            "extension missing `dealloc(ptr, len)` export",
        )?;
        let abi_version = map_wasmtime(
            instance.get_typed_func::<(), i32>(&mut store, "abi_version"),
            "extension missing `abi_version() -> i32` export",
        )?;
        let indexed_call_names = map_wasmtime(
            instance.get_typed_func::<(), i64>(&mut store, "indexed_call_names"),
            "extension missing `indexed_call_names() -> packed_ptr_len` export",
        )?;
        let index_call = map_wasmtime(
            instance.get_typed_func::<(i32, i32), i64>(&mut store, "index_call"),
            "extension missing `index_call(ptr, len) -> packed_ptr_len` export",
        )?;
        let handle_event =
            match instance.get_typed_func::<(i32, i32), i64>(&mut store, "handle_event") {
                Ok(handle_event) => Some(handle_event),
                Err(_) => None,
            };

        let actual_abi = map_wasmtime(
            store.set_fuel(host_config.fuel_per_call),
            "failed to set extension fuel before abi_version",
        )
        .and_then(|()| {
            map_wasmtime(
                abi_version.call(&mut store, ()),
                "failed to call extension abi_version",
            )
        })?;
        if actual_abi != ABI_VERSION as i32 {
            return Err(anyhow!(
                "Wasm extension ABI version {} != host ABI version {}",
                actual_abi,
                ABI_VERSION
            ));
        }

        map_wasmtime(
            store.set_fuel(host_config.fuel_per_call),
            "failed to set extension fuel before indexed_call_names",
        )?;
        let names_packed = map_wasmtime(
            indexed_call_names.call(&mut store, ()),
            "failed to call extension indexed_call_names",
        )?;
        let (names_bytes, names_ptr, names_len) = read_packed_bytes(
            &memory,
            &mut store,
            names_packed,
            host_config.max_output_bytes,
        )?;
        free_guest_bytes(
            &dealloc,
            &mut store,
            names_ptr,
            names_len,
            "indexed_call_names output",
        )?;
        let indexed_call_names_cache: Vec<String> =
            serde_json::from_slice(&names_bytes).context("invalid indexed_call_names JSON")?;

        Ok(Self {
            store,
            memory,
            alloc,
            dealloc,
            abi_version,
            indexed_call_names,
            index_call,
            handle_event,
            id: id.into(),
            indexed_call_names_cache,
            config: host_config,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn abi_version(&mut self) -> Result<u32> {
        self.refuel("abi_version")?;
        let version = map_wasmtime(
            self.abi_version.call(&mut self.store, ()),
            "failed to call extension abi_version",
        )?;
        Ok(version as u32)
    }

    pub fn indexed_call_names(&self) -> &[String] {
        &self.indexed_call_names_cache
    }

    pub fn refresh_indexed_call_names(&mut self) -> Result<()> {
        self.refuel("indexed_call_names")?;
        let packed = map_wasmtime(
            self.indexed_call_names.call(&mut self.store, ()),
            "failed to call extension indexed_call_names",
        )?;
        let (bytes, ptr, len) = read_packed_bytes(
            &self.memory,
            &mut self.store,
            packed,
            self.config.max_output_bytes,
        )?;
        self.free_guest_bytes(ptr, len, "indexed_call_names output")?;
        self.indexed_call_names_cache =
            serde_json::from_slice(&bytes).context("invalid indexed_call_names JSON")?;
        Ok(())
    }

    pub fn index_call(&mut self, ctx: &CallContext) -> Result<Vec<IndexPatch>> {
        if self.handle_event.is_some() {
            let event = ExtensionEvent {
                event: "index.call.enter".to_string(),
                call: Some(ctx.clone()),
                document: None,
            };
            return self.handle_event(&event).map(|output| output.index_patches);
        }

        let input = serde_json::to_vec(ctx).context("failed to encode CallContext JSON")?;
        if input.len() > self.config.max_input_bytes {
            return Err(anyhow!(
                "extension input payload {} bytes exceeds max {} bytes",
                input.len(),
                self.config.max_input_bytes
            ));
        }
        let ptr = self.write_guest_bytes(&input)?;
        self.refuel("index_call")?;
        let packed = map_wasmtime(
            self.index_call
                .call(&mut self.store, (ptr, input.len() as i32)),
            "failed to call extension index_call",
        )?;
        self.free_guest_bytes(ptr as u32, input.len() as u32, "index_call input")?;

        let (output, out_ptr, out_len) = read_packed_bytes(
            &self.memory,
            &mut self.store,
            packed,
            self.config.max_output_bytes,
        )?;
        self.free_guest_bytes(out_ptr, out_len, "index_call output")?;
        let patches = serde_json::from_slice(&output)
            .context("extension returned invalid IndexPatch JSON")?;
        Ok(patches)
    }

    pub fn handle_event(&mut self, event: &ExtensionEvent) -> Result<ExtensionOutput> {
        let Some(handle_event) = self.handle_event.clone() else {
            return Ok(ExtensionOutput {
                index_patches: Vec::new(),
                response_patches: Vec::new(),
                command_patches: Vec::new(),
            });
        };
        let input = serde_json::to_vec(event).context("failed to encode ExtensionEvent JSON")?;
        if input.len() > self.config.max_input_bytes {
            return Err(anyhow!(
                "extension event input payload {} bytes exceeds max {} bytes",
                input.len(),
                self.config.max_input_bytes
            ));
        }
        let ptr = self.write_guest_bytes(&input)?;
        self.refuel("handle_event")?;
        let packed = map_wasmtime(
            handle_event.call(&mut self.store, (ptr, input.len() as i32)),
            "failed to call extension handle_event",
        )?;
        self.free_guest_bytes(ptr as u32, input.len() as u32, "handle_event input")?;

        let (output, out_ptr, out_len) = read_packed_bytes(
            &self.memory,
            &mut self.store,
            packed,
            self.config.max_output_bytes,
        )?;
        self.free_guest_bytes(out_ptr, out_len, "handle_event output")?;
        serde_json::from_slice(&output).context("extension returned invalid ExtensionOutput JSON")
    }

    fn write_guest_bytes(&mut self, bytes: &[u8]) -> Result<i32> {
        if bytes.len() > i32::MAX as usize {
            return Err(anyhow!(
                "extension input payload too large for i32 ABI: {} bytes",
                bytes.len()
            ));
        }

        let ptr = map_wasmtime(
            self.alloc.call(&mut self.store, bytes.len() as i32),
            "failed to allocate guest memory",
        )?;
        if ptr < 0 {
            return Err(anyhow!("extension alloc returned negative pointer {}", ptr));
        }
        self.memory
            .write(&mut self.store, ptr as usize, bytes)
            .context("failed to write guest memory")?;
        Ok(ptr)
    }

    fn refuel(&mut self, label: &str) -> Result<()> {
        map_wasmtime(
            self.store.set_fuel(self.config.fuel_per_call),
            &format!("failed to set extension fuel before {label}"),
        )
    }

    fn free_guest_bytes(&mut self, ptr: u32, len: u32, label: &str) -> Result<()> {
        free_guest_bytes(&self.dealloc, &mut self.store, ptr, len, label)
    }
}

fn engine() -> Result<Engine> {
    let mut config = Config::new();
    config.consume_fuel(true);
    config.wasm_exceptions(true);
    map_wasmtime(
        Engine::new(&config),
        "failed to create Wasm extension engine",
    )
}

fn read_packed_bytes(
    memory: &Memory,
    store: &mut Store<ExtensionStore>,
    packed: i64,
    max_len: usize,
) -> Result<(Vec<u8>, u32, u32)> {
    let (ptr, len) = unpack_ptr_len(packed)?;
    if len as usize > max_len {
        return Err(anyhow!(
            "extension output payload {} bytes exceeds max {} bytes",
            len,
            max_len
        ));
    }
    let mut bytes = vec![0; len as usize];
    memory
        .read(store, ptr as usize, &mut bytes)
        .context("failed to read guest memory")?;
    Ok((bytes, ptr, len))
}

fn free_guest_bytes(
    dealloc: &TypedFunc<(i32, i32), ()>,
    store: &mut Store<ExtensionStore>,
    ptr: u32,
    len: u32,
    label: &str,
) -> Result<()> {
    if len == 0 {
        return Ok(());
    }
    map_wasmtime(
        dealloc.call(store, (ptr as i32, len as i32)),
        &format!("failed to free guest {label} buffer"),
    )
}

fn map_wasmtime<T>(result: std::result::Result<T, wasmtime::Error>, context: &str) -> Result<T> {
    result.map_err(|err| anyhow!("{context}: {err:?}"))
}

fn unpack_ptr_len(packed: i64) -> Result<(u32, u32)> {
    if packed < 0 {
        return Err(anyhow!(
            "extension returned negative packed pointer/length {}",
            packed
        ));
    }

    let packed = packed as u64;
    let ptr = (packed >> 32) as u32;
    let len = (packed & 0xffff_ffff) as u32;
    Ok((ptr, len))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ruby_fast_lsp_extension_api::{
        Argument, ArgumentValue, DocumentContext, NamespaceKind, Receiver, SourcePosition,
        SourceRange,
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
    fn wasm_extension_handle_event_returns_output() {
        let wasm = wat::parse_str(test_extension_event_wat()).unwrap();
        let mut ext = WasmExtension::from_bytes("test", &wasm).unwrap();

        let output = ext
            .handle_event(&ExtensionEvent {
                event: "index.call.enter".to_string(),
                call: Some(let_context()),
                document: None,
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
                IndexPatch::DefineMethod(_) | IndexPatch::ApplyMixin(_) => None,
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
                }),
            })
            .unwrap();

        assert_eq!(output.response_patches.len(), 2);
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

    fn let_context() -> CallContext {
        CallContext {
            method_name: "let".to_string(),
            receiver: Receiver::None,
            arguments: vec![Argument {
                value: ArgumentValue::Symbol("user".to_string()),
                range: range(),
            }],
            current_namespace: vec!["User".to_string()],
            namespace_kind: NamespaceKind::Singleton,
            call_range: range(),
            message_range: range(),
            resolved_callees: Vec::new(),
            enclosing_calls: vec![ruby_fast_lsp_extension_api::ResolvedCall {
                method_name: "describe".to_string(),
                receiver: Receiver::Constant(vec!["RSpec".to_string()]),
                resolved_callees: vec![ruby_fast_lsp_extension_api::ResolvedCallee {
                    owner: vec!["RSpec".to_string()],
                    owner_kind: NamespaceKind::Singleton,
                    method: "describe".to_string(),
                    resolution: ruby_fast_lsp_extension_api::CalleeResolution::ReceiverOnly,
                }],
                call_range: range(),
                message_range: range(),
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
}
