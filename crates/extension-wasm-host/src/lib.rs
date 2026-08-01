use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

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
// The public ABI permits 64 KiB inputs. mruby's JSON decoder legitimately
// consumes more than 100M Wasm instructions for project contexts containing a
// production-sized lockfile, so the fuel ceiling must cover the full accepted
// payload rather than only the tiny fixtures used by most extension tests.
// The independent epoch deadline still interrupts every guest boundary after
// 500 ms, including guests that consume no fuel efficiently.
const DEFAULT_FUEL_PER_CALL: u64 = 1_000_000_000;
const DEFAULT_WALL_TIMEOUT: Duration = Duration::from_millis(500);
const EPOCH_TICK: Duration = Duration::from_millis(5);
const COMPILED_MODULE_CACHE_SCHEMA: u32 = 1;

#[derive(Clone, Copy, Debug)]
pub struct WasmExtensionConfig {
    pub max_input_bytes: usize,
    pub max_output_bytes: usize,
    pub max_memory_bytes: usize,
    pub fuel_per_call: u64,
    pub wall_timeout: Duration,
}

impl Default for WasmExtensionConfig {
    fn default() -> Self {
        Self {
            max_input_bytes: DEFAULT_MAX_INPUT_BYTES,
            max_output_bytes: DEFAULT_MAX_OUTPUT_BYTES,
            max_memory_bytes: DEFAULT_MAX_MEMORY_BYTES,
            fuel_per_call: DEFAULT_FUEL_PER_CALL,
            wall_timeout: DEFAULT_WALL_TIMEOUT,
        }
    }
}

struct ExtensionStore {
    wasi: p1::WasiP1Ctx,
    limits: StoreLimits,
}

struct EpochTicker {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl EpochTicker {
    fn start(engine: Engine) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::Acquire) {
                thread::sleep(EPOCH_TICK);
                engine.increment_epoch();
            }
        });
        Self {
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for EpochTicker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(thread) = self.thread.take() {
            thread.join().expect(
                "INVARIANT VIOLATED: extension epoch ticker thread panicked. This is a bug because the ticker only sleeps and increments a Wasmtime engine epoch. Fix: remove panicking work from the ticker loop.",
            );
        }
    }
}

pub struct WasmExtension {
    _compiled: CompiledWasmExtension,
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

struct CompiledWasmExtensionInner {
    engine: Engine,
    module: Module,
    _epoch_ticker: EpochTicker,
}

#[derive(Clone)]
pub struct CompiledWasmExtension {
    inner: Arc<CompiledWasmExtensionInner>,
}

/// Owns the exact Wasmtime engine used to compile or restore one extension.
///
/// Each compile/restore result owns one epoch ticker. Clones used to instantiate
/// project guests share that immutable module and ticker, while every guest
/// still owns an independent store, memory, limits, and mutable state.
pub struct WasmExtensionCompiler {
    engine: Engine,
}

impl WasmExtensionCompiler {
    pub fn new() -> Result<Self> {
        Ok(Self { engine: engine()? })
    }

    /// Returns a deterministic identity for Wasmtime's target/compiler/config
    /// compatibility plus Ruby Fast LSP's compiled-product schema.
    pub fn cache_identity(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        COMPILED_MODULE_CACHE_SCHEMA.hash(&mut hasher);
        self.engine
            .precompile_compatibility_hash()
            .hash(&mut hasher);
        hasher.finish()
    }

    pub fn compile(&self, wasm_bytes: &[u8]) -> Result<CompiledWasmExtension> {
        let module = map_wasmtime(
            Module::from_binary(&self.engine, wasm_bytes),
            "failed to compile Wasm extension bytes",
        )?;
        Ok(CompiledWasmExtension::new(self.engine.clone(), module))
    }

    pub fn compile_and_serialize(
        &self,
        wasm_bytes: &[u8],
    ) -> Result<(CompiledWasmExtension, Vec<u8>)> {
        let compiled = self.compile(wasm_bytes)?;
        let serialized = compiled.serialize()?;
        Ok((compiled, serialized))
    }

    /// Restores a module from byte-exact output previously returned by
    /// `compile_and_serialize` for the same `cache_identity`.
    ///
    /// # Safety
    ///
    /// Wasmtime compiled artifacts contain native code and are only lightly
    /// validated. The caller must prove that `serialized` is unmodified output
    /// from `compile_and_serialize`, including an exact source digest,
    /// compatibility identity, payload length, and payload checksum check.
    pub unsafe fn deserialize_verified(&self, serialized: &[u8]) -> Result<CompiledWasmExtension> {
        let module = map_wasmtime(
            // SAFETY: the caller contract above is exactly Wasmtime's
            // `Module::deserialize` contract.
            unsafe { Module::deserialize(&self.engine, serialized) },
            "failed to deserialize verified compiled Wasm extension module",
        )?;
        Ok(CompiledWasmExtension::new(self.engine.clone(), module))
    }
}

impl CompiledWasmExtension {
    pub fn serialize(&self) -> Result<Vec<u8>> {
        map_wasmtime(
            self.inner.module.serialize(),
            "failed to serialize compiled Wasm extension module",
        )
    }

    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let engine = engine()?;
        let module = map_wasmtime(
            Module::from_file(&engine, path.as_ref()),
            &format!(
                "failed to compile Wasm extension module at {}",
                path.as_ref().display()
            ),
        )?;
        Ok(Self::new(engine, module))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let engine = engine()?;
        let module = map_wasmtime(
            Module::from_binary(&engine, bytes),
            "failed to compile Wasm extension bytes",
        )?;
        Ok(Self::new(engine, module))
    }

    fn new(engine: Engine, module: Module) -> Self {
        let epoch_ticker = EpochTicker::start(engine.clone());
        Self {
            inner: Arc::new(CompiledWasmExtensionInner {
                engine,
                module,
                _epoch_ticker: epoch_ticker,
            }),
        }
    }
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
        let compiled = CompiledWasmExtension::from_file(path)?;
        Self::from_compiled_with_config(id, compiled, host_config)
    }

    pub fn from_bytes(id: impl Into<String>, bytes: &[u8]) -> Result<Self> {
        Self::from_bytes_with_config(id, bytes, WasmExtensionConfig::default())
    }

    pub fn from_bytes_with_config(
        id: impl Into<String>,
        bytes: &[u8],
        host_config: WasmExtensionConfig,
    ) -> Result<Self> {
        let compiled = CompiledWasmExtension::from_bytes(bytes)?;
        Self::from_compiled_with_config(id, compiled, host_config)
    }

    pub fn from_compiled(id: impl Into<String>, compiled: CompiledWasmExtension) -> Result<Self> {
        Self::from_compiled_with_config(id, compiled, WasmExtensionConfig::default())
    }

    pub fn from_compiled_with_config(
        id: impl Into<String>,
        compiled: CompiledWasmExtension,
        host_config: WasmExtensionConfig,
    ) -> Result<Self> {
        if host_config.wall_timeout.is_zero() {
            return Err(anyhow!(
                "extension wall-clock timeout must be greater than zero"
            ));
        }
        let engine = &compiled.inner.engine;
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
        prepare_store(&mut store, host_config, "instantiate")?;

        let mut linker = Linker::new(engine);
        map_wasmtime(
            p1::add_to_linker_sync(&mut linker, |state: &mut ExtensionStore| &mut state.wasi),
            "failed to add WASI preview1 imports to extension linker",
        )?;
        let instance = map_guest_call(
            linker.instantiate(&mut store, &compiled.inner.module),
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
        let handle_event = instance
            .get_typed_func::<(i32, i32), i64>(&mut store, "handle_event")
            .ok();

        prepare_store(&mut store, host_config, "abi_version")?;
        let actual_abi = map_guest_call(
            abi_version.call(&mut store, ()),
            "failed to call extension abi_version",
        )?;
        if actual_abi != ABI_VERSION as i32 {
            return Err(anyhow!(
                "Wasm extension ABI version {} != host ABI version {}",
                actual_abi,
                ABI_VERSION
            ));
        }

        prepare_store(&mut store, host_config, "indexed_call_names")?;
        let names_packed = map_guest_call(
            indexed_call_names.call(&mut store, ()),
            "failed to call extension indexed_call_names",
        )?;
        let (names_bytes, names_ptr, names_len) = read_packed_bytes(
            &memory,
            &mut store,
            names_packed,
            host_config.max_output_bytes,
        )?;
        prepare_store(&mut store, host_config, "free indexed_call_names output")?;
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
            _compiled: compiled,
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
        let version = map_guest_call(
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
        let packed = map_guest_call(
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
        self.index_call_output(ctx)
            .map(|output| output.index_patches)
    }

    pub fn index_call_output(&mut self, ctx: &CallContext) -> Result<ExtensionOutput> {
        if self.handle_event.is_some() {
            let event = ExtensionEvent {
                event: "index.call.enter".to_string(),
                call: Some(ctx.clone()),
                document: None,
                project: None,
                settings: None,
                files: None,
                process_results: None,
            };
            return self.handle_event(&event);
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
        let packed = map_guest_call(
            self.index_call
                .call(&mut self.store, (ptr, input.len() as i32)),
            "failed to call extension index_call",
        )
        .map_err(|error| {
            anyhow!(
                "extension index_call input was {} bytes: {error:#}",
                input.len()
            )
        })?;
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
        Ok(ExtensionOutput::index_patches(patches))
    }

    pub fn handle_event(&mut self, event: &ExtensionEvent) -> Result<ExtensionOutput> {
        let Some(handle_event) = self.handle_event.clone() else {
            return Ok(ExtensionOutput {
                index_patches: Vec::new(),
                execution_contexts: Vec::new(),
                response_patches: Vec::new(),
                command_patches: Vec::new(),
                process_requests: Vec::new(),
                reindex_files: Vec::new(),
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
        let packed = map_guest_call(
            handle_event.call(&mut self.store, (ptr, input.len() as i32)),
            "failed to call extension handle_event",
        )
        .map_err(|error| anyhow!("extension event input was {} bytes: {error:#}", input.len()))?;
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

        self.refuel("alloc")?;
        let ptr = map_guest_call(
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
        prepare_store(&mut self.store, self.config, label)
    }

    fn free_guest_bytes(&mut self, ptr: u32, len: u32, label: &str) -> Result<()> {
        self.refuel(&format!("free {label}"))?;
        free_guest_bytes(&self.dealloc, &mut self.store, ptr, len, label)
    }
}

fn engine() -> Result<Engine> {
    let mut config = Config::new();
    config.consume_fuel(true);
    config.epoch_interruption(true);
    config.wasm_exceptions(true);
    map_wasmtime(
        Engine::new(&config),
        "failed to create Wasm extension engine",
    )
}

fn prepare_store(
    store: &mut Store<ExtensionStore>,
    config: WasmExtensionConfig,
    label: &str,
) -> Result<()> {
    map_wasmtime(
        store.set_fuel(config.fuel_per_call),
        &format!("failed to set extension fuel before {label}"),
    )?;
    let tick_nanos = EPOCH_TICK.as_nanos();
    let timeout_nanos = config.wall_timeout.as_nanos();
    let ticks = timeout_nanos.div_ceil(tick_nanos).max(1);
    let ticks = u64::try_from(ticks).map_err(|_| {
        anyhow!(
            "extension wall-clock timeout {} ms exceeds supported epoch range",
            config.wall_timeout.as_millis()
        )
    })?;
    store.set_epoch_deadline(ticks);
    Ok(())
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

fn map_guest_call<T>(result: std::result::Result<T, wasmtime::Error>, context: &str) -> Result<T> {
    result.map_err(|error| {
        let detail = format!("{error:?}");
        if detail.to_ascii_lowercase().contains("interrupt") {
            anyhow!("{context}: extension wall-clock deadline exceeded: {detail}")
        } else {
            anyhow!("{context}: {detail}")
        }
    })
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
        Argument, ArgumentValue, CalleeResolution, DocumentContext, ExecutionContextTarget,
        LockedGem, LockedGemSource, NamespaceKind, ProjectContext, ProjectSourceKind, Receiver,
        ResolvedCall, ResolvedCallee, SourcePosition, SourceRange,
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
}
