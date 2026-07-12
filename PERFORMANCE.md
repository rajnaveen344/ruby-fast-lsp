# Production Performance Budgets

Ruby Fast LSP measures production latency with a deterministic, generated Ruby
project rather than relying on ad-hoc stopwatch observations. The built-in
corpus contains 39 project files plus 132 core stub files (172 analysis files,
about 2.4 MB of source on the current Ruby 3.3 fixture).

Run the release benchmark with:

```bash
cargo run --release --bin profiler -- \
  --benchmark-iterations 100 \
  --check-budgets
```

The benchmark performs a cold workspace index, opens a representative
controller, verifies that each semantic query returns a useful result, and
records nearest-rank p50 and p95 latency for full-buffer body-only edits,
completion, hover, definition, references, and semantic diagnostic projection.
Engine memory is the analysis engine's recursively estimated owned heap, not
whole-process RSS.

## Budgets

| Measurement | Budget |
| --- | ---: |
| Cold indexing | 2 s |
| Body-only edit p95 | 100 ms |
| Completion p95 | 50 ms |
| Hover p95 | 25 ms |
| Definition p95 | 25 ms |
| References p95 | 50 ms |
| Semantic diagnostics p95 | 25 ms |
| Estimated engine heap | 32 MiB |

These are regression budgets, not claims that every project or machine will
have identical latency. Change a budget only with a recorded corpus, before and
after measurements, and an explanation of the product tradeoff. Do not weaken
semantic correctness or move project-wide work into the typing path to meet a
budget.

## July 2026 Reference Baseline

Reference machine: Apple M4 Pro, 24 GiB RAM, macOS 26.2. Release build, 100
measured iterations:

| Measurement | Result |
| --- | ---: |
| Cold indexing | 695.8 ms |
| Body-only edit p95 | 1.121 ms |
| Completion p95 | 0.074 ms |
| Hover p95 | 0.081 ms |
| Definition p95 | 0.062 ms |
| References p95 | 7.606 ms |
| Semantic diagnostics p95 | 0.001 ms |
| Estimated engine heap | 5.7 MiB |

The benchmark currently measures the deterministic medium fixture. Release
smoke projects and larger optional corpora remain separate checks; they must not
become a redundant semantic test suite.

## July 2026 Release Smoke Projects

These checkouts are disposable local evidence, not a second semantic suite:

| Project | Revision | Analysis shape | Cold result | Engine heap |
| --- | --- | ---: | ---: | ---: |
| Sinatra | `946812bdec8faf6598fed154a8d611ead612b6fd` | 618 files / 5.0 MB | 1.34 s | 15.2 MiB |
| Discourse | `ca7f32c972e9f8b18c6ea47736e00787c3c5e0e2` | 11,159 files / 44.8 MB | 9.47 s | 176.3 MiB |

Both completed with the release profiler after the shebang/comment-parser crash
fix. Run the same smoke with:

```bash
target/release/profiler --workspace /path/to/project
```

Diagnostic volume remains a production blocker rather than a successful
quality measurement: Sinatra produced 1,891 engine diagnostics and Discourse
76,192. These counts are not classified false-positive rates, but they are too
large to claim real-project diagnostic precision. The simulator separately
enforces zero engine semantic false positives at its oracle-reviewed valid
sites.
