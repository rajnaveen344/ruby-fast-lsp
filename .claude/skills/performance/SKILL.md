# Performance Skill

Use this skill when optimizing code, profiling performance, or making performance-critical decisions in the Ruby Fast LSP project. Provides guidance on benchmarking, profiling, and optimization patterns. Triggers: performance, optimization, slow, profiling, benchmark, memory, latency, speed.

---

## Performance Philosophy

### Priority Order

1. **Correctness first** - Wrong fast code is useless
2. **Measure before optimizing** - No premature optimization
3. **Optimize the right thing** - Profile to find bottlenecks
4. **Simple optimizations first** - Low-hanging fruit

### Resource Priority (Slowest First)

1. **Disk I/O** - Milliseconds per operation
2. **Network** - Variable, often slow
3. **Memory allocation** - Microseconds
4. **CPU computation** - Nanoseconds

Optimize slowest resources first, adjusted for frequency.

---

## Profiling Commands

### CPU Profiling with Flamegraph

```bash
# Install
cargo install flamegraph

# Profile (requires sudo on macOS)
sudo cargo flamegraph --bin ruby-fast-lsp -- /path/to/project

# View
open flamegraph.svg
```

### Memory Profiling with dhat

```bash
# Add to Cargo.toml (dev only)
[dev-dependencies]
dhat = "0.3"

# In code
#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn main() {
    #[cfg(feature = "dhat-heap")]
    let _profiler = dhat::Profiler::new_heap();

    // ... rest of main
}

# Run
cargo run --features dhat-heap
# Creates dhat-heap.json
```

### Benchmark with Criterion

```bash
# Add to Cargo.toml
[dev-dependencies]
criterion = "0.5"

[[bench]]
name = "indexing"
harness = false
```

```rust
// benches/indexing.rs
use criterion::{criterion_group, criterion_main, Criterion};

fn bench_index_file(c: &mut Criterion) {
    let content = include_str!("../fixtures/large_file.rb");

    c.bench_function("index_1000_line_file", |b| {
        b.iter(|| {
            index_content(content)
        })
    });
}

criterion_group!(benches, bench_index_file);
criterion_main!(benches);
```

```bash
cargo bench
```

---

## Key Performance Patterns

### 1. String Interning with Ustr

For frequently compared/stored strings:

```rust
use ustr::Ustr;

// GOOD: Interned - O(1) comparison, shared storage
struct Symbol {
    name: Ustr,  // 8 bytes, pointer equality
}

let a: Ustr = "MyClass".into();
let b: Ustr = "MyClass".into();
assert!(a == b);  // Pointer comparison, not string comparison

// BAD: String - O(n) comparison, separate allocations
struct Symbol {
    name: String,  // 24 bytes + heap allocation
}
```

**When to use Ustr:**

- FQN names (constants, methods)
- File paths (repeated lookups)
- Symbol names in index

### 2. Pre-allocation

Avoid repeated allocations in loops:

```rust
// GOOD: Pre-allocated
let mut results = Vec::with_capacity(expected_count);
for item in items {
    results.push(process(item));
}

// BAD: Grows dynamically
let mut results = Vec::new();
for item in items {
    results.push(process(item));  // May reallocate multiple times
}
```

### 3. Avoid Cloning in Hot Paths

```rust
// GOOD: Borrow
fn process_symbols(symbols: &[Symbol]) {
    for symbol in symbols {
        analyze(symbol);  // Borrows
    }
}

// BAD: Clone
fn process_symbols(symbols: Vec<Symbol>) {
    for symbol in symbols {
        analyze(&symbol);  // Owns, may have cloned
    }
}
```

### 4. Use Slices Over Vectors

```rust
// GOOD: Accept slice
fn find_in(haystack: &[Symbol], needle: &str) -> Option<&Symbol>

// BAD: Require owned vector
fn find_in(haystack: Vec<Symbol>, needle: &str) -> Option<Symbol>
```

### 5. Batch Operations

```rust
// GOOD: Batch insert
fn index_files(&mut self, files: &[PathBuf]) {
    let mut batch = Vec::with_capacity(files.len());
    for file in files {
        batch.push(self.process_file(file));
    }
    self.index.insert_batch(batch);
}

// BAD: Individual inserts
fn index_files(&mut self, files: &[PathBuf]) {
    for file in files {
        let processed = self.process_file(file);
        self.index.insert(processed);  // Lock per insert
    }
}
```

### 6. Lazy Evaluation

```rust
// GOOD: Compute only if needed
fn get_type_info(&self) -> Option<TypeInfo> {
    self.cached_type.get_or_init(|| {
        expensive_type_computation()
    }).clone()
}

// BAD: Always compute
fn get_type_info(&self) -> Option<TypeInfo> {
    expensive_type_computation()  // Even if not used
}
```

---

## Data Structure Selection

### For Lookups

| Need                | Structure          | Lookup   | Insert   |
| ------------------- | ------------------ | -------- | -------- |
| Key-value by string | `HashMap<Ustr, V>` | O(1)     | O(1)     |
| Sorted by key       | `BTreeMap<K, V>`   | O(log n) | O(log n) |
| Prefix matching     | `Trie`             | O(k)     | O(k)     |
| Set membership      | `HashSet<Ustr>`    | O(1)     | O(1)     |

### For Collections

| Need             | Structure       | Notes                   |
| ---------------- | --------------- | ----------------------- |
| Ordered, indexed | `Vec<T>`        | Best for iteration      |
| LIFO             | `Vec<T>`        | Use as stack            |
| FIFO             | `VecDeque<T>`   | Use as queue            |
| Stable indices   | `SlotMap<K, V>` | Elements can be removed |

### For Concurrency

| Need            | Structure       |
| --------------- | --------------- |
| Read-heavy      | `RwLock<T>`     |
| Write-heavy     | `Mutex<T>`      |
| Lock-free reads | `DashMap<K, V>` |
| Atomic counters | `AtomicUsize`   |

---

## LSP-Specific Performance

### Incremental Indexing

Don't re-index everything on change:

```rust
// GOOD: Incremental
fn on_file_changed(&mut self, uri: &Url) {
    // Remove old entries for this file
    self.index.remove_file(uri);

    // Re-index just this file
    if let Ok(content) = self.read_file(uri) {
        self.index_file(uri, &content);
    }
}

// BAD: Full re-index
fn on_file_changed(&mut self, _uri: &Url) {
    self.reindex_workspace();  // Expensive!
}
```

### Debounce Rapid Changes

```rust
use tokio::time::{sleep, Duration};

async fn handle_did_change(&self, uri: Url) {
    // Cancel previous pending reindex
    self.cancel_pending_reindex(&uri);

    // Wait for typing to settle
    sleep(Duration::from_millis(150)).await;

    // Now reindex
    self.reindex_document(&uri).await;
}
```

### Limit Completion Results

```rust
const MAX_COMPLETIONS: usize = 100;

fn get_completions(&self) -> Vec<CompletionItem> {
    self.all_symbols()
        .filter(|s| s.matches_prefix(prefix))
        .take(MAX_COMPLETIONS)  // Don't return thousands
        .map(|s| s.to_completion_item())
        .collect()
}
```

### Use Prefix Trees for Completion

```rust
use radix_trie::Trie;

struct CompletionIndex {
    constants: Trie<String, ConstantInfo>,
    methods: Trie<String, MethodInfo>,
}

impl CompletionIndex {
    fn complete_constant(&self, prefix: &str) -> impl Iterator<Item = &ConstantInfo> {
        self.constants
            .get_raw_descendant(prefix)
            .into_iter()
            .flat_map(|subtrie| subtrie.values())
    }
}
```

---

## Performance Budgets

### Target Latencies

| Operation                    | Target | Max   |
| ---------------------------- | ------ | ----- |
| Completion                   | 50ms   | 100ms |
| Go-to-definition             | 20ms   | 50ms  |
| Hover                        | 20ms   | 50ms  |
| Find references              | 100ms  | 500ms |
| Initial indexing (10K files) | 5s     | 10s   |
| Incremental reindex (1 file) | 50ms   | 200ms |

### Memory Targets

| Metric               | Target            |
| -------------------- | ----------------- |
| Base memory          | < 50MB            |
| Per 1K files indexed | + 20MB            |
| Peak during indexing | < 2x steady state |

---

## Performance Logging

Add timing to critical paths:

```rust
use std::time::Instant;
use log::info;

pub async fn handle_completion(params: CompletionParams) -> Vec<CompletionItem> {
    let start = Instant::now();

    let result = compute_completions(params).await;

    let elapsed = start.elapsed();
    if elapsed > Duration::from_millis(50) {
        warn!("[PERF] Slow completion: {:?}", elapsed);
    } else {
        info!("[PERF] Completion in {:?}", elapsed);
    }

    result
}
```

---

## Optimization Checklist

### Before Optimizing

- [ ] Have you profiled to find the bottleneck?
- [ ] Is this actually slow in practice?
- [ ] Do you have a benchmark to measure improvement?

### Common Wins

- [ ] Use `Ustr` for frequently compared strings
- [ ] Pre-allocate vectors with known capacity
- [ ] Avoid cloning in loops
- [ ] Use references instead of owned values
- [ ] Batch database/index operations
- [ ] Cache expensive computations

### After Optimizing

- [ ] Run benchmarks to verify improvement
- [ ] Check memory usage hasn't regressed
- [ ] Ensure correctness tests still pass
- [ ] Document why optimization was needed
