# Testing Guide

Ruby Fast LSP uses multiple testing strategies to ensure correctness and reliability.

## Test Categories

### Unit Tests

Standard Rust unit tests for individual components:

```bash
cargo test
```

### Integration Tests

Tests that verify LSP protocol compliance and feature correctness:

```bash
cargo test integration
```

### Snapshot Tests

Uses [insta](https://insta.rs/) for snapshot testing of LSP responses:

```bash
cargo test snapshots
cargo insta review  # Review snapshot changes
```

## Simulation Testing

We use **property-based testing** with [proptest](https://proptest-rs.github.io/proptest/) to find edge cases that manual tests miss.

### Quick Simulation Run

Run the simulation runner with default settings (100 cases):

```bash
cargo test sim --release
```

Run with more cases:

```bash
PROPTEST_CASES=1000 cargo test sim --release
```

### Soak Testing (Overnight Fuzzing)

For thorough testing, run the soak test which runs indefinitely until you press Ctrl+C:

```bash
cargo test soak --release -- --nocapture --ignored
```

Or with a maximum iteration limit:

```bash
PROPTEST_CASES=100000 cargo test soak --release -- --nocapture --ignored
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

| Category        | What's Tested                            |
| --------------- | ---------------------------------------- |
| **Text Sync**   | Document open/edit/save/close operations |
| **Definitions** | Go-to-definition resolves correctly      |
| **Types**       | Type inference via inlay hints           |
| **Completions** | Autocomplete suggestions                 |
| **Symbols**     | Document and workspace symbols           |
| **Stability**   | No crashes on random input               |

### Regression Seeds

When proptest finds a failure, it saves the seed to `src/test/simulation/regressions.txt`. These seeds are automatically re-run on future test runs to prevent regressions. This file is checked into source control.

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

### Reproduce a Simulation Failure

If simulation testing finds a bug, it prints a seed. Re-run with that seed:

```bash
PROPTEST_SEED=<seed_from_output> cargo test simulation_runner
```

### View Simulation Failure Log

After running soak tests:

```bash
cat simulation_failures.log
```

## CI Integration

Tests are run automatically on:

- Every PR (quick tests)
- Nightly (extended simulation runs)

See `.github/workflows/` for CI configuration.
