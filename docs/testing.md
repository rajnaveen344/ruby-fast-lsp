# Testing Guide

Ruby Fast LSP uses multiple testing strategies to ensure correctness and reliability.

## Test Categories

### Unit Tests

Standard Rust unit tests for individual components:

```bash
cargo test
```

### Integration Tests

Tests that verify LSP protocol compliance and feature correctness.

Located in `src/test/integration/`:

- `classes/` - Goto and references for classes/modules
- `methods/` - Goto and references for methods
- `constants/` - Goto and references for constants
- `code_lens/` - Module mixin code lenses
- `inlay_hints/` - Type hints for literals, constructors, YARD

```bash
cargo test integration --lib
```

## Inline Test Harness

We use a marker-based inline harness similar to rust-analyzer:

```rust
check_goto(r#"
<def>class Foo
end</def>

Foo$0.new  // cursor at $0, expects definition at <def>
"#).await;
```

### Available Check Functions

| Function            | Markers                              | Description     |
| ------------------- | ------------------------------------ | --------------- |
| `check_goto`        | `$0`, `<def>...</def>`               | Goto definition |
| `check_references`  | `$0`, `<ref>...</ref>`               | Find references |
| `check_type`        | `$0`, `<type:TYPE>`                  | Type inference  |
| `check_inlay_hints` | `<hint:TYPE>`                        | Inlay hints     |
| `check_code_lens`   | `<lens:LABEL>`                       | Code lenses     |
| `check_diagnostics` | `<err>...</err>`, `<warn>...</warn>` | Diagnostics     |

## Simulation Testing

We use **property-based testing** with [proptest](https://proptest-rs.github.io/proptest/) to find edge cases that manual tests miss.

### Quick Simulation Run

Run the main simulation test (100 cases by default):

```bash
cargo test test::simulation::tests::sim --release -- --ignored
```

Run with more cases:

```bash
PROPTEST_CASES=1000 cargo test test::simulation::tests::sim --release -- --ignored
```

Run all simulation-related tests (including unit tests for position tracking):

```bash
cargo test test::simulation --release
```

### Soak Testing (Overnight Fuzzing)

For thorough testing, run the soak test which runs indefinitely until you press Ctrl+C:

```bash
cargo test test::simulation::tests::soak_test::soak --release -- --nocapture --ignored
```

Or with a maximum iteration limit:

```bash
PROPTEST_CASES=100000 cargo test test::simulation::tests::soak_test::soak --release -- --nocapture --ignored
```

**Output:**

```
🔥 SOAK TEST MODE
   Running indefinitely until Ctrl+C
   Failures will be collected (not stopped on first failure)
   Results will be written to src/test/simulation/soak_failures.log

✓ Progress: 1600 | 73 failures (34 unique) | 2s | 800/s
```

Results are written to `src/test/simulation/soak_failures.log` with deduplicated failure types and seeds for reproducibility.

### What Simulation Tests Cover

| Category        | What's Tested                                        |
| --------------- | ---------------------------------------------------- |
| **Edits**       | Safe edits with deterministic position tracking      |
| **Text Sync**   | Document open/edit/save/close operations             |
| **Definitions** | Go-to-definition resolves correctly (even post-edit) |
| **Completions** | Autocomplete suggestions                             |
| **Symbols**     | Document and workspace symbols                       |
| **Stability**   | No crashes on random input                           |

### Regression Seeds

When proptest finds a failure, it saves the seed to `src/test/simulation/regressions.txt`. These seeds are automatically re-run on future test runs to prevent regressions.

**Important:** Seeds are only valid for the current test signature. If the test parameters change (e.g., adding/removing inputs), old seeds become invalid and the file should be cleared.

## Running Specific Tests

```bash
# Run all tests
cargo test

# Run tests matching a pattern
cargo test definition

# Run with output
cargo test -- --nocapture

# Run in release mode (faster for simulation)
cargo test --release
```

## Debugging Test Failures

### When Simulation Fails

When proptest finds a failure, it:

1. **Prints the minimal failing input** (after shrinking)
2. **Saves the seed** to `src/test/simulation/regressions.txt`
3. **Automatically replays** the seed on every subsequent test run

Example failure output:

```
test test::simulation::tests::sim ... FAILED
minimal failing input: tracked = TrackedCode { code: "class Foo...", ... }
```

The seed is automatically saved, so just run the test again to reproduce:

```bash
cargo test test::simulation::tests::sim --release -- --nocapture
```

### View the Regression File

```bash
cat src/test/simulation/regressions.txt
```

### Clear Stale Seeds

If the test signature has changed and old seeds are causing issues:

```bash
# Clear all seeds (keeps the header comments)
head -10 src/test/simulation/regressions.txt > tmp && mv tmp src/test/simulation/regressions.txt
```

### View Soak Test Failure Log

After running soak tests:

```bash
cat src/test/simulation/soak_failures.log
```

The log contains:

- Unique failure types (deduplicated)
- Code snippets that triggered failures

## CI Integration

Tests are run automatically on:

- Every PR (quick tests)
- Nightly (extended simulation runs)

See `.github/workflows/` for CI configuration.
