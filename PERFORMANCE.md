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
large to claim real-project diagnostic precision. The LSP adapter therefore
publishes cold-index diagnostics only for currently open documents; closed-file
facts remain queryable in the engine without flooding the editor. The simulator
separately enforces zero engine semantic false positives at its oracle-reviewed
valid sites.

Representative open-file diagnostics can be sampled through the real didOpen
lifecycle without adding a project-specific test suite:

```bash
cargo build --release --bin profiler
target/release/profiler --workspace /path/to/project \
  --diagnostics-file app/models/user.rb
```

`--diagnostics-file` is repeatable, workspace-relative, and emits one JSON line
per file containing the semantic diagnostics visible after opening it. A July
2026 sample of Sinatra `base.rb`/`main.rb` and Discourse `user.rb`/
`application_controller.rb` exposed general false-positive classes in
forwarding definitions, incomplete ancestor/mixin chains, positional options
hashes, and generated writers. Fixing the first three reduced the sampled
Discourse files from 314/221 diagnostics to 12/40 after the writer correction,
and the sampled Sinatra files from more than 120/11 to 59/10. Most remaining
errors are missing dependency constants in disposable checkouts without
installed bundles; do not use that environment to claim dependency-aware
precision.

The post-fix deterministic 100-iteration benchmark remained within every
budget: 706.2 ms cold indexing, 1.069 ms edit p95, 0.080 ms completion,
0.077 ms hover, 0.078 ms definition, 8.243 ms references, sub-microsecond
diagnostic projection, and 5.8 MiB estimated engine heap.

### Dependency-complete Rails smoke

Lobsters at `aebacf4a95dab1eace58cc2592249b137ec36268` provides the
dependency-complete Rails sample. Its isolated production bundle contains 106
gems from 54 direct Gemfile dependencies. The release profiler indexed 3,367
gem files and 469 project files in 4.81 s with 60.3 MiB estimated engine heap.
Opening `app/models/user.rb` and
`app/controllers/application_controller.rb` through the real lifecycle
produced zero semantic diagnostics in both files.

This smoke exposed exponential loop stabilization in RDoc's generated
398,786-byte Markdown parser. Restricting repeated fixed-point passes to the
outermost lexical loop reduced that formerly nonterminating file to 655 ms of
release analysis. The same audit also established anonymous `*`/`**` parameter
semantics and typed frozen value-constant receiver lookup.

After these changes, the deterministic 100-iteration benchmark still passed
every budget: 786.751 ms cold indexing, 0.935 ms edit p95, 0.067 ms completion,
0.072 ms hover, 0.068 ms definition, 7.796 ms references, 0.001 ms diagnostics,
and 5.8 MiB estimated engine heap.

### Extension-complete GoshPosh smoke

The dependency-complete `/Users/naveenraj/goshposh/server` corpus exercises
3,065 indexed files and 43.4 MB of Ruby with all five packaged framework guests.
The release profiler must be pointed at an extracted/installed VS Code extension
root; that path supplies both stubs and the adjacent `extensions/` packages.

After moving immutable 98-gem project metadata to RSpec activation and replacing
the mruby SDK's Ruby JSON hot path with bounded native JSON, cold indexing was
30.79 s with a 170.0 MB estimated engine heap. The RSpec guest handled 6,148
index calls in 3.36 s total (4.90 ms maximum), emitted 6,531 patches and 4,940
execution contexts, and reported zero failures, traps, resource failures,
rejections, conflicts, or disablements. The same workload previously took
55.7 s after basic compiler/GC tuning, with 39.0 s spent in the guest.

Opening `spec/apps/api/auth2_spec.rb` reused cold facts in 9.80 ms. A reference
probe on the RSpec-group `platform` helper returned exactly its single use at
line 1912; the extension-free control returned 631 unrelated project calls.
This is representative production evidence, not a project-specific semantic
test suite.
