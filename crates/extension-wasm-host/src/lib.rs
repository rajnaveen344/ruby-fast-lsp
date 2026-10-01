#[macro_use]
#[allow(unused_macros)]
#[path = "../../ruby-analysis/src/invariant.rs"]
mod invariant;

use crate::invariant::ExpectInvariant;
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
            thread.join().expect_invariant(
                "extension epoch ticker thread panicked",
                "the ticker only sleeps and increments a Wasmtime engine epoch",
                "remove panicking work from the ticker loop",
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
mod tests;
