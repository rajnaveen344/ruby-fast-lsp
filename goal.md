# Ruby Fast LSP: Fast, Deterministic Multi-Root Indexing

## Reusable Goal Text

Elevate Ruby Fast LSP's multi-root workspace indexing to an evidence-backed
9/10 production level. Opening an umbrella folder containing several isolated
Ruby projects must not cause unbounded competing scans, repeated parsing of
identical dependencies, misleading readiness, or racing status-bar updates.
Build one server-owned indexing scheduler with bounded concurrency,
active-project priority, explicit per-project state, monotonic progress
generations, deterministic failure handling, and cancellation/replacement for
workspace and runtime changes. Keep CPU-heavy work off the async LSP reactor and
place scheduler admission, blocking workers, Rayon, extension guests, and JVM
work under one measured process-wide resource budget. Coalesce watcher storms
and counter-only status updates, and bound both resident cache memory and owned
persistent-cache disk use. Preserve one isolated `AnalysisEngine` and exact
runtime, Bundler, extension, and external-document provenance per Ruby project.
Eliminate redundant discovery, reads, parsing, and fact construction through
validated immutable caches without merging semantic ownership or weakening
diagnostics. Coalesce concurrent identical gem, runtime, stub, extension,
JAR/JMOD, extraction, signature, and decompilation work through one
single-flight producer, then persist only checksum-verified derived products
under a bounded, automatically cleaned Ruby Fast LSP cache; reuse package
manager artifacts in place and never modify or delete them. Persist only
immutable external/runtime products in this goal, never workspace-owned source,
unsaved buffers, diagnostics, or mutable project semantic state. Make the
single bottom-right Ruby Fast LSP item follow the active document's deepest
owning project and render only authoritative server state; queued, indexing,
ready, failed, and no-project states must never overwrite one another out of
order.
Reject cyclic or contradictory inheritance facts at the semantic graph write
boundary with deterministic, source-attributed behavior; real dependency code,
conditional compatibility branches, or generated facts must never panic an
indexing worker or leave the workspace in a misleading partial-ready state.
Make same-file navigation available within 500 ms, project navigation within
five seconds cold, dependency navigation within fifteen seconds cold, and keep
exhaustive sibling work in the background. Measure cold, warm, active-project,
all-project, memory, CPU, disk, and incremental behavior on synthetic fixtures
and `/Users/naveenraj/goshposh`, then enforce the resulting local performance
budgets in tests and the packaged VSIX.

## Product Outcome

A developer opening a multi-service folder should observe:

- The extension acknowledges startup and shows the owning project within 100
  milliseconds of receiving the active document.
- The open file's syntax, symbols, and same-file navigation work within 500
  milliseconds without waiting for workspace indexing.
- Project-source Go to Definition becomes available within five seconds on the
  reference `goshposh` workspace, even while dependencies continue indexing.
- After project navigation is ready, caret-driven Document Highlight,
  Go to Definition, hover, and sibling interactive requests on a large open
  file remain fast: highlight must use same-document work only and must not
  block the LSP request loop behind a project-wide reference walk. Measured
  on `goshposh` `server/lib/api_app.rb`, a ~10.5 s full-project highlight
  stalled F12 until it finished even though goto itself completed in ~1–2 ms.
- The Ruby project containing the active file becomes useful first.
- Other projects index with bounded CPU, memory, and disk pressure.
- Switching the active editor immediately shows that file's owning project and
  its true phase.
- Progress for one project cannot move another project's percentage backward,
  mark it ready, hide its status, or replace its error.
- A project reports ready only after its required semantic inputs have
  completed successfully.
- Already-ready projects remain queryable while other projects are queued,
  indexing, rebuilding, failing, being added, or being removed.
- Identical immutable runtime, stub, gem, JAR, and extension inputs are not
  repeatedly read and parsed for every isolated project.
- No constants, methods, diagnostics, runtime choices, or external-document
  provenance leak between project engines.
- Logs and a detailed status command explain what is queued, running, reused,
  failed, cancelled, and complete without flooding the bottom bar.

The target is **9/10 multi-root indexing**, not instant indexing of arbitrary
repositories. First-time parsing of genuinely different source remains real
work. The product must make that work bounded, prioritized, observable, and
free from avoidable duplication.

### Readiness is staged

Do not use one boolean named `ready` for several materially different product
states:

1. **Document ready**: the active buffer is parsed and same-file navigation is
   available.
2. **Project navigation ready**: project-owned definitions and references are
   indexed; ordinary work may begin.
3. **Dependency navigation ready**: the locked gem, runtime, stdlib, JRuby, and
   JVM/JAR inputs required by the owning project are navigable.
4. **Semantically complete**: graph resolution and complete diagnostics for the
   project are published.
5. **Workspace complete**: every discovered project has reached a terminal
   ready or failed state.

Requests arriving before their required stage must not return a false “not
found.” They should use already-valid facts, prioritize the missing bounded
input, or report that the owning project is still indexing according to the
LSP feature's contract.

## Original Failure and Current Checkpoint

The original implementation had two coupled defects:

1. Initial workspace indexing spawns one independent task per discovered Ruby
   project. Each coordinator performs runtime, dependency, stub, stdlib, gem,
   JVM, extension, and project work without a server-owned scheduling policy.
2. Every coordinator publishes its local message and percentage through the
   same LSP work-done token, `"indexing"`. The VS Code adapter renders those
   interleaved reports directly and also owns a separate runtime item.

Consequences confirmed in the original code:

- A percentage is local to one project but displayed as global progress.
- Concurrent reports can move the visible percentage backward or switch labels
  unpredictably.
- The final task sends `Indexing complete` even when another project failed;
  the failure is represented only by a boolean left false.
- VS Code schedules an untracked three-second hide after an end event. A newer
  indexing run can begin before that timer fires, allowing the old timer to hide
  live progress.
- Runtime status exposes only `indexingComplete: bool`, so the editor cannot
  distinguish queued, discovering, indexing dependencies, indexing project
  code, resolving, cancelled, failed, or stale generations.
- The active-editor runtime refresh rejects stale request responses, but raw
  indexing progress does not have an equivalent generation or sequence guard.
- Two status-bar items present partially overlapping indexing/runtime state.

This is a real ownership and concurrency bug, not merely a wording problem in
the editor.

The current working tree has implemented the first corrective slices:

- Typed per-project phases, generations, sequences, terminal failure/cancel
  states, aggregate snapshots, and structured request/notification transport.
- A server-owned bounded scheduler with deterministic priority ordering and
  active-document reprioritization. Admission is also exclusive per project, so
  replacement generations cannot mutate one isolated engine concurrently.
- Scheduler priority has bounded starvation behavior: one active/open-document
  admission may bypass waiting background work, then the oldest admissible
  background project receives the next slot. Deterministic selector and
  asynchronous queue tests cover the policy.
- Cancellable generation tokens for queued and active work. Replacement,
  workspace removal, runtime rebuild, and shutdown cancel the exact prior run;
  cancelled queued work wakes without admission, and coordinators validate
  their immutable generation at explicit phase and diagnostic-publication
  checkpoints.
- Each coordinator retains the exact isolated engine captured at launch.
  Removing or replacing its workspace cannot dynamically reroute late writes
  into the orphan engine or a newer project engine.
- Staged coordinator ordering that indexes project-owned sources before
  exhaustive dependencies and delays complete diagnostics until final
  resolution.
- Ruby version detection, project dependency scanning, Rayon project fact
  collection, core-stub construction, runtime stdlib indexing, JRuby
  classpath/catalog and runtime-source/signature materialization, gem
  discovery/product-manifest preparation, final semantic resolution, and engine
  compaction run on blocking workers. The unused coordinator-wide Ruby
  load-path subprocess probes were removed from production startup. Current-
  thread Tokio regressions prove both a worker boundary and scheduler-saturated
  CPU work leave the async reactor responsive while respecting the project
  concurrency limit.
- The server-owned worker boundary now has one fairness-aware weighted
  admission queue. Task count, CPU weight, conservative transient-memory
  bytes, and I/O slots are reserved atomically under one lock and released by
  one exact RAII lease; impossible requests panic instead of waiting forever.
  Nested Rayon phases reserve the complete owned pool, sequential phases claim
  one CPU lane, active-project intent is retained before enqueue, and queued
  cancellation releases no partial resource. Cold coordinator phases plus
  checksum-keyed gem product construction, binding, and resolution carry their
  owning project and resource class through this path. Six focused governor
  tests cover nested bounds, atomic multi-dimensional admission, no
  head-of-line blocking for a request whose complete claim fits, cancellation,
  active priority, and invalid accounting.
- Profiler schema 6 records the configured CPU/task/transient-memory/I/O
  budget and exact peak/end usage. The deterministic built-in sample completed
  nine governed tasks with zero panics, no leaked active or queued leases, a
  four-lane CPU peak, 256 MiB transient-memory peak, one I/O-slot peak, and
  1.138-second semantic completion. This validates accounting and evidence
  shape only; it does not establish production defaults. Sanitized evidence is
  checked in at
  `support/performance/indexing-resource-governor-2026-07-30.json`.
- The governor now supports async external work without converting it into a
  blocking worker. Its exact lease spans the future lifetime and records
  post-admission cancellation separately from normal completion and panic.
  Runtime installation scans/version probes, trusted extension watched-file
  child processes, and RuboCop/Standard lint, safe-fix, and formatting
  subprocesses use that path. Contention tests prove each waits for the complete
  weighted claim and releases CPU, transient-memory, I/O, and task accounting
  exactly once. The redundant global system-Ruby subprocess was removed;
  runtime selection remains project-owned.
- Open, change, and save semantic replacement now serializes per document and
  executes the complete parser/indexer pass on a blocking worker under one
  weighted open-document lease. Index-time Wasm guest calls are covered by that
  outer lease without nested admission. Weak per-URI async locks preserve the
  newest document version while allowing inactive locks to be reclaimed. A
  current-thread saturation regression proves didOpen queues for the complete
  resource claim without blocking the reactor; an overlapping didChange
  regression proves an older queued version cannot overwrite newer semantic
  facts.
- Extension discovery, loading, activation, settings reload, and replacement
  now run off the async reactor under one background weighted lease.
  Request-time document-symbol and code-lens guest calls use open-document
  leases, while response surfaces with no loaded supporting guest bypass
  admission. Reconfiguration is serialized without holding the registry write
  lock during guest construction or old-guest deactivation. Project
  coordinators no longer load configured packages independently: every
  isolated engine consumes the one server-owned registry. Current-thread
  saturation tests cover load/reload and both request surfaces.
- The exact JRuby import provider built for an isolated project is retained on
  that project workspace and selected by ordinary URI ownership/provenance.
  didOpen, didChange, and didSave therefore materialize lazy Java signatures,
  verified sources, and bounded decompiler output through the same outer
  open-document lease as the semantic replacement. A real edit-lifecycle
  regression proves a newly added `java_import` becomes navigable after cold
  indexing without cross-project provider leakage or nested resource
  admission.
- Cold project indexing no longer performs a separate eager JRuby preflight
  read/parse over every project file. The ordinary project pass derives a
  bounded Java-navigation plan from its existing Prism tree, behind an exact
  catalog-package/Java-DSL source prefilter, and materializes required
  signatures or implementation sources before the pass's normal deferred
  resolution. A cold-batch regression proves `java_import` navigation exists
  without an interactive reindex, while the existing edit regression proves
  later additions still materialize on demand.
- The corresponding two-project `goshposh` JRuby profile eliminated the false
  core-phase cost: summed core work fell from **71.363 seconds** to **646 ms**
  while exact `BSON::ObjectId` navigation still reached the locked Java gem.
  This slice is accepted for correctness and phase ownership, but the overall
  performance result is rejected: active-project navigation was **72.766
  seconds**, dependency navigation **107.570 seconds**, semantic completion
  **109.202 seconds**, external peak RSS **1.892 GB**, and total internal wall
  time **109.202 seconds**. Full six-lane project passes serialized the two
  siblings, while deliberately ephemeral gem products recorded 604 producers
  and no completed reuse. Evidence and the next bottlenecks are recorded in
  `support/performance/jruby-single-pass-project-index-2026-07-30.json`.
- Cooperative project-source admission now divides the default six-lane pool
  into two exact three-lane Rayon pools, each still covered by the one atomic
  task/CPU/memory/I/O governor. The same two-project run overlapped both source
  passes and reduced active-project navigation to **42.602 seconds**,
  dependency navigation to **77.184 seconds**, and total internal wall time to
  **78.681 seconds**. External peak RSS grew **9.7%**, within the comparison
  ceiling, and both semantic probes remained exact. The lane partition is
  accepted; the five- and fifteen-second targets remain open. All 604 gem
  products were still independent ephemeral producers, making demand-loaded
  persistent reuse the next measured bottleneck. Evidence is in
  `support/performance/cooperative-project-lanes-2026-07-30.json`.
- The demand-loaded persistent gem cache and subsequent engine hot-path work
  now produce 604 checksum-validated fresh-process hits for the same two
  isolated JRuby projects without retaining completed gem products in process.
  Exact project and locked Java-platform BSON navigation remain intact.
- Exact gem input preparation is now streamed in deterministic direct-root
  then transitive breadth-first order. Discovery runs once, but each gem's
  source manifest is read, checksum-validated, cache-loaded, and rebound into
  the owning isolated engine before the next manifest is prepared; one final
  global resolution still completes the batch. With the corrected cross-file
  `UserPmm` and locked `BSON::ObjectId` probes, a fully warm fresh-process
  two-project run produced project navigation at **3.166 seconds**, BSON
  navigation at **12.536 seconds**, active semantic completion at **24.240
  seconds**, and all-project completion at **31.589 seconds**. BSON became
  navigable 10.116 seconds before the active dependency stage completed, with
  604/604 gem and 358/358 Java persistent hits and 1.839 GB internal peak RSS.
  Removing the subsequently measured but unused `Gem.path` subprocess reduced
  BSON navigation again to **10.937 seconds**, active semantic completion to
  **22.504 seconds**, all-project completion to **30.258 seconds**, and internal
  peak RSS to **1.740 GB**; exact locations and hit counts were unchanged.
  Exact gem discovery now overlaps the active project source pass as a bounded
  five-lane plus one-lane partition while the active-navigation reservation
  continues to block sibling project passes. A corrected profiler dataset
  identity excludes readiness results and hashes only stable project,
  runtime/classpath, and source inputs. On the identical
  `8864144bee838218f8ea1ac6b37b65b36b7ebd3734e3d114e6d8a8b43213bc73`
  dataset, the controlled overlap reduced locked BSON navigation from
  **10.819 seconds** to **8.653 seconds**, active semantic completion from
  **22.555 seconds** to **20.167 seconds**, and all-project completion from
  **30.523 seconds** to **27.852 seconds**. External peak RSS increased only
  **2.963%**, from 1.737 GB to 1.789 GB, and resource peaks remained exactly
  six CPU lanes, two tasks, 512 MiB transient memory, and two I/O slots.
  Both definitions stayed exact with 604/604 gem and 358/358 Java persistent
  hits.
  Auto-scope installed-gem discovery now performs Bundler resolution and its
  RubyGems fallback inside one selected-runtime process instead of launching a
  failed Bundler process followed by a second global process. On the same
  dataset, active installed discovery fell from **1.927 seconds** to **1.298
  seconds** and sibling discovery from **2.111 seconds** to **1.082 seconds**.
  The repeat produced exact `UserPmm` navigation at **2.991 seconds**, BSON
  navigation at **8.272 seconds**, active semantic completion at **20.026
  seconds**, and all-project completion at **28.303 seconds**. This is accepted
  as redundant-process removal, not as proof of a terminal latency gain,
  because discovery is currently hidden behind the longer project pass.
  A subsequent directory-priority experiment is explicitly rejected and
  removed: prioritizing conventional implementation roots plus the active
  document's top-level root selected 1,982 of 2,618 files, regressed `UserPmm`
  navigation to **3.515 seconds**, and added a batch barrier without producing
  a genuinely bounded navigation set. Future warm project work must use exact
  semantic export/fingerprint reuse or demand identity, not broad path
  heuristics.
  A follow-up experiment that persisted complete workspace-owned project
  semantic facts is explicitly rejected and removed. Although its best warm
  run reached 19.874 seconds terminal wall, it retained roughly 2.19 GB RSS,
  missed the project/dependency readiness budgets, and violated this goal's
  ownership boundary by persisting mutable project semantics. The rejected
  cache namespace and serialization dependency were removed; the accepted
  external-only architecture was revalidated on dataset
  `8864144bee838218f8ea1ac6b37b65b36b7ebd3734e3d114e6d8a8b43213bc73`
  with 604/604 gem and 358/358 Java persistent hits. Exact `UserPmm`
  navigation was live at **3.038 seconds**, locked `BSON::ObjectId`
  navigation at **8.033 seconds**, the active project was semantically
  complete at **19.304 seconds**, and both projects completed at **27.345
  seconds**, with **1.642 GB** internal peak RSS.
  Initial project admission is now registered as one synchronous batch before
  any coordinator awaits a permit. A measured concurrency-one control exposed
  that asynchronous per-task registration could admit a background sibling
  before the already-known active project; the resource governor then blocked
  that sibling's project pass while the active coordinator remained queued.
  The batch boundary removes this scheduler/resource deadlock and has a
  deterministic regression. A corrected concurrency-one run admitted the
  active `server` project first, produced `UserPmm` at **2.909 seconds** and
  BSON at **8.011 seconds**, then completed both projects in **38.864
  seconds**. The normal concurrency-two repeat remained exact at **27.844
  seconds** terminal wall.
  Instrumented JRuby startup shows the next immutable-product bottleneck
  precisely: each large project spends about **2.335 seconds** before source
  work, including roughly **1.18 seconds** establishing exact classpath
  checksums and **1.05 seconds** decoding/composing the already cached
  per-artifact class metadata. A checksum-keyed composed-catalog product was
  implemented, tested across fresh processes and consumer paths, measured, and
  then rejected and removed. Its zero-copy warm path reduced the catalog
  subphase to **0.82–0.85 seconds** per project and kept RSS within the
  comparison ceiling, but did not improve dependency or semantic readiness.
  More importantly, first publication serialized the two projects behind one
  producer, delayed exact `UserPmm` navigation to **5.294 seconds**, and moved
  it beyond the five-second target. The accepted per-artifact cache remains.
  Do not retry a monolithic catalog product unless publication is demonstrably
  outside active-project readiness and consumers cannot hold governed work
  while waiting. Evidence is in
  `support/performance/java-catalog-product-rejection-2026-07-31.json`.
  The streaming slice is accepted, but this valid-warm run still fails the
  one-second project, three-second dependency, and fifteen-second workspace
  warm-cache targets; it does not prove the cold-cache gate. Evidence is in
  `support/performance/streamed-gem-binding-2026-07-31.json`.
- JRuby Java source navigation no longer rereads or hashes a complete source
  archive for every imported class after classpath discovery established its
  checksum. Discovery records a stable file identity, resolution streams only
  the selected entry, central-directory comparisons do not instantiate every
  ZIP entry, and one verified parsed source archive is retained lazily per
  project resolver. The final two-project run reduced project navigation from
  **42.777 seconds** to **15.152 seconds**, dependency navigation from
  **60.438 seconds** to **32.246 seconds**, and internal wall time from
  **62.077 seconds** to **33.824 seconds**. Same-file work remained below the
  500 ms budget and both semantic probes were exact. This targeted cache is
  accepted, but the overall performance result is rejected: the 5-second
  project, 15-second dependency, and 30-second semantic-completion budgets are
  still missed. Evidence is in
  `support/performance/jruby-source-navigation-cache-2026-07-31.json`.
- CFR decompilation now has a measured 256 MiB total resident-memory ceiling in
  addition to its process-count, timeout, input, output, and checksum bounds.
  The JVM uses explicit 128 MiB heap plus direct-memory, metaspace, code-cache,
  and compressed-class-space limits. Native RSS inspection uses
  `proc_pidinfo` on macOS, `/proc/<pid>/statm` on Linux, and
  `GetProcessMemoryInfo` on Windows; inspection failure, overage, or timeout
  kills and reaps the child before returning an isolated error. The checked-in
  representative CFR campaign measured 69.4–70.0 MB peak RSS across 96–256 MiB
  heap settings and selected 128 MiB heap/256 MiB RSS to match the existing
  JRuby work claim. Evidence is in
  `support/performance/jruby-decompiler-memory-2026-07-30.json`.
- Completed shared core-engine templates now use a weighted single-flight cache
  bounded to eight entries and 128 MiB of engine-estimated heap. In-flight
  consumers retain their immutable value safely across eviction. Completed
  gem products remain deliberately ephemeral after controlled evidence showed
  that retaining them cost 112 MB for only 3.3 MB of second-project reuse.
  Profiler schema 6 records retained core and gem product weight plus core
  evictions. Isolated project engines remain separately owned semantic truth;
  their aggregate heap and process RSS are measured rather than evicted or
  merged.
- Shared core-template installation no longer replaces an engine after an open
  document has contributed live facts. The coordinator retains the empty-engine
  clone fast path, rechecks after the single-flight wait, and otherwise indexes
  core stubs additively. A red/green lifecycle regression proves exact unsaved
  content and same-file definition navigation survive startup core binding.
- Reusable dependency seeds are now isolated from that same open-document
  race. Core templates always yield a separate clean seed, JRuby runtime
  implementation inputs are added to both the project and seed engines through
  ordinary file-owned facts, and a production invariant rejects project,
  excluded, or previously bound gem sources from the seed. On a new isolated
  cache, the two configured `goshposh` JRuby projects reused 95 exact gem
  products instead of producing all 604 independently; locked
  `BSON::ObjectId` navigation reached **14.628 seconds**, inside the cold
  dependency budget. Cold project navigation (**9.433 seconds**), active
  semantic completion (**31.018 seconds**), and all-project completion
  (**56.880 seconds**) remain over their absolute targets. The paired
  fresh-process warm run retained exact answers with 604/604 gem and 358/358
  Java hits, but also remains above the strict warm budgets. Evidence is in
  `support/performance/dependency-seed-isolation-2026-07-31.json`.
- Independent checksum-keyed Java artifact products now resolve in the
  governor-owned six-lane Rayon pool, while final project catalog composition
  remains sequential in exact classpath order. A focused duplicate-class
  regression proves deterministic winner and shadowed provenance. On paired
  isolated cold caches, active catalog preparation fell from **6.225 seconds**
  to **4.312 seconds**, project definition navigation from **9.433 seconds** to
  **7.229 seconds**, BSON navigation from **14.628 seconds** to **12.864
  seconds**, and all-project completion from **56.880 seconds** to **54.443
  seconds**. Internal peak RSS fell **19.7%** in the cold comparison and exact
  governed peaks remained six CPU lanes, two tasks, 512 MiB transient memory,
  and two I/O slots. Two warm repeats kept exact 604/604 gem and 358/358 Java
  hits and completed in about 27.9 seconds, but still miss the strict warm
  budgets. Evidence is in
  `support/performance/parallel-java-artifact-resolution-2026-07-31.json`.
- JAR manifest classpath expansion now consumes the same bounded,
  metadata-stable byte buffer used to establish the artifact checksum instead
  of rereading the complete archive immediately. All ten classpath tests pass.
  The adjacent warm profile left classpath discovery at approximately 1.16
  seconds, so this is accepted only as redundant-I/O removal and carries no
  latency claim. Evidence is in
  `support/performance/classpath-manifest-single-read-2026-07-31.json`.
- Early Go to Definition now uses one bounded, generation-scoped navigation
  demand controller instead of returning a false not-found response while the
  owning project is still indexing. The ordinary query runs first. On a miss,
  exact project and dependency identities are requested concurrently; project
  files are promoted into a bounded fact batch, locked gems are promoted at
  both the startup and streaming dependency frontiers, and the same isolated
  engine performs the retry. Replacement generations supersede stale waiters,
  cancellation has the corresponding LSP result, saturation or a still-pending
  stage returns a retriggerable `ServerCancelled`, and no side engine or
  semantically incomplete lookup path exists. Zero-candidate project demands
  remain pending until the complete project stage proves absence.
  On the exact warm two-project JRuby dataset
  `3f2b6c85ea95985fc5d7d759ad93960773cbe3aa552b634bd387bf9695f18657`,
  Exact bounded project demands are now consumed immediately at the project
  frontier before dependency discovery; a file already handled by the startup
  frontier completes the same demand without being parsed twice. Exhaustive
  batches now collect against one immutable post-frontier namespace context,
  preventing earlier batches from changing the semantic input of later ones
  and reducing batch-stream time without weakening the final resolve. Across
  two identical fresh-process warm-cache repeats, `UserPmm` became navigable
  in **964-1,012 ms** (**988 ms median**) and locked Java-platform
  `BSON::ObjectId` in **1.616-1.678 seconds** (**1.647 seconds median**), both
  while the owning project still
  reported `indexingProject`. All 607 gem and 358 Java products were validated
  fresh-process cache hits, governed resource peaks remained exact, and both
  definition locations retained Project/Gem provenance. The slice is accepted
  for staged usefulness and correctness, not terminal throughput: both
  projects completed in **21.695-21.756 seconds**, above the 15-second warm
  target, active semantic completion remained **19.010-19.755 seconds**, and
  peak RSS was **1.942-1.974 GB**. Terminal time is now within 0.3% of the
  original full-ahead baseline, but RSS still exceeds the allowed baseline
  increase. One project-navigation repeat exceeded the strict one-second
  threshold by 12 ms, so the target is median-passing rather than fully
  accepted at p95. Evidence is in
  `support/performance/request-driven-navigation-2026-07-31.json`.
  The adjacent full local code gate passes 1,312 root library tests, 11
  profiler tests, every non-root workspace/framework test, 43 VS Code adapter
  tests, formatting, and an optimized release build. That gate also made
  runtime stdlib collection deterministic by collecting against one immutable
  namespace snapshot and committing in sorted order, and made the black-box
  editor exercise the real `initialize`/`initialized` plus bounded
  `retriggerRequest` lifecycle. Packaged-VSIX acceptance remains separate.
- Warm project collection now moves each freshly read source buffer into its
  `RubyDocument`, registers it with the engine by borrow, and no longer stores
  a duplicate source inside `SourceDocument`. ASCII engine sources retain only
  their line index and content hash; non-ASCII sources retain exactly one
  engine-owned copy for UTF-16 conversion. Prism comment ranges are initialized
  lazily and invalidated on document update, while shebang masking remains
  unchanged. Across two identical exact-workspace repeats, median peak RSS fell
  from **1.958 GB** to **1.588 GB** (**18.90%**) and is **1.70% below** the
  original full-ahead baseline. Terminal completion was **22.034 seconds**,
  only **1.49% above** that baseline; active semantic completion was **19.541
  seconds**. Dependency navigation remained within budget at a **1.573-second**
  median, while project navigation measured **1.101-1.116 seconds** and
  therefore still does not satisfy the one-second p95 target. Evidence is in
  `support/performance/borrowed-source-registration-2026-07-31.json`.
- Exact project demands already queued when project discovery begins now form
  a bounded micro-frontier ahead of unrelated active-document candidates. The
  same `IndexerProject` retains the deterministic active and exhaustive
  complements, generation-owned waiters wake only after their candidate facts
  enter the isolated engine, zero-candidate/ambiguous requests retain complete
  absence semantics, and requests arriving during the micro-frontier still use
  the existing post-frontier drain. On the same exact two-project warm JRuby
  dataset, `UserPmm` became navigable in **517-631 ms** (**574 ms median**), a
  **48.22%** median reduction from the allocation-optimized repeats and now
  fully inside the strict one-second target. Locked `BSON::ObjectId` remained
  navigable in **1.607-1.796 seconds**. Median terminal wall time improved to
  **21.890 seconds** and peak RSS remained within the accepted ceiling at
  **5.36% above** the original full-ahead baseline. The full root library suite
  passes with **1,313 tests**. Exhaustive throughput remains open: active
  semantic completion is **19.399 seconds** and all-project completion is
  **21.890 seconds**. Evidence is in
  `support/performance/initial-project-demand-frontier-2026-07-31.json`.
- A symbolized native sample then showed ordinary call-node traversal entering
  the extension dispatcher for every Ruby call before learning that no loaded
  extension tracked most method names. The dispatcher now rejects names absent
  from its complete deterministic `tracked_call_names` set before cloning or
  scanning extension state. Across two exact repeats, median project-file
  visitor CPU fell **6.85%** for the active project and **5.39%** for its
  sibling; median peak RSS fell **12.07%** to **1.496 GB**. Exact project
  navigation remained below one second at a **531 ms median**, dependency
  navigation remained below three seconds at **1.721 seconds**, and all 607 gem
  plus 358 Java products were fresh-process hits with no producer, panic, or
  governed-resource leak. Terminal wall time remained effectively flat at
  **21.867 seconds** and active semantic completion at **19.604 seconds**, so
  the slice is accepted as a measured hot-path improvement rather than closure
  of the exhaustive target. A shared local method-return-map experiment was
  measured, regressed terminal time to a **23.721-second median**, and was
  reverted. A lazy current-file type-subject index was also measured and
  reverted after hashing/synchronization increased terminal time to a
  **23.676-second median**. Evidence is in
  `support/performance/extension-call-prefilter-2026-07-31.json`.
- FactCollector now reuses the exact declaration already selected by ordinary
  MRO and visibility resolution instead of resolving that declaration owner as
  a second receiver. A revision-bound resolved-callee cache is capped at 64
  entries per source collection; excess unique calls compute normally, and an
  engine semantic replacement invalidates the cache. Across two exact repeats,
  active-project semantic completion improved **4.285%** to an **18.764-second
  median**, dependency navigation improved to **1.676 seconds**, and project
  navigation remained **530.5 ms**. Terminal wall remained effectively flat at
  **21.804 seconds**. Median peak RSS was **1.759 GB**, **8.879%** above the
  original full-ahead baseline and 18.1 MB below the explicit 10% ceiling. The
  unbounded, duplicated-return, 256-entry, and 128-entry variants were rejected
  before accepting the 64-entry shape. All **390** `ruby-analysis` tests and
  **1,313** root library tests pass. Evidence is in
  `support/performance/resolved-callee-query-cache-2026-07-31.json`.
- The exact immutable JRuby provider is now handed to its owning generation as
  soon as catalog construction completes. Later bounded project batches use
  the provider through the ordinary visitor and per-file replacement path;
  only files collected before the handoff retain compact replay hints. On the
  same exact two-project warm dataset, replay fell from **60 to 18 files** for
  the active project and from **59 to 0 files** for the sibling. Across two
  repeats, terminal wall improved **3.642%** to **21.010 seconds**, active
  semantic completion improved **9.033%** to **17.069 seconds**, and dependency
  navigation improved to **1.659 seconds**. Project navigation remained below
  one second at a **608.5 ms median**. Median peak RSS fell to **1.737 GB**,
  **7.559%** above the full-ahead baseline and 39.4 MB below the explicit 10%
  ceiling. A semantic-context fingerprint regression proves that mixed
  provider-aware batches plus bounded replay converge to the same file-owned
  facts as a fully providerless pass plus full replay. All **1,314** root
  library tests pass. Evidence is in
  `support/performance/jruby-provider-batch-handoff-2026-07-31.json`.
- Activation-scoped extension guests now retain their owning project context on
  the isolated guest instance rather than cloning and serializing the same
  project payload for every matching call. Legacy ABI-v1 per-call guests still
  receive one complete lazily constructed context. Across two exact warm
  `goshposh` repeats, Minitest guest time fell **37.633%**, user CPU fell
  **5.377%**, median peak RSS fell **3.378%**, and terminal wall improved
  **1.275%** to **18.621 seconds**. Project navigation remained at a **571 ms**
  median and dependency navigation at **1.606 seconds**. Both complete semantic
  fingerprints and the per-file manifest hash stayed identical. This slice is
  accepted as bounded hot-path work; the 15-second workspace and five-second
  active semantic targets remain open. Evidence is in
  `support/performance/extension-project-context-delivery-2026-08-01.json`.
- YARD extraction now receives the method line already owned by the source
  document and scans only the attached preceding comment block instead of
  allocating every preceding line for every method. The engine name registry
  now keeps each `FullyQualifiedName` and `ConstLookup` once in insertion order
  rather than cloning it into both a map key and an ID vector. A YARD-only
  candidate reduced CPU but missed the fixed RSS ceiling, so it was not
  accepted alone. With the single-owned registry, three exact warm `goshposh`
  repeats preserved both semantic fingerprints and the complete manifest,
  achieved 607/607 gem and 358/358 Java hits, and improved median wall by
  **7.242%** to **17.273 seconds**, user CPU by **8.255%**, and active semantic
  completion by **7.375%** to **14.594 seconds**. The registry estimate fell
  about **54%**, median peak RSS fell to **1.641 GB**, and the fixed ceiling has
  **135.4 MB** of headroom. Evidence is in
  `support/performance/yard-line-scan-and-name-registry-2026-08-01.json`.
- Constant-value inference now asks `TypeStore` for the borrowed latest known
  fact for one subject instead of expanding every matching fact, and compares
  that result with borrowed current-file facts using the exact prior
  deterministic range precedence. Three exact warm `goshposh` repeats preserve
  both complete semantic fingerprints and the whole exported manifest, achieve
  607/607 gem and 358/358 Java hits, improve median wall another **2.379%** to
  **16.862 seconds**, user CPU by **7.444%**, and active semantic completion by
  **4.385%** to **13.954 seconds**. Project navigation remains **392 ms** and
  dependency navigation **1.649 seconds**. Median peak RSS is **1.736 GB**,
  **40.8 MB** below the fixed ceiling. All **394** `ruby-analysis` and **1,318**
  root library tests pass. Evidence is in
  `support/performance/borrowed-constant-type-lookup-2026-08-01.json`.
- Stable semantic export and result fingerprints now update both existing
  64-bit FNV lanes during one field traversal instead of walking every FQN,
  type, method parameter, and graph component twice. A focused regression
  proves byte-for-byte equivalence with both legacy lanes. Three exact warm
  `goshposh` repeats preserve both complete semantic-result fingerprints and
  the whole manifest, improve median terminal readiness **1.307%** to
  **16.642 seconds**, project navigation to **377 ms**, dependency navigation
  to **1.527 seconds**, and user CPU **0.425%**. Active semantic completion is
  effectively flat at **14.034 seconds** and median peak RSS rises to
  **1.757 GB**, leaving **20.0 MB** below the fixed ceiling; those remain
  explicit constraints. All **395** `ruby-analysis` and **1,318** root library
  tests pass. Evidence is in
  `support/performance/dual-lane-semantic-fingerprint-2026-08-01.json`.
- Shared symbol/type subject buckets no longer re-sort every existing fact
  after replacing one file. Replacement-only stores stable-sort the appended
  file tail, binary-search its `SourceFileId`, and rotate the complete stable
  group into place; a `TypeStore` that has ever used append-only `add` retains
  the original full-sort path so the optimization cannot assume a false prefix
  invariant. Three exact warm `goshposh` candidate repeats preserved both
  semantic-result fingerprints and the whole manifest while improving median
  terminal readiness **9.536%** to **15.055 seconds**, user CPU **7.548%**,
  active semantic completion **7.567%** to **12.972 seconds**, and median peak
  RSS **2.751%**. Evidence is in
  `support/performance/file-owned-index-splice-2026-08-01.json`.
- FactCollector now seeds its per-method inference map from a borrowed
  `TypeStore` domain view that selects only known method returns in the exact
  prior arena order; it no longer expands and clones every unrelated type fact
  first. The earlier shared mutable return-map experiment remains rejected.
  After the mixed-store safety qualification, three exact warm fresh-process
  repeats preserved the two complete semantic-result fingerprints, whole
  manifest hash, exact Project/Gem definitions, 607/607 gem and 358/358 Java
  hits, and zero resource leaks. Median terminal readiness is now **14.430
  seconds**, crossing the strict 15-second warm workspace target; active
  semantic completion is **12.217 seconds**, project navigation **357 ms**,
  dependency navigation **1.455 seconds**, and median peak RSS **1.662 GB**,
  leaving **115.3 MB** below the fixed ceiling. All **400** `ruby-analysis`
  tests pass. Evidence is in
  `support/performance/borrowed-method-return-view-2026-08-01.json`.
- Exact owner/name method resolution now selects a borrowed effective fact
  directly from MethodStore's already ordered bucket, applies the existing
  `Absent`/`Unavailable` precedence, collapses exact duplicates, and expands
  only one unique winner. MRO, execution-context applications,
  `method_missing`, ambiguity, and diagnostics remain engine-resolution owned.
  Three exact warm `goshposh` repeats preserve both semantic fingerprints, the
  whole manifest, exact Project/Gem definitions, all 607 gem and 358 Java hits,
  and zero resource leaks. Median terminal readiness improves another
  **1.886%** to **14.158 seconds**, user CPU **1.754%**, and active semantic
  completion **1.326%** to **12.055 seconds**; project navigation is **363 ms**
  and dependency navigation **1.443 seconds**. Median peak RSS is **1.740 GB**,
  **37.3 MB** below the fixed ceiling. The post-change profile reduces the old
  expanded exact-fact path from about **2.95%** to **0.07%** inclusive and
  final engine resolution from **9.68%** to **8.85%**. All **402**
  `ruby-analysis` tests pass. Evidence is in
  `support/performance/borrowed-exact-method-match-2026-08-01.json`.
- Method-reference resolution now borrows an already cached lookup chain rather
  than cloning its vector and FQNs for every method name. Chain construction
  remains single-sourced in engine resolution, and the borrow ends before
  recursive execution-context and `method_missing` lookup. Three exact warm
  `goshposh` repeats preserve both semantic fingerprints, the historical whole
  manifest, exact Project/Gem definitions, all 607 gem and 358 Java hits, and
  zero resource leaks. Median terminal readiness improves **1.514%** to
  **13.944 seconds**, active semantic completion **1.742%** to **11.845
  seconds**, and dependency navigation **7.831%** to **1.330 seconds**; project
  navigation is **361 ms**. Median peak RSS falls to **1.675 GB**, leaving
  **101.7 MB** below the fixed ceiling. The post-change profile contains no
  chain-vector clone below method-reference resolution. All **403**
  `ruby-analysis` tests pass. Evidence is in
  `support/performance/borrowed-method-lookup-chain-2026-08-01.json`.
- That resolution-local cache now stores compact interned owner IDs instead of
  cloning owner FQNs back into every exact method lookup. A test-only interner
  counter proves a cached Child-to-Parent chain performs one receiver
  existence probe for a second method, rather than one probe per owner. Three
  exact warm `goshposh` repeats preserve both complete semantic fingerprints,
  exact Project/Gem definitions, all 607 gem and 358 Java persistent hits, and
  zero resource leaks. Relative to the borrowed-chain checkpoint, median
  terminal readiness improves another **0.411%** to **13.886 seconds**, user
  CPU **0.733%**, active semantic completion **0.253%** to **11.815 seconds**,
  project navigation to **357 ms**, dependency navigation to **1.315
  seconds**, and median peak RSS **4.600%** to **1.598 GB**. Every run remains
  below the fixed 1.777 GB ceiling, with 24.2 MB minimum headroom. The
  symbolized profile reduces `NameRegistry::fqn_id` **20.6%** to **1.962%**
  inclusive, method-reference resolution **17.1%** to **3.455%**, and final
  engine resolution **10.0%** to **7.441%**. A broader direct-ID MRO traversal
  was rejected and reverted: the semantic fingerprint gate first caught an
  edge-only namespace promotion, and even after that boundary was fixed its
  three-run median regressed wall, CPU, and every readiness milestone. The
  focused edge-only regression remains. All **405** `ruby-analysis` tests
  pass. Evidence, including both rejected shapes, is in the same performance
  record.
- AST-time local receiver inference now reuses the lexical `VariableScopes`
  cursor already maintained by `FactCollector`, rather than scanning every
  scope, variable, and recorded location before most receiver lookups. The
  existing lookup still captures outer block locals, stops at method/class
  boundaries, and honors assignment order; editor-position query surfaces
  remain location based. A red regression observed two global scans for exact
  captured `User#save` and nested `String#upcase` receivers; green preserves
  both owners with zero scans, and all **406** `ruby-analysis` tests pass.
  A canonical-path, interleaved three-versus-three `goshposh` A/B preserves
  both complete semantic fingerprints, the exact manifest, 607/607 gem hits,
  358/358 Java hits, and zero resource leaks. Against its immediate control,
  median user CPU improves **2.151%**, wall **0.794%** to **13.841 seconds**,
  active semantic completion **0.593%** to **11.742 seconds**, and peak RSS
  **1.994%** to **1.562 GB**. Project and dependency probes remain within
  measurement noise at **356 ms** and **1.329 seconds**. Copied `/tmp`
  executables were explicitly excluded after their changed bundled-resource
  discovery altered stub/stdlib provenance and semantic fingerprints. Evidence
  is in `support/performance/fact-collector-active-scope-2026-08-01.json`.
- The follow-up fact-pass slice removes the lazy whole-file textual assignment
  fallback after exact lexical receiver lookup fails. The fallback reparsed
  right-hand-side fragments, ignored hard Ruby method boundaries, and could
  borrow a later same-named assignment from another method. A red regression
  proves that an untyped `user` parameter no longer becomes `User` because a
  different method later assigns `user = User.new`; captured block locals still
  resolve through `VariableScopes`. All **407** `ruby-analysis` tests pass. A
  canonical-path, interleaved three-versus-three `goshposh` A/B improves median
  user CPU **2.436%**, wall **1.555%** to **13.664 seconds**, active semantic
  completion **2.778%** to **11.550 seconds**, and peak RSS **8.409%** to
  **1.536 GB**. Project and dependency probes remain fast at **358 ms** and
  **1.338 seconds**. The exact semantic export manifest, dataset, Project/Gem
  definitions, 607/607 gem hits, 358/358 Java hits, and resource invariants are
  unchanged. Complete semantic-result fingerprints intentionally change because
  the invalid cross-method fallback had invented about 36,000 method candidates
  and 11,000 diagnostics per project; the corrected fingerprints are exact in
  all candidate runs. The post-change symbolized profile contains neither
  removed fallback function. Evidence is in
  `support/performance/fact-collector-source-ordered-local-receivers-2026-08-01.json`.
- A profile-driven attempt to share extension `tracked_call_names`
  classification between patch dispatch and enclosing-frame tracking was
  rejected and fully reverted. A long-lived registry snapshot improved median
  wall **1.222%** but raised median RSS **24.728%**, with every candidate above
  the fixed ceiling. A second design preserved the original short-lived
  registry ownership and improved wall **1.149%**, but median RSS still
  regressed **5.133%**, median CPU regressed **0.115%**, and one run reached
  **1.818 GB**—41.2 MB over the ceiling. Both designs preserved exact semantic
  fingerprints, manifest, extension tests, and navigation, but neither meets
  the production resource contract. The temporary probe and production changes
  were removed; evidence is in
  `support/performance/extension-call-classification-rejection-2026-08-01.json`.
- A subsequent attempt to eliminate per-method known-return map construction
  was also rejected and fully reverted. An incrementally maintained borrowed
  per-file `HashMap` improved median wall **2.994%**, but raised median RSS
  **11.023%** and exceeded the fixed ceiling in two runs. Replacing that table
  with a compact FQN-sorted vector improved wall **2.897%** and CPU **2.229%**,
  but made lifetime overlap worse: median RSS rose **22.347%** to **2.003 GB**,
  and every candidate run exceeded the ceiling. Both variants preserved the
  exact manifest, semantic fingerprints, Project/Gem navigation, cache reuse,
  and resource cleanup; neither is production-safe. Their code and temporary
  tests were removed. Evidence is in
  `support/performance/borrowed-method-return-context-rejection-2026-08-01.json`.
- A narrower follow-up disabled `TypeTracker`'s per-statement variable snapshots
  only where `FactCollector` consumes the inferred return and immediately drops
  the tracker. It retained no new per-file state and preserved the exact
  semantic manifest, both project fingerprints, Project/Gem navigation, all
  607 gem and 358 Java cache hits, and resource cleanup. The controlled
  three-pair A/B nevertheless showed no measurable production gain: median
  wall regressed **0.283%**, user CPU **0.072%**, active semantic readiness
  **0.381%**, and dependency navigation **0.417%**; the **1.777%** median RSS
  improvement remained within observed variance and one candidate exceeded the
  fixed ceiling. The profiled target was only **0.41%** inclusive. The code and
  temporary test were removed; evidence is in
  `support/performance/type-tracker-discarded-snapshots-rejection-2026-08-01.json`.
- Profiler schema 9 now records a stable path-independent semantic-result
  fingerprint for every isolated engine, covering exact declarations,
  references and targets, graph state, types, diagnostics, execution contexts,
  and source kind without hashing engine-local IDs. The fingerprint exposed a
  real worker-order race: parallel files queried and mutated the same live
  engine. Batches now pre-register every file in path order, collect complete
  file-owned facts against one unchanged batch context, and insert those facts
  sequentially through the ordinary deferred replacement lifecycle. JRuby
  replay uses the same collect-then-replace rule. Two fresh-process 1,024-file
  repeats became semantically identical but exceeded the RSS ceiling, so that
  intermediate was rejected. The accepted 512-file policy produced identical
  per-project fingerprints and engine counts across two fresh processes, plus
  a four-fresh-engine parallel regression. Median wall is **21.671 seconds**,
  active semantic completion is **17.210 seconds**, project navigation is
  **676 ms**, dependency navigation is **1.794 seconds**, and peak RSS is
  **1.750 GB**: **8.312%** above the full-ahead baseline and 27.3 MB below the
  explicit ceiling. All **391** `ruby-analysis`, **1,315** root library, and
  **12** profiler tests pass. Evidence is in
  `support/performance/deterministic-project-batches-2026-07-31.json`.
- A current-thread multi-root LSP regression saturates both scheduler slots
  with sibling CPU workers, then proves an already-ready isolated engine keeps
  the same real Go to Definition answer, produces hover, and completes a
  body-only edit plus refreshed definition within the 500 ms interactive
  budget. The full root library suite passes with 1,255 tests; the VS Code
  adapter suite passes with 34 tests.
- One right-aligned VS Code status item driven by structured snapshots; the old
  raw `$/progress` presentation, second item, and delayed hide timer are gone.
- Status request and notification snapshots are now sequenced under one server
  publication lock. Active-editor requests carry the document URI and
  reprioritize both the project scheduler and weighted resource queue, including
  switches between already-open files. The adapter caches the accepted
  aggregate and project vector together and rejects equal or older complete
  snapshots, so a delayed response cannot reapply stale project phases. A
  current-thread saturation test proves status routing and queued cancellation
  remain under 100 ms while the sole worker is occupied; focused Node tests
  cover the URI transport and reordered-snapshot rejection.
- Status notifications now render directly from the accepted complete snapshot
  instead of issuing a second runtime/status request for every transition. The
  single bottom-right item opens runtime configuration after readiness and a
  deterministic active-project-first Quick Pick while queued, indexing,
  cancelled, or failed. That view exposes every project root, generation,
  phase, progress, elapsed target, both navigation milestones, and the bounded
  failure reason. Counter-only server snapshots are coalesced behind one
  200-millisecond flush; generation, phase, aggregate scheduler state,
  readiness milestones, cancellation, and failure bypass the throttle. A
  focused publication-state regression proves fifty same-phase counters
  schedule only one flush and cannot delay replacement generations or terminal
  failures. The VS Code adapter suite passes with 38 tests.
- Watched-file input now passes through one server-owned 100-millisecond
  generation gate before extension callbacks or semantic mutation. A newer
  batch invalidates the older timer, retains only the final event per URI, and
  emits one deterministic URI-ordered batch. Shutdown invalidates pending
  watcher work. Gemfile, lockfile, auto-runtime marker, trusted project
  extension, and owning JRuby classpath changes are mapped to one replacement
  generation per affected project; ordinary closed source/RBS changes retain
  exact per-file replacement. Runtime replacement clears stale effective
  runtime, compatibility, classpath, import provider, external provenance, and
  engine state before rebuilding and reopening live documents. Focused tests
  cover a cross-notification source storm, duplicate runtime events producing
  one generation, project scoping, trust, auto versus explicit runtime markers,
  shutdown, failure cleanup, and changed winning-JAR implementation removal.
- Standalone project roots without a `Gemfile` now skip automatic installed-gem
  discovery and retain project/core/stdlib semantics only. The deliberate
  `includedGems` exception schedules one governed active-runtime global
  discovery after project scanning and still exposes only explicitly requested
  gems. Automatic Gemfile-based discovery now frames its JSON result with an
  exact protocol marker, so Bundler UI output cannot corrupt the payload. The
  complete release suite passes 1,277 library tests plus 11 profiler tests, and
  focused regressions prove both standalone branches.
- Process-local single-flight infrastructure whose producer is owned
  independently of any initiating waiter. Focused tests cover concurrent
  waiters, producer failure and retry, bounded retention/eviction, and
  cancellation of the initiating waiter without restarting shared work.
- A candidate checksum-keyed **per exact gem** dependency product now exists in
  the working tree. The earlier whole-closure product was rejected after
  measurement because its peak memory was not defensible. The current product
  defines project-neutral file-fact templates, exact source and semantic
  fingerprints, bounded collection lanes and ephemeral in-flight sharing, and
  per-consumer rebinding through the ordinary isolated-engine replacement
  lifecycle.
- Product binding validates the complete manifest, checksums, URI mapping, and
  file count before the first engine mutation, inserts all accepted gem
  products with deferred resolution, then resolves the consumer engine once.
  Producer collection no longer inserts dependency facts into a temporary
  semantic graph.
- A coordinator-level two-project semantic test proves one exact gem producer,
  rebinding into both isolated engines, definition/type/signature equivalence,
  consumer-correct external paths, ordinary replacement isolation, and a
  later independent producer after the flight completes. Profiler schema 3
  exposes producer, waiter,
  validation, rebinding/insertion, eviction, and retained-memory evidence.
- The concurrent dependency-product flight is accepted. Completed process
  retention is rejected: retaining 112 MB after the first configured-JRuby
  project made only 93 exact products totaling 3.3 MB reusable by the second
  and did not materially improve dependency readiness. Completed products are
  removed after overlapping waiters receive them. Sequential and fresh-process
  reuse now belongs to the pending demand-loaded persistent cache.
- Shared immutable known-namespace snapshots per indexing batch, eliminating a
  full engine namespace rebuild and clone for every dependency file.
- Stable extension applicability fingerprints that exclude per-document source
  URI/kind, plus skipped framework-extension dispatch for external sources
  while retaining built-in runtime providers such as JRuby.
- Persistent compiled-Wasm products now remove the measured fresh-process
  framework compilation bottleneck without sharing project semantics. Each
  extension source is read once per discovery generation; its exact source
  digest and Wasmtime target/compiler/config compatibility identity address a
  private, checksum-validated serialized module. Source and compiled logical
  payloads are capped at 64 MiB before allocation/decompression, invalid native
  artifacts are removed under the exact cross-process lock and rebuilt, and
  cache failure falls back to valid source compilation. Two controlled
  `goshposh` cold-to-warm pairs reduced median five-extension load time from
  **2.743 seconds** to **140 ms** (**94.9%**) and reduced peak RSS in both pairs.
  Every run retained the exact two project semantic-result fingerprints and
  manifest hash, 607/607 gem hits, 358/358 Java hits, and zero resource leaks.
  The five ordinary warm repeats still exceeded the previously fixed aggregate
  RSS ceiling, so this accepts only the extension product—not overall memory or
  readiness. Evidence is in
  `support/performance/persistent-compiled-wasm-2026-08-01.json`.
- Process-local classpath file products now coalesce identical checksum and
  bounded JAR-manifest work without sharing a project classpath or catalog.
  The 4,096-entry/16 MiB cache retains only SHA-256 plus manifest entries; raw
  JAR/JMOD/source bytes are dropped, and every consumer revalidates metadata and
  reapplies its own byte limits. Real two-project JRuby `goshposh` runs produced
  271 exact products for 360 lookups and reused 89 common runtime/JDK/Maven
  files while retaining only **156,158 bytes**. The reusable sibling classpath
  phase fell from **1.146 seconds** to **466 ms** with two workers and **451 ms**
  under sequential admission (**59-61% faster**); summed classpath time fell
  about **26%**. Both scheduling shapes preserved the exact prior project
  classpath fingerprints, semantic-result fingerprints, and whole semantic
  manifest hash with zero cache failures or resource leaks. Evidence is in
  `support/performance/classpath-file-product-single-flight-2026-08-01.json`.
- An unconditional zero-retention flight around each persistent Java artifact
  lookup was measured and rejected. Across three candidate `goshposh` runs,
  all **1,074** lookups became independent producers and **zero** identical
  keys joined. The median wall result moved by only **-0.62%**, while user CPU
  regressed **1.37%**, project/dependency/semantic readiness regressed
  **1.1-2.1%**, internal peak RSS regressed **4.02%**, and one candidate run
  was a severe outlier. Exact semantic manifests, project fingerprints, and
  navigation remained equal, so the code was fully reverted rather than
  retaining an unproductive synchronization layer. Evidence is in
  `support/performance/java-artifact-ephemeral-flight-rejection-2026-08-01.json`.
- Exact parsed Java class metadata now has a measured bounded sequential-reuse
  path. A server-owned 256-entry/256-MiB cache retains checksum-keyed artifact
  products whose archives and project declarations share immutable
  `Arc<ClassFile>` allocations; per-project paths, classpath order, duplicate
  winners, providers, facts, and engines remain isolated. The 128-MiB pilot was
  rejected after cyclic eviction retained only 117 identities and reused only
  22 of 190 repeats. Across three interleaved 256-MiB candidate/baseline pairs,
  the accepted design retained all 168 exact identities, reused all 190
  repeated lookups, reduced persistent reads from 358 to 168, and improved
  median wall **4.34%**, user CPU **4.14%**, project readiness **3.86%**,
  dependency readiness **3.23%**, internal RSS **17.13%**, and external RSS
  **11.93%**. Every candidate stayed below the fixed 1.777-GB RSS ceiling and
  all semantic manifests, project fingerprints, and definition results were
  byte-identical. Process-wide reuse is exposed in profiler schema 12 and the
  authoritative indexing detail picker. Evidence is in
  `support/performance/shared-java-class-metadata-cache-2026-08-01.json`.
- Runtime stdlib ownership is now exact and executable-location independent.
  Discovery invokes only the owning project's selected executable and Java
  home, never the server `PATH` or guessed runtime homes; bundled core stubs can
  no longer be rediscovered or reclassified as stdlib. Identical project probes
  share one 32-entry/1 MiB process-local single-flight product keyed by the
  canonical runtime executable identity plus Java home. Three paired
  `goshposh` runs preserved byte-identical semantic exports while reducing
  median terminal readiness from **15.085 seconds** to **13.710 seconds**
  (**9.1%**) and retaining only **315 bytes**. Evidence is in
  `support/performance/runtime-stdlib-path-single-flight-2026-08-01.json`.
- Project-source collection now consumes each owned `(URI, source)` input and
  moves the source buffer directly into its `RubyDocument`; the parallel
  priority and exhaustive partitions no longer clone every project file before
  fact collection. Three exact `goshposh` pairs preserved the complete semantic
  manifest and both project fingerprints while improving median terminal wall
  **1.804%**, active semantic completion **2.206%**, project collection
  **3.159%**, and peak RSS **2.278%**. Evidence is in
  `support/performance/project-source-move-ownership-2026-08-01.json`.
  A broader attempt to share the remaining `RubyDocument` source allocation
  through `Arc<String>` was rejected and reverted: its timing gain was below
  one half percent while median RSS increased **21.105%**. Do not repeat that
  ownership shape without new lifetime evidence. The rejection is recorded in
  `support/performance/shared-ruby-document-source-rejection-2026-08-01.json`.
- Prism-location range conversion no longer eagerly allocates formatted start
  and end overflow messages for every ordinary constant, method, diagnostic,
  raise, and `super` reference. A typed boundary preserves the complete
  fail-fast invariant message only on actual `u32` overflow. Three interleaved
  warm-cache pairs preserved the complete semantic manifest, both project
  fingerprints, exact Project/Gem definitions, 607/607 gem hits, 168/168
  persistent Java hits, and zero resource leaks. Median project collection
  improved **3.116%**, user CPU **2.866%**, active semantic completion
  **1.806%** to **11.853 seconds**, and terminal wall **1.537%** to **13.319
  seconds**. Median external RSS moved **1.647%**, but every candidate remained
  at least 217 MB below the fixed ceiling. Evidence is in
  `support/performance/text-range-overflow-formatting-2026-08-01.json`.
- Extension applicability is now evaluated lazily once per file traversal and
  reused by enclosing-frame tracking, frame ownership, and Wasm dispatch. The
  snapshot is only a registry-fingerprinted bit vector; it retains no Wasm
  instances, registry snapshot, project context clone, engine, or semantic
  facts. A changed exact locked-gem version receives a new fail-closed
  decision, while a racing registry replacement falls back to exact current
  applicability. The symbolized `goshposh` profile reduces locked-gem
  applicability from **1.64%** of total application CPU to **0.04%**. Three
  valid fully warm baseline/candidate pairs preserve both complete semantic
  fingerprints, the sorted full semantic export manifest, exact Project/Gem
  definitions, 607/607 gem hits, 168/168 persistent Java hits, and zero
  resource leaks. Median user CPU improves **3.191%**, wall **0.693%** to
  **13.082 seconds**, and active semantic completion **0.534%** to **11.539
  seconds**. Median RSS moves **3.518%** amid wide allocator variance, but every
  candidate remains below the fixed ceiling with at least 72.8 MB headroom.
  This is distinct from the rejected long-lived extension registry snapshots
  and combined call-classification shapes. Evidence is in
  `support/performance/extension-applicability-snapshot-2026-08-01.json`.
- Extension semantic-target seeding now carries one dependency applicability
  fingerprint computed by the isolated project's existing
  `ProjectContextSeed`, rather than serializing and hashing the complete locked
  gem vector once per project file. The 32-byte identity changes on an exact
  dependency refresh; the ordinary synthetic Stub file is then replaced in
  the same isolated engine, removing stale targets when a framework version no
  longer applies. It retains no engine, extension instance, registry snapshot,
  source buffer, or semantic fact. The formerly sampled
  `ensure_semantic_seed_facts` path falls from **2.03%** of application CPU to
  no samples in the post-change 1 kHz symbolized profile. Three valid
  interleaved warm `goshposh` pairs preserve both project fingerprints, the
  complete semantic manifest, exact Project/Gem definitions, 607/607 gem hits,
  168/168 Java hits, and zero resource leaks. Median wall improves **1.900%**
  to **13.136 seconds**, user CPU **2.028%**, active project navigation
  **1.863%** to **5.269 seconds**, dependency navigation **1.440%** to **11.502
  seconds**, and active semantic completion **1.427%** to **11.539 seconds**.
  Median RSS moves **1.349%**, while every candidate stays below the fixed
  ceiling with at least 138.4 MB headroom. Evidence is in
  `support/performance/extension-semantic-seed-fingerprint-2026-08-01.json`.
- A follow-up that delayed each file's complete extension `ProjectContext`
  until its first syntactically tracked call was measured and rejected. Common
  names such as `include`, `extend`, `before`, and framework DSL calls meant
  nearly every real `goshposh` file still materialized the context. The exact
  symbolized path moved only from **56.953 ms** to **55.331 ms** of sampled CPU.
  Three valid fully warm pairs preserved the complete semantic manifest and
  exact Project/Gem navigation, but median active dependency readiness
  regressed **0.255%**, active semantic completion regressed **0.359%**, median
  RSS rose **2.167%**, and one candidate exceeded the fixed RSS ceiling by
  57.5 MB. The slice was reverted; the restored release profiler is
  byte-identical to the accepted semantic-seed binary. Do not retry this shape
  without first narrowing the syntactic tracking set or proving a compact ABI
  context that does not weaken extension applicability or provenance. Evidence
  is in
  `support/performance/lazy-extension-context-rejection-2026-08-01.json`.
- A broad borrowed file-type-fact iterator was measured and rejected. It
  reduced median terminal wall **1.671%** and user CPU **1.797%**, but did not
  improve active-project readiness, slowed the live dependency probe
  **5.977%**, and increased median peak RSS **4.091%**. The experiment was
  fully reverted and its release profiler is byte-identical to the accepted
  semantic-seed checkpoint. Do not repeat broad per-file borrowing; any future
  type lookup must be an exact owner/name/position query with independent
  evidence. The rejection is recorded in
  `support/performance/borrowed-file-type-facts-rejection-2026-08-01.json`.
- Gem product cache-key preparation now derives static Java imports, dotted
  and canonical proxy references, and the remaining JRuby DSL markers from one
  Prism parse and one combined visitor. A plain gem source previously paid
  three parses solely to return `false`. The focused red test observed three
  parses and now proves exactly one. Three interleaved fully warm `goshposh`
  pairs preserve both project semantic fingerprints, the complete semantic
  export manifest, exact provenance, 607/607 gem hits, 168/168 Java hits, and
  zero resource leaks. Median terminal wall improves **3.878%** to **12.485
  seconds**, user CPU **1.846%**, active project readiness **4.092%** to
  **5.157 seconds**, active dependency readiness **5.035%** to **10.977
  seconds**, and active semantic completion **5.053%** to **11.011 seconds**.
  Every candidate remains below the fixed RSS ceiling with at least 93.2 MB
  headroom. The parallel gem-indexer test race that mutated process-global
  `HOME` is also serialized and the complete workspace test suite passes.
  Evidence is in
  `support/performance/one-pass-jruby-gem-prefilter-2026-08-01.json`.
- A graph node-definition ownership index that prevents cold insertion of every
  new file from scanning all existing graph nodes for nonexistent prior facts.
- Exact resolved-callee return-type queries that avoid rebuilding an owner MRO
  after lookup has already selected the declaration, plus a revision-bound,
  per-collector resolved-callee query cache capped at 64 entries and invalidated
  on semantic replacement.
- A generation-local exact JRuby provider handoff at bounded project-batch
  boundaries. Provider-aware batches use ordinary file-owned collection and
  cannot enter the replay set; only earlier providerless files are replayed.
- Deterministic per-project profiler records plus an aggregate summary with
  machine, build, dataset, runtime, CPU, peak-RSS, engine-size, source-byte, and
  single-flight evidence. Schema 9 includes a stable complete semantic-result
  fingerprint per isolated project rather than relying on aggregate counts and
  spot probes alone.
- The macOS ARM64 VSIX is built from the dirty checkout's freshly rebuilt
  target binary, smoke-tested from the extracted archive, and installed as
  `naveenraj.ruby-fast-lsp@0.2.6`. The packaged smoke now waits for the
  sequenced `ruby-fast-lsp/indexing/statusChanged` ready snapshot instead of
  the removed legacy `$/progress` presentation, then proves all five bundled
  framework guests load, ERB host completion stays outside Ruby regions, and
  JRuby class/member navigation, hover, completion, signature help, references,
  and runtime identity work without developer paths. The archive installed at
  the latest explicit editor checkpoint hashes to
  `d1e05d3df5a5ac96fd5b95f3e5f06b9864f77b843f6ee7c6180d959a1f8f022c`;
  its packaged and installed native binary both hash to
  `cc120df4a30436bab3fb80afbe721185495f09534c3c77e03f0d3b21076a70fa`.
  It includes the accepted TextRange allocation, per-file extension
  applicability, dependency semantic-seed, and one-pass JRuby gem-prefilter
  slices and passed the complete extracted-archive smoke at the final local
  gate. A prior real installed-binary LSP
  session over the two isolated JRuby `goshposh`
  projects reached authoritative ready state in **14.456 seconds**, exposed
  358 Java artifact lookups with 168 producers and all 190 duplicates reused,
  and resolved both project-owned `UserPmm` and exact locked-gem
  `BSON::ObjectId` definitions. This closes package assembly and installed
  real-workspace validation for the current slice, not the remaining runtime
  switching, failure, edit-isolation, or five-second project/semantic budgets.
- Initial per-phase timing instrumentation and a real 67-project `goshposh`
  baseline.

These are foundations, not completion. The current evidence-backed rating is
**8.8/10**. The current slice must not be rated 9/10 or treated as
production-finished until the following gaps are closed:

- Same-pass inheritance validation is fixed for the reproduced alias,
  self-cycle, cycle-closing, and conflicting-superclass cases. Engine ingestion
  still needs equivalent lifecycle tests for cycles assembled across files,
  unresolved-edge retries, cached products, and extension patches.
- Scheduler fairness, starvation resistance, and ready-engine
  definition/hover/edit responsiveness are proven with deterministic
  multi-project tests. Retained-memory accounting and the remaining
  request/status detail surfaces still need equivalent proof.
- CPU-heavy indexing work is proven not to block the async LSP reactor. A
  current-thread saturated-worker test also proves active-document status
  routing remains responsive, updates both priority owners, and cancels queued
  weighted work without a stale queue entry. Queries against ready facts have
  direct saturated-worker coverage.
- Cold coordinator and gem-product work now obey one candidate process-wide
  CPU, transient-memory, and I/O admission policy, and nested Rayon work cannot
  escape the owned pool. Runtime probes, extension-requested processes, and
  editor linter/formatter processes share the same admission queue. The default
  512 MiB transient budget and two I/O slots still require real `goshposh`
  comparison before acceptance. Open/change/save index-time extension guest
  calls and interactive JRuby materialization now run inside the admitted
  semantic pass; extension registry loading/reloading and request-time
  document-symbol/code-lens guests also have explicit weighted admission.
  JVM subprocesses now have a measured hard resident-memory kill boundary.
  Shared completed products are bounded by weighted retention or deliberately
  ephemeral; isolated project-engine aggregate heap and final process RSS still
  require acceptance on the real umbrella workspace. One large parallel phase
  currently reserves the full indexing
  pool; real evidence must decide whether project lane partitions are required
  in addition to the host lanes already reserved for the reactor.
- Request-driven navigation is now bounded, generation-safe, and exact for
  project files and locked gems, including requests arriving at either
  dependency frontier. The warm dependency probe passes its three-second
  target at a current **1.253-second** three-repeat median. The exact project
  probe passes its strict one-second warm target in all current repeats at a
  **357 ms** median. The exact two-project workspace also passes the 15-second
  warm terminal target at a current **12.485-second** median. The remaining
  warm performance gaps are active project-navigation readiness at **5.157
  seconds** rather than five seconds and active semantic completion at **11.011
  seconds** rather than five seconds. Median peak RSS is **1.671 GB**, and
  every repeat remains below the fixed 1.777 GB ceiling with **93.2 MB**
  minimum measured headroom.
  Cold-cache, one-project-change, failure, active-priority, and real-workspace
  installed-VSIX acceptance remain required.
- The accepted per-gem product coalesces concurrent work but deliberately
  retains no completed values. Shared work must now extend through a
  demand-loaded persistent protocol and to the measured expensive immutable
  stdlib/runtime, signature, exact source-map, extraction, and decompilation
  products. Per-artifact JAR/JMOD metadata and compiled-Wasm modules already use
  the accepted persistent protocol. Cache identities use content checksums and
  every semantic fingerprint, not package version or path alone.
- A reported cache hit is not accepted as reuse evidence until the validated
  product has been rebound into the requesting isolated engine and real
  definition/type queries return the same results as a cold build. Measure
  lookup, deserialization, validation, rebinding, insertion, and retained-memory
  cost; do not optimize a hit counter while leaving the expensive semantic work
  unchanged.
- Configured-JRuby cold and sequential profiles distinguish completed hits from
  joined in-flight work. Completed retention was removed after proving that
  only 3.3 MB of 112 MB retained after project one could be reused by project
  two. The adjacent ephemeral run retained zero product bytes with effectively
  unchanged total and dependency wall time, while preserving exact
  `BSON::ObjectId` and `TimeUnit` navigation. Persistent reuse must improve
  readiness without recreating this resident-memory cost.
- The product identity and source-precedence contract now has focused
  fresh-process coverage for analyzer source, parser/dependency lock, explicit
  product and payload schemas, exact gem name/version/platform/source kind,
  declared dependency context, runtime/platform/provider semantics, extension
  applicability, project-neutral seed semantics, exact content, and
  serialization format. A real stale `goshposh` cache exposed that package
  version plus a manually bumped schema was insufficient. The accepted fix
  embeds a build-generated SHA-256 of the complete `ruby-analysis` source tree,
  root fact composition, and JRuby import/catalog producer code. The stale
  cache then missed all 512 distinct gem identities while reusing all 358 Java
  and five Wasm products; its immediate fresh-process replay hit all 607 gem
  identities and produced the exact same semantic export SHA and both project
  fingerprints. Evidence is in
  `support/performance/gem-semantic-producer-identity-2026-08-01.json`.
- Persistent cache publication, corruption recovery, schema/version
  invalidation, cross-process contention, disk accounting, automatic bounded
  cleanup, and a safe show/clear command are implemented and tested.
- Persistent entries are bounded before allocation and deserialization, use
  private user-cache permissions, and contain only immutable external/runtime
  derived products in this goal. Workspace-owned source, unsaved buffers,
  diagnostics, and project semantic state are not persisted or shared.
- Source/RBS, Gemfile, lockfile, auto-runtime marker, trusted project-extension,
  configuration, folder, and classpath lifecycle paths now invalidate the
  owning project or exact file. Existing persistent gem products now prove that
  source content, lock closure, core/runtime semantic seed, and JRuby classpath
  changes reserve a new identity instead of selecting old facts. Persistent
  Java artifact products likewise reject changed byte identities. Fresh-process
  rebinding proves folder/path changes retain exact consumer provenance, while
  bounded cleanup touches only Ruby Fast LSP-owned products. Every future
  persistent product must add equivalent invalidation proof before acceptance.
- Bursts of duplicate filesystem watcher events are normalized and coalesced
  into one final URI batch and one owning-project replacement generation. A
  packaged VS Code watcher-storm acceptance remains.
- The VS Code item has generation-safe elapsed/target presentation and an
  authoritative all-project Quick Pick. That view now joins each project's
  exact server-reported runtime, JDK, and classpath identity and labels
  persistent gem/Java cache plus gem single-flight counters as process-wide
  reuse evidence. Restart now suspends the old transport, coalesces concurrent
  callers, resets sequence only after replacement, and rejects delayed events
  after disposal. Existing dynamic-folder rehoming, deepest-owner selection,
  active-project reprioritization, and reordered-response tests complete the
  focused lifecycle matrix; packaged acceptance remains.
- Snapshot publication is bounded for counter-only changes while phase,
  generation, scheduler aggregate, readiness, terminal, and failure changes
  remain immediate. A real initialized LSP client-socket regression proves
  fifty counter changes emit one bounded flush and the navigation-ready phase
  bypasses throttling with strictly increasing sequences.
- The profiler now records queued-to-stage readiness milestones and
  active-document navigation probes. A probe repeatedly executes the real
  engine definition query during indexing, records its first successful phase,
  generation, sequence, exact locations, owning project, query latency, and
  target source kinds, then retains the ordinary post-index result for
  comparison. A project reaching a terminal state without a semantic answer is
  a profiler failure rather than false readiness evidence. Persistent-cache
  hits/misses/rejections and defensible physical disk-read evidence remain.
- Cold, warm-process, fresh-process persistent-cache, one-project-change,
  runtime-change, failure, and active-priority measurements pass the budgets
  below from the packaged extension.

## M0 Baseline Evidence

First scheduler-backed cold run recorded on 2026-07-26 using the release
`profiler`, `/Users/naveenraj/goshposh`, and an internal concurrency limit of
two:

- Project discovery found **67 isolated Gemfile roots**. The set includes
  repeated devops worktree trees plus `admin`, `server`,
  `server-devops-5441`, `show-notifier`, and smaller tools.
- Cold all-project completion took approximately **5 minutes 58 seconds**
  (18:20:54–18:26:52 local profiler timestamps), versus the 90-second target.
- Small projects commonly completed project-owned fact collection in roughly
  **5–100 ms**, while repeatedly spending roughly **0.4–6.3 seconds** in
  runtime/core/stdlib/gem work. Project navigation is therefore not the primary
  umbrella bottleneck.
- Every small MRI project rebuilt the same **132 core stub files**. Estimated
  engine heap for a mostly core-only project was approximately **5.4–5.9 MB**.
- `server` indexed 16,396 files and 102,656,605 source bytes with an estimated
  333.8 MB engine heap. `server-devops-5441` concurrently indexed 16,429 files
  and 103,133,071 source bytes with an estimated 335.6 MB engine heap.
- Logs showed the two server trees parsing the same locked gem names and
  versions concurrently. This proves the need for checksum-keyed gem
  single-flight reuse rather than additional unbounded workers.

The profiler now emits deterministic per-project evidence and a machine-readable
aggregate with machine/build fingerprints, CPU, peak RSS, block I/O,
source/engine bytes, project/runtime fingerprints, and process-local
single-flight counters. The first post-change attempt exposed a local
`StringScanner` superclass cycle while indexing real dependency code. A
real-source-derived regression now preserves the syntactic subclass identity
when a flow-insensitive compatibility alias would otherwise reopen its own
superclass, and same-pass graph insertion rejects cycle-closing and conflicting
inheritance facts before inference.

A schema-3 representative cold sample on 2026-07-28 proved the new semantic
readiness measurement end to end. Cross-file Go to Definition from
`app/controllers/users_controller.rb` to the project-owned
`app/services/user_service.rb` first succeeded after **900 ms** during
`indexingDependencies`; project-navigation readiness was also 900 ms,
dependency-navigation readiness was 952 ms, and full semantic completion was
964 ms. The successful target was verified as `SourceKind::Project`, and the
same query after completion returned the identical location. This is a
functional navigation measurement, not an inference from status timestamps.

The corrected schema-2 release profiler completed all 67 projects on
2026-07-27:

- Exact profiler binary SHA-256:
  `31595435557dac39a63dd61951714f07b833e149ac359a0e124db274603bc4c5`;
  tracked diff SHA-256:
  `484c945de3ff2f10bbd08b9e58ff808d15d1bfe0bd4f4bdaf65e409139fca7f9`;
  base source revision:
  `fee94ffb7f2d1b723b9b11086de72350e5d5da09`.
- Aggregate dataset fingerprint SHA-256:
  `3c6835b7166c67e44bc0a563ba62d3289c76b732fd2263e9578e9103da12c836`.
- Reference machine: Apple M4 Pro, 14 logical CPUs, 24 GiB physical memory,
  macOS Darwin 25.2.0; scheduler concurrency remained two.
- Complete wall time was **331.852 seconds (5m32s)**, versus the initial
  approximately 358-second run. This is only about a 7% improvement and remains
  far outside the 90-second budget.
- User CPU was **513.937 seconds**, system CPU was **25.337 seconds**, and peak
  RSS was **2,003,828,736 bytes (1.87 GiB)**. The second measured peak remains
  within 10% of the preceding 1.873 GB run.
- The isolated engines retained **63,310 files**, **480,674,555 source bytes**,
  and an estimated **1,417,167,866 bytes** of engine heap.
- Core-stub single-flight recorded 67 lookups, one producer, one joined flight,
  65 hits, zero failures, and one retained template.
- Summed dependency work was **566.411 seconds**. The two server trees spent
  **227.014 seconds** and **222.547 seconds** indexing dependencies while their
  project-owned passes took only **8.100 seconds** and **7.978 seconds**.
- Logs directly show both large engines concurrently processing the same locked
  vendor gem identities. Checksum-keyed gem source/fact products are therefore
  the next measured reuse target.
- With every project submitted at background priority, queued-to-project-ready
  latency was **662 ms minimum, 62.225 s p50, 88.029 s p95, and 326.022 s
  maximum**. This is not an active-project measurement; it proves why
  active-document priority and a separate acceptance run are required.
- Queued-to-dependency-ready latency was **10.236 s minimum, 64.073 s p50,
  88.271 s p95, and 330.466 s maximum**.

Durable sanitized evidence is checked in at
`support/performance/multi-root-m0-2026-07-27.json`. The corresponding raw local
evidence is `/tmp/ruby-fast-lsp-goshposh-m0-schema2-20260727.jsonl` with
diagnostics in `/tmp/ruby-fast-lsp-goshposh-m0-schema2-20260727.log`. M0 remains
open for real active-document query probes.
The first implemented reuse slice prepares one neutral core-stub engine template
per compatibility identity and clones it into isolated engines. It does not yet
deduplicate gem facts or persistent cross-process work.

The first symbolized CPU-profile campaign for the real `server` dependency
phase is also complete. It found semantic preparation and graph lifecycle work,
not Prism parsing, to be the dominant initial cost. Three correctness-preserving
hot-path fixes reduced the same cold `server` run as follows:

- Initial symbolized run: **132.259 seconds total**, **120.860 seconds
  dependencies**, **185.240 seconds user CPU**, and **977.7 MB peak RSS**.
- After shared known-namespace snapshots: **67.460 seconds total** and
  **55.836 seconds dependencies**.
- After stable extension applicability and external-source dispatch fixes:
  **55.010 seconds total** and **38.395 seconds dependencies**.
- After indexed graph node-definition ownership: **40.383 seconds total**,
  **29.206 seconds dependencies**, **77.394 seconds user CPU**, and **746.7 MB
  peak RSS**.

The final run retained the same 16,396 files and 102,656,605 source bytes, with
effectively unchanged engine heap, while reducing total wall time by 69.5% and
dependency time by 75.8%. It remains above the 15-second cold dependency target,
so this is evidence for the next cache/reuse work, not completion. Local raw
profiles live under `/tmp/ruby-fast-lsp-server-*-20260727.*`; preserve durable
sanitized summaries before relying on temporary files.

The next two measured engine slices then reduced the same cold project further:

- Exact resolved-callee return-type lookup reduced total time to **32.346
  seconds** and dependency time to **21.252 seconds**.
- A revision-bound per-collector method-return query cache reduced total time to
  **31.407 seconds** and dependency time to **20.812 seconds**, while retaining
  the same project file/source coverage.

A broad engine-wide MRO cache was explicitly rejected after profiling because
its retained memory and peak RSS were not defensible. A narrower
revision-invalidated cache for only the top-level/Object fallback chain passed
all 378 `ruby-analysis` tests and reduced the identical cold `server` run to
**23.512 seconds total** and **15.564 seconds for dependencies**, with
**56.961 seconds user CPU**, **1,066,516,480 bytes peak RSS**, the same 16,396
files and 102,656,605 source bytes, and effectively unchanged engine heap. This
25.2% dependency improvement over the preceding accepted run is retained.

Future reuse must remain bounded and must justify both latency and memory. The
current dominant problem is no longer an unmeasured parse-cache hypothesis: it
is construction and rebinding of reusable, project-neutral dependency semantics
across isolated engines.

The candidate gem product was then measured and redesigned rather than accepted
on its first shape:

- A closure-wide product reached roughly **51.8 seconds** wall time and
  **1.19–1.22 GB** peak RSS, so that design was rejected.
- Removing temporary producer-engine insertion reduced product construction,
  but unbounded parallel collection still raised peak RSS to roughly
  **1.30 GB** and was also rejected.
- Splitting the cache identity and work unit per exact selected gem, validating
  before binding, moving immutable source payloads into the consumer, and
  bounding file collection produced the current candidate.
- With 12 bounded collection lanes, one measured `server` dependency phase was
  approximately **15.30 seconds**, product construction consumed approximately
  **4.57 seconds** in aggregate, binding approximately **3.29 seconds**, and peak
  RSS was approximately **1.02 GB** before the final retention-bound change.
- A real concurrent `server` plus `server-devops-5441` run performed **460**
  lookups with **230** producers, **216** joined flights, **14** completed hits,
  zero failures, and **27,334** total bound files. This proves shared production
  work, but not the required sequential warm-cache latency.
- A configured JRuby 9.2.21.0 cold `server` run then completed in **81.288
  seconds** internally (**85.35 seconds** externally), with **27.639 seconds**
  in core/runtime preparation, **19.262 seconds** in dependencies, **24.378
  seconds** in project indexing, and approximately **1.61 GB peak RSS**. Exact
  `BSON::ObjectId` navigation succeeded.
- The following scheduler-concurrency-one `server` then
  `server-devops-5441` run completed in **159.690 seconds** internally
  (**164.38 seconds** externally). It performed **601** gem-product lookups:
  **508** producers, **93** completed hits, no joined flights, no failures, and
  **232** evictions, retaining **134,102,429 bytes** across 276 entries.
  Transactional binding succeeded 601 times for 29,035 files. Exact
  `BSON::ObjectId` navigation resolved to the consumer's extracted locked Java
  gem, and `TimeUnit` resolved to the consumer's exact JRuby/JDK source.
- That sequential run reached **2,260,713,472 bytes peak RSS**, above both the
  recorded two-project baseline and the goal's 10% growth ceiling. The second
  project reused only 93 of 301 lookups and still required 74.929 seconds.
  Therefore the present retention/admission shape is rejected as a production
  default even though its semantic identity and rebinding are correct.
- Whole-project JRuby classpath fingerprints were also proven too broad for
  ordinary Ruby gem products. The current key includes the runtime-provider
  fingerprint only when a gem's source actually uses Java imports, Java proxy
  references, or Java-specific calls; Java-sensitive products retain the exact
  classpath identity. Focused tests cover both sides of this boundary.
- A real RxJava class exposed an overly restrictive aggregate classfile
  attribute limit. The bounded parser now accepts the JVM's valid 65,535
  aggregate count while retaining independent class-size, member-count,
  per-attribute byte, annotation, and recursion bounds.
- JRuby package imports no longer eagerly source/decompile every class in an
  imported package. All bounded signatures remain available, but implementation
  source is materialized only for explicit or uniquely referenced classes.
  This reduced the measured JRuby core/runtime preparation phase from
  approximately 119.9 seconds to 27.6 seconds.
- A controlled adjacent preflight run proved that the 93 reusable second-project
  products totaled only **3,317,982 bytes**, after retaining **112,127,594
  bytes** from project one. The retained run completed in **154.528 seconds**
  internally with **38.916 seconds** of summed dependency work.
- The accepted ephemeral-flight run retained **zero** completed product bytes,
  completed in **155.475 seconds** internally with **38.633 seconds** of summed
  dependency work, and preserved exact `BSON::ObjectId` plus Java `TimeUnit`
  definition targets. The sub-second wall difference is not a material benefit
  for 112 MB of resident templates. macOS peak counters varied, so both RSS and
  peak-footprint counters are preserved rather than selecting the favorable
  one.

The sanitized decision record is
`support/performance/gem-single-flight-retention-2026-07-27.json`; its raw
profiles remain under `/tmp`. The focused post-change gates, exact working-tree
and binary checksums, dataset fingerprints, concurrency, cache state, semantic
probes, and counter limitations are recorded together. Packaged behavior and
the full local completion gate remain pending.

## Correct Architecture

### Ownership

```text
VS Code adapter
  renders active-project snapshot
  rejects stale generation/sequence
  never computes semantic readiness
              ▲
              │ structured snapshot/request
              │
server indexing scheduler
  owns queue, priority, concurrency, generation,
  aggregate state, cancellation, failure, publication
              │
              ▼
per-project indexing coordinator
  performs one project's phases
  reports typed phase counters/results upward
              │
              ▼
isolated AnalysisEngine
  owns that project's semantic truth and replacement lifecycle
```

- `src/` owns scheduling, workspace lifecycle, progress publication, and the
  LSP/editor transport.
- `ruby-analysis` owns semantic facts, graphs, inference, queries, and
  diagnostics. It must not know about status bars or scheduler policy.
- A project coordinator must not publish the global progress token directly.
- The editor must not infer project roots, runtime mappings, readiness, or
  percentages from logs and timing.
- One project continues to own one isolated `AnalysisEngine`.
- Shared caches may contain only immutable, validated, project-neutral source
  products. Every engine still receives file-owned facts through its ordinary
  replacement path.

### Authoritative state model

Represent project state explicitly rather than with a boolean:

```text
Discovered
  -> Queued
  -> ResolvingRuntime
  -> DiscoveringInputs
  -> IndexingCore
  -> IndexingProject
  -> ProjectNavigationReady
  -> IndexingDependencies
  -> DependencyNavigationReady
  -> ResolvingSemantics
  -> PublishingDiagnostics
  -> Ready

Any active generation may become Failed or Cancelled.
A new generation supersedes the old generation; it does not mutate it.
```

Each published project snapshot must include:

- Canonical project root and workspace-container root.
- Run generation and monotonically increasing sequence number.
- Current state and phase.
- Completed and total work in a well-defined unit.
- Whether the total is known; never invent a percentage before it is.
- Reused versus newly processed file/byte counts.
- Queue position and whether the project is actively scheduled.
- Runtime summary when resolved.
- Start time, most recent transition time, and completed time.
- A bounded actionable failure summary when failed.

The aggregate snapshot must include:

- Scheduler generation.
- Counts for discovered, queued, active, ready, failed, and cancelled projects.
- Active worker count and concurrency limit.
- Aggregate completed/known work with a documented weighting rule, or no
  aggregate percentage when the work denominator is not defensible.

State transitions must be validated. A stale task cannot publish into a newer
generation. A failed project cannot transition to ready without a new
generation. Removing a project cancels its work and clears its published state.

## Scheduling and Performance Policy

### Measure before changing behavior

Add structured timings and counters for every project and phase:

- Queue wait, wall time, and CPU time.
- Files and source bytes discovered, read, parsed, converted to facts, resolved,
  reused, skipped, and replaced.
- Runtime, Bundler, stub, stdlib, gem, JVM/JAR, extension, project, graph,
  diagnostics, and publication time.
- Cache lookups, hits, misses, invalidations, bytes, and reason for rejection.
- Cache producer time, waiter time, validation, deserialization, rebinding,
  insertion, eviction, retained bytes, and whether reuse removed physical reads,
  parsing, fact construction, or only one smaller phase.
- Peak and final RSS plus estimated engine heap per project.
- Scheduler active workers, task steals/reprioritizations, cancellations, and
  failures.

Record separate baselines for:

1. One large project opened directly.
2. The full `/Users/naveenraj/goshposh` umbrella folder.
3. Cold cache.
4. Warm process with unchanged inputs.
5. Fresh process with valid persistent cache.
6. One project changed.
7. Runtime or lockfile changed in one project.

Do not accept a faster all-project total that makes the active project slower
or creates excessive peak memory and disk contention.

### Interactive request latency (document highlight)

VS Code requests `textDocument/documentHighlight` whenever the caret sits on a
symbol in the active editor. That path is interactive and concurrent with
Go to Definition, hover, folding, semantic tokens, and CodeLens.

Measured on `/Users/naveenraj/goshposh` `server/lib/api_app.rb` (2026-08-01):

1. Caret on a method name triggered document highlight.
2. The handler ran project-wide `find_references` (~10.5 s) and kept one
   same-file hit.
3. That synchronous work blocked tower-lsp's request-poll loop
   (`buffer_unordered`), so F12's definition request waited on the wire until
   highlight finished even though goto itself completed in ~1–2 ms.
4. CodeAction / Full document sync / CodeLens were not the stall (linter none;
   flush 0 ms; CodeLens ~30 ms).

Required direction:

- Document Highlight must use same-document reference lookup only.
- Multi-second or project-wide reference work must stay off the async
  request-poll path so one feature cannot stall sibling interactive requests.
- Re-measure caret-on-symbol then F12 on `api_app.rb` after the fix; goto must
  no longer wait behind highlight.

### Bounded, prioritized scheduler

- Replace one-unbounded-task-per-project startup with a bounded scheduler.
- Determine the default concurrency from measured CPU, memory, and disk
  behavior. Keep it an internal policy unless evidence establishes a genuine
  user need for configuration.
- Prioritize the active/open document's owning project. A `didOpen` arriving
  during startup may reprioritize queued work deterministically.
- Within the active project, index the open document, universal core semantics,
  and project-owned sources before exhaustively indexing external dependencies.
  Gate completeness-dependent diagnostics until the dependency graph is
  complete.
- A definition request for a not-yet-indexed locked dependency may promote that
  exact dependency ahead of ordinary background work. Demand priority must
  remain bounded, deterministic, and scoped to the owning project.
- Do not interrupt an engine write at an unsafe boundary. Cancellation occurs
  only at explicit coordinator checkpoints.
- Use deterministic tie-breaking: priority class, discovery order, then
  canonical project root.
- Dynamic workspace add/remove and runtime rebuild use the same scheduler and
  state machine as initial indexing.
- Query routing remains available independently for every ready engine.
- CPU-heavy indexing runs on bounded worker resources rather than monopolizing
  the async LSP reactor. Admission limits cover both task count and the measured
  memory pressure of large projects.
- Nested execution systems share an explicit process-wide resource policy.
  Tokio blocking tasks, Rayon lanes, extension guests, and JVM/decompiler work
  may not independently expand concurrency beyond the scheduler's measured CPU,
  memory, and I/O envelope.
- Duplicate watcher events are normalized, debounced/coalesced by owning
  project and semantic input identity, and resolved to one replacement
  generation that observes the final state.

### Remove redundant work safely

First prove duplication with counters, then eliminate it in this order:

1. Avoid repeated filesystem discovery and metadata reads for identical,
   checksum-verified inputs.
2. Reuse byte-identical source content and parse products.
3. Reuse project-neutral fact templates only if file IDs, source ownership,
   extension applicability, runtime compatibility, and provenance are rebound
   and validated before insertion into an engine.
4. Persist only content-addressed immutable products with a schema, parser,
   runtime, platform, extension, and policy fingerprint.

Cache keys must include every input that can alter meaning. Cache entries are
invalid on uncertainty; never use path or timestamp alone as semantic identity.
Do not share mutable `AnalysisEngine`, diagnostics, graph state, file IDs,
project extension state, runtime selection, or external-document ownership.

Exact Gemfile/lockfile closure and source identity remain mandatory. Performance
work must not scan unrelated global gems, promote `vendor/cache`, merge sibling
projects, or suppress unresolved-edge safeguards merely to report readiness
earlier.

### Invalid inheritance input

Superclass and mixin facts enter the same validated, file-owned engine lifecycle
as every other semantic fact:

- Reject a self-edge, cycle-closing edge, or conflicting superclass edge before
  same-pass inference or graph resolution can observe it.
- Attribute the rejection to the exact source fact and preserve deterministic
  behavior across filesystem order, parallel workers, and cache reuse.
- Distinguish invalid Ruby inheritance from analyzer artifacts caused by
  conditional compatibility branches, aliases, generated patches, or reopened
  declarations. Do not “fix” either case by guessing a superclass.
- Keep the inference assertion as a last-resort invariant. Production indexing
  must prevent user or dependency source from reaching that invariant.
- A rejected external fact may fail or degrade only its owning project according
  to the documented diagnostics/readiness policy; it must not panic a worker,
  poison a shared cache entry, or cancel unrelated projects.

### Artifact and derived-cache ownership

Ruby Fast LSP is not another Bundler, RubyGems, JDK, or Maven package manager:

- Reuse exact installed expanded gem sources, locked `.gem` archives, Bundler
  Git/path sources, JARs, JMODs, JDK `src.zip`, and JRuby runtime artifacts in
  place. Do not download or duplicate an artifact merely to cache it.
- Cache owned derived products: validated `.gem` extraction, source checksums,
  Ruby parse products, project-neutral fact templates, classfile metadata,
  generated Java signatures, source-member maps, and bounded decompiled
  implementation documents.
- Raw class/JAR declarations may be reused by checksum. Project-specific
  classpath ordering, duplicate-class selection, import aliases, overload
  resolution, provenance, and semantic insertion remain per engine.
- A whole runtime or classpath fingerprint may participate in a derived-product
  key only when that product's source semantics actually depend on it.
  Ordinary Ruby gem products must remain reusable across unrelated JRuby
  classpaths; Java-sensitive gem, signature, source-map, and implementation
  products must retain every exact artifact and ordering input that can change
  their meaning.
- Extension-generated facts may be reused only when the package fingerprint,
  activation context, framework/version applicability, input content, and
  semantic patch vocabulary are identical. Otherwise rerun the guest.

Concurrent projects requesting one cache identity must join one
**single-flight** producer instead of performing duplicate work:

- One in-process producer owns the read, parse, extract, or decompile operation;
  additional projects await its immutable result.
- Producer execution is owned independently of the first caller's future. A
  dropped, cancelled, or superseded initiating project cannot abandon the
  shared computation while another live waiter exists.
- Cancellation of one waiting project must not cancel a product still required
  by another project.
- Producer failure wakes every waiter with the same bounded error but does not
  permanently poison the key. A later generation may retry.
- Cross-process persistent writes use an ownership lock or equivalent atomic
  protocol, write to a temporary path, verify the checksum and manifest, then
  rename atomically.
- A crash, partial write, schema mismatch, checksum mismatch, unsafe archive
  path, or symlink escape invalidates only that entry and never becomes
  semantic input.

The cache lives only under Ruby Fast LSP's user-cache directory. It must have a
measured internal disk budget, automatic least-recently-used and orphan cleanup,
startup cleanup of incomplete entries, and a command to show size and clear
only Ruby Fast LSP-owned data. It must never delete or rewrite Bundler,
RubyGems, RVM, rbenv, asdf, JDK, Maven, Gradle, or project-owned files. Cache
policy remains server-owned and does not add routine VS Code settings.
Cache directories and files use private user permissions, and manifest-declared
sizes plus hard entry limits are checked before allocation or deserialization.
This goal persists only immutable external/runtime derived products; it does not
persist workspace-owned source, unsaved buffers, diagnostics, or a project's
mutable semantic engine.

Process-local immutable caches also require measured byte/entry bounds and
eviction. A completed single-flight value must not pin every gem, parse product,
JAR index, or decompiled document for the lifetime of a large umbrella
workspace. Eviction must never invalidate facts already rebound into an engine,
and in-flight values must remain alive until all current consumers finish.

## Bottom-Bar Product Contract

Use one authoritative Ruby Fast LSP status item on the right.

For the active Ruby/ERB document it shows the deepest owning project:

- `$(clock) admin: queued`
- `$(sync~spin) admin: project 3.2s / 5s`
- `$(sync~spin) admin: dependencies 42% · 8s / 15s`
- `$(warning) admin: slow indexing · 18s`
- `$(ruby) JRuby 9.2.21.0`
- `$(warning) admin: indexing failed`
- `$(ruby) No Ruby project`

Exact wording may be refined, but these semantics are required:

- The compact text describes only the active document's owning project.
- The tooltip shows the phase details and aggregate multi-project summary.
- While work is active, show elapsed time and the applicable readiness target.
  Show a percentage only when its denominator is known and monotonic.
- Crossing a readiness target changes the presentation to `slow indexing` and
  records the breached phase; it does not falsely mark valid work as failed or
  silently stop it.
- The runtime selector remains accessible from the item after readiness.
- Clicking a queued/indexing/failed state exposes a detailed project-status
  Quick Pick before offering runtime actions. It lists every discovered
  project, phase, elapsed/target time, runtime, reused/new work, and bounded
  failure summary; selecting a failure can open the relevant output details.
- Logs may show all project events, but they do not drive the item.
- A server notification carries a complete authoritative snapshot with
  generation and sequence. The editor performs an initial status request, then
  applies only newer snapshots.
- Active-editor changes render the newest cached snapshot immediately and
  request a refresh generation-safely.
- No delayed hide timer may conceal a later state. Disposal, restart, and
  workspace removal cancel pending editor work explicitly.
- A failed or cancelled generation never displays a checkmark or “Ready.”
- Do not alternate between a left global indexing item and a right runtime item
  for the same lifecycle.
- Intermediate counter-only snapshots are coalesced to a bounded publication
  rate. Phase transitions, active-project changes, completion, cancellation,
  and failures bypass that throttle and publish immediately.

## Milestones

### M0 — Reproducible baseline and race tests

- Add the multi-project profiler dimensions and preserve raw evidence.
- Add a minimal real-source-derived regression for cyclic/conflicting
  superclass facts and prove real source cannot reach the inference invariant.
- Build a deterministic fixture with at least three isolated projects, shared
  dependencies, different runtimes, one failure, and controlled phase delays.
- Add a VS Code test that reproduces interleaved percentages and the stale hide
  timer before the fix.

Exit: the current race and redundant-work baseline are demonstrated by failing
tests and recorded measurements.

### M1 — Typed project state and authoritative snapshots

- Replace `indexing_complete: AtomicBool` as the public truth with the explicit
  state machine.
- Add generation/sequence validation and structured request/notification
  transport.
- Make success, failure, cancellation, rebuild, add, and remove transitions
  exact and testable.

Exit: no task or editor event can publish stale state or report global success
when a project failed.

### M2 — Bounded active-project-first scheduling

- Route initial, added-project, and rebuild work through one scheduler.
- Add deterministic priority and safe cancellation checkpoints.
- Prove ready projects remain responsive while other work continues.

Exit: worker count is bounded and the active project wins the documented
priority without starvation.

### M3 — Shared immutable work products

- Use the baseline to identify the largest repeated reads/parses.
- Add validated process-local reuse first.
- Add one-producer/many-waiter single-flight coordination for identical work.
- Add persistent content-addressed reuse only after lifecycle correctness is
  proven.
- Add atomic cross-process publication, corruption recovery, owned-cache disk
  accounting, and automatic cleanup.
- Preserve per-engine fact replacement and provenance.

Exit: identical external inputs produce one measured computation, concurrent
requesters share it, restarts produce valid cache hits, disk use remains
bounded, cold and reused query results are semantically equivalent, a real
navigation probe succeeds from every reused product, and no semantic
cross-project leakage occurs.

### M4 — Incremental invalidation

- Reindex only the owning project for project file, Gemfile, lockfile, runtime,
  extension, classpath, and workspace changes.
- Invalidate only affected cache identities.
- Cancel superseded generations and delete stale facts deterministically.

Exit: changing one project does not rebuild or alter an unrelated sibling.

### M5 — Single deterministic VS Code status item

- Remove the competing raw progress presentation.
- Render authoritative active-project snapshots.
- Add fake-timer and delayed-response tests for switching editors, overlapping
  generations, failure, restart, add/remove, and disposal.

Exit: the status item cannot flicker backward, be hidden by an old timer, show
the wrong project, or claim false readiness.

### M6 — Real workspace and packaged acceptance

- Profile the full `goshposh` umbrella folder cold and warm.
- Exercise rapid active-editor switching during indexing.
- Prove project/runtime isolation and external-document provenance.
- Package and install the current-platform VSIX and repeat status/navigation
  acceptance without developer paths.

Exit: measured budgets pass from the installed artifact.

## Required Performance Budgets

M0 must record exact machine and dataset fingerprints before optimizing. The
final budgets are then checked into the profiler fixture and must satisfy at
least:

- Status ownership and the initial phase appear within **100 ms** after the
  server receives the active document.
- Active-buffer parsing and same-file Go to Definition are available within
  **500 ms** at p95.
- Project-owned Go to Definition is available within **5 seconds** from cold
  start and **1 second** with a valid warm cache.
- Required gem, runtime, stdlib, JRuby, and JAR navigation is available within
  **15 seconds** from cold start and **3 seconds** with a valid warm cache.
- The active project is semantically complete within **30 seconds** from cold
  start and **5 seconds** with a valid warm cache.
- The complete `/Users/naveenraj/goshposh` umbrella workspace reaches terminal
  state within **90 seconds** from cold start and **15 seconds** with a valid
  persistent cache on the recorded reference machine.
- Active project's semantically-complete time also improves by **50% or more**
  from the M0 umbrella baseline.
- All-project cold completion also improves by **35% or more** from baseline.
- Byte-identical shared external sources are parsed once per cache identity,
  not once per project.
- Concurrent demand for an identical gem, JAR, runtime, stub, or extension
  product performs one producer computation; every waiter receives the same
  verified immutable result.
- Persistent cache size stays within its recorded internal disk budget and
  automatic cleanup never touches externally owned artifacts.
- Peak RSS does not exceed the cold baseline by more than **10%** and should
  decrease when bounded concurrency replaces unbounded startup.
- A ready project's definition, hover, completion, and edit latency remain
  within their existing budgets while sibling indexing is active.
- Document Highlight on a ready project file stays within the interactive
  budget (same order as same-file navigation: **500 ms** p95) using
  same-document reference lookup only. It must not compute project-wide
  references and then discard cross-file hits, and it must not run multi-second
  synchronous work on the tower-lsp request-poll path where it stalls goto,
  hover, folding, tokens, and CodeLens.
- A one-project source edit performs no semantic replacement in sibling
  engines.
- Status snapshot application is monotonic and constant-time in the number of
  visible status items; there is only one.

The 500 ms, 5-second, and 15-second interactive-navigation limits are product
requirements and may not be replaced by relative improvements. If the
reference machine cannot meet a full-completion limit, retain the limit as the
9/10 target and record the remaining phase bottleneck rather than silently
lowering the bar.

## Test and Acceptance Matrix

- Scheduler unit tests with deterministic fake coordinators.
- State-machine invariant and stale-generation tests.
- Graph-ingestion tests for direct self-edges, multi-node cycles, conflicting
  superclass edges, alias/reopen compatibility branches, generated extension
  facts, and deterministic source attribution.
- Multi-root black-box LSP tests for success, partial failure, cancellation,
  dynamic folders, runtime changes, and active-project priority.
- Cache identity, corruption, version drift, invalidation, and provenance tests.
- Cold-versus-reused semantic equivalence tests for definitions, types, method
  lookup, signatures, graph edges, source precedence, and external-document
  provenance.
- Single-flight tests for concurrent waiters, producer failure, waiter
  cancellation, process contention, partial writes, recovery, and cleanup.
- Reactor-responsiveness tests while CPU workers are saturated, plus bounded
  process-cache retention and safe eviction tests.
- Watcher-storm tests proving duplicate/reordered events converge on one
  generation and the final filesystem state.
- Artifact-ownership tests proving cleanup touches only Ruby Fast LSP's cache.
- Engine isolation tests with conflicting constants, gem versions, JRuby/JDK
  inputs, and extensions.
- VS Code tests with fake timers and deliberately reordered responses/events.
- Single-project regression tests proving the scheduler adds no semantic or
  material latency regression.
- Real `/Users/naveenraj/goshposh` cold/warm/incremental profiling.
- Current-platform packaged VSIX smoke and manual active-editor status
  verification.

## Definition of 9/10

The rating may reach 9/10 only when:

- All milestones and recorded budgets pass.
- The server is the sole owner of project indexing lifecycle and aggregate
  scheduling.
- Bottom-bar state is active-project-correct, monotonic, generation-safe, and
  failure-aware.
- Concurrency is bounded and active-project-first.
- Reused immutable work cannot leak semantic ownership.
- Workspace, runtime, lockfile, and extension changes invalidate exactly the
  affected project and cache identities.
- Real umbrella-folder evidence and installed-VSIX evidence are recorded.
- The full local gate passes.

The remaining 1/10 may include adaptive scheduling across unusual storage
devices, distributed/shared team caches, perfect progress estimation for
previously unseen repositories, and every third-party filesystem watcher edge
case.

## Local Completion Gate

Hosted CI is not required. Before the final commit:

```bash
cargo fmt --all -- --check
cargo test
cargo test --workspace --exclude ruby-fast-lsp
cargo build --release
npm --prefix editors/vscode/vsix test
./editors/vscode/create_vsix.sh --current-platform-only
```

Also run the multi-root profiler on `/Users/naveenraj/goshposh` for cold,
warm-cache, one-project-change, runtime-change, partial-failure, and
active-project-priority scenarios. Record exact repository state, runtimes,
lockfiles, platform, cache state, concurrency, phase timings, file/byte counts,
cache results, CPU, peak RSS, query latency, and packaged VSIX checksum.

## Implementation Order

Completed foundation:

1. Reproduce the race and record the initial incomplete baseline.
2. Introduce typed state snapshots and staged readiness.
3. Route indexing through one bounded scheduler with active-project priority.
4. Remove the competing VS Code progress path and render one structured item.
5. Add process-local single-flight infrastructure and core-stub template reuse.
6. Make queued/active generation replacement cancellable, serialize
   same-project admission, pin coordinators to their launch engine, and cancel
   work on removal, runtime rebuild, and shutdown.
7. Make single-flight producers independent of initiating waiters; add
   cancellation, failure/retry, bounded-retention, and timing evidence.
8. Redesign the measured whole-closure gem product into exact per-gem immutable
   products; make binding transactional and deferred, and prove two-project
   isolated reuse with definitions, types, signatures, provenance, and
   replacement lifecycle.
9. Reject completed in-memory gem-product retention after controlled
   configured-JRuby evidence showed 112 MB retained for only 3.3 MB of
   second-project reuse and no material readiness gain. Keep the accepted
   ephemeral concurrent single-flight lifecycle and preserve the decision in
   `support/performance`.
10. Replace full-lane project-source serialization with cooperative exact lane
    partitions under the same global governor. A deterministic test proves two
    three-lane Rayon pools concurrently consume exactly the six-lane, two-task,
    fixture transient-memory, and two-I/O-slot budget; real `goshposh` evidence
    accepts the shape without claiming the remaining readiness targets.
11. Coalesce same-process classpath checksum/manifest work by stable canonical
    file identity. Keep only bounded descriptors, preserve project-owned
    ordering/catalog composition, expose process-wide reuse counters, and prove
    concurrent plus sequential reuse on the exact `goshposh` classpaths.
12. Make runtime stdlib discovery use only the exact selected executable, keep
    bundled stubs independently owned, and coalesce identical runtime path
    probes through a bounded server-owned single-flight product. Prove semantics
    are independent of the profiler executable's physical location.

Next work, in order:

1. Extend the accepted persistent derived-product protocol to the next measured
   immutable inputs after the completed per-artifact JAR/JMOD and compiled-Wasm
   products, the completed process-local classpath descriptor cache, and the
   completed bounded shared parsed-class metadata cache:
   runtime/stdlib signatures, exact source maps, extraction, and bounded
   decompilation. Key reusable JVM work by artifact
   checksum; retain project-specific classpath ordering, duplicate-class
   selection, imports, overload resolution, provenance, and engine insertion.
   Reuse package-manager artifacts in place. Do not reintroduce the rejected
   monolithic composed Java catalog on the active readiness path. Do not wrap
   every persistent Java artifact lookup in the rejected zero-retention flight:
   the current two-project schedule produced zero joins. Keep the accepted
   per-artifact Arc sharing bounded and target the remaining runtime/signature,
   source-map, extraction, and decompilation products without duplicating
   project semantic ownership.
2. Preserve the accepted exact request-driven navigation path and the passing
   **12.485-second** warm workspace completion while reducing active semantic
   completion from **11.011 seconds** to five seconds. The current
   512-file project checkpoints
   preserve mid-pass demand observation but add terminal overhead. The measured
   extension-dispatch prefilter is already accepted; shared or retained local
   method-return contexts and local type-subject indexing experiments regressed
   and were reverted. The retained method-return lifetime was rejected with
   both a hash table and compact sorted vector, so do not repeat it by changing
   only the container.
   Exact resolved-callee reuse is now accepted at a 64-entry per-source bound;
   do not grow it beyond the measured memory ceiling. The exact JRuby provider
   now reaches later project batches and leaves only 18 active-frontier files
   for replay. The measured YARD scan, single-owned name registry, and borrowed
   latest-known constant-type lookup, file-owned bucket splice, and borrowed
   known-method-return view are also accepted. Stable 128-bit semantic
   fingerprints traverse each field once while retaining the exact legacy
   bytes. The latest symbolized profile no longer contains the full type-fact
   expansion, shared symbol/type bucket sorts, or whole-file local-variable
   fallback as hot paths; file fact collection remains dominant at **39.30%**,
   followed by call handling and final engine resolution. Exact owner/name fact expansion has been
   removed, resolution borrows its cached chain rather than cloning the
   vector, and that cache now stores interned owner IDs. A direct ID-domain MRO
   traversal was measured, exposed an edge-only namespace trap, and was
   rejected after its corrected form still regressed production timing; do not
   repeat that shape without a new profile and data structure. The first
   fact-pass context-reuse slice is accepted: local receiver inference now
   starts from the collector's active lexical scope. The second slice removes
   the semantically invalid whole-file text fallback and establishes corrected,
   deterministic semantic-result fingerprints. Per-file lazy extension
   applicability is also accepted and removes the repeated semver path without
   retaining registry ownership. The project dependency applicability identity
   is now computed once per exact lock snapshot rather than once per file.
   Gem cache-key preparation now uses one combined Prism traversal rather than
   three independent parses. Broad borrowed file-type facts were measured and
   rejected; do not repeat that shape.
   Schema-14 `ResolvePassStats` instrumentation is accepted and records final
   resolve cache hit/miss cardinalities plus coarse subphase timings; keep it
   for future profiles. A cache-hit fast path inside
   `resolve_reference_candidates` was measured and rejected: resolve-phase
   median improved about 6.6%, but warm semantic completion, dependency
   navigation, project navigation, and wall all regressed. Evidence is in
   `support/performance/resolve-pass-cache-cardinality-2026-08-01.json` and
   `support/performance/resolve-cache-hit-fast-path-rejection-2026-08-01.json`.
   Do not retry hit-path micro-optimizations there without a new profile showing
   resolve dominates the remaining semantic gap; prefer dependency rebinding /
   phase overlap / fact-collection targets next.
   Select the next distinct target from the recorded v282 profile. The v283
   profile belongs to the reverted lazy-context experiment and is useful only
   as rejection evidence. Do not repeat the regressing shared local
   type-subject shape, retained per-file return context, discarded-snapshot
   special mode, or the rejected combined extension call-classification shapes.
   Do not retry lazy per-file extension-context materialization without a
   narrower call-classification or compact-context design proven independently.
   Extending the active five-lane reservation through the exhaustive project
   tail was also measured and rejected: it regressed terminal wall time by
   **8.8%**, active semantic completion by **5.1%**, and breached the fixed RSS
   ceiling in one run. Evidence is in
   `support/performance/active-project-reservation-tail-rejection-2026-08-01.json`.
   Dependency rebinding remains open without broad path heuristics,
   mutable project caches, unbounded checkpoints, or weaker absence semantics.
3. Add exact invalidation/rejection evidence with every new persistent product,
   then prove the completed watcher generation gate emits only the final
   filesystem state in the packaged client.
4. Run cold, warm-process, fresh-process persistent-cache,
   one-project-change, runtime-change, failure, and active-priority acceptance
   on `goshposh`; require the completed per-project semantic-result fingerprint
   to remain equal across equivalent runs; pass the full local gate; reuse the
   installed checksum-verified VSIX and repeat real-workspace navigation/status
   acceptance without developer paths.

Already completed and not to be repeated unless a regression appears:

- The reproduced `StringScanner` alias/self-superclass crash is fixed at the
  graph-fact write boundary with focused regressions.
- The schema-2 67-project M0 aggregate, exact build/dataset fingerprints,
  staged readiness timestamps, CPU, peak RSS, engine/source bytes, and
  process-local core-stub single-flight evidence are recorded.
- The focused `server` profile campaign established that semantic preparation,
  graph lifecycle, and contextual lookup—not raw Prism parsing—were the
  dominant costs. Known-namespace snapshots, stable extension applicability,
  graph ownership indexing, and exact resolved-callee return queries already
  removed most of that repeated work. Do not restart with a speculative generic
  parse cache.

Every slice follows red-green-refactor. Do not begin by merging engines,
silencing diagnostics, indexing fewer locked dependencies without a completeness
contract, or adding more user settings.
