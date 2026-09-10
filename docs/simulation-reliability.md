# Simulation reliability acceptance

This is the acceptance record for improving the simulator's ability to detect
correctness, lifecycle, isolation, and recovery defects. The proposed 9/10
rating is qualitative; no percentage of future defects detected is claimed.
Every completed criterion requires evidence, not only a passing test count.

## Acceptance criteria

| Requirement | Current evidence | Outstanding work |
| --- | --- | --- |
| Read-only observations | Seven publication/observation/isolation controls pass. Both `FakeEditor` and inline diagnostic tags read actual publication output. Missing publication differs from an empty clear. | Final workspace acceptance and side-effect controls for other observations. |
| Complete expected results | Fresh comparisons retain full hover responses and definition links; location arrays retain duplicates. Exact method navigation/reference and rename lifecycle contracts pass, alongside seven corrupt-result comparator controls. | Generated graph oracle checks still use subset and line-only assertions in several paths; replace them with independently generated exact spans. |
| Independent semantics | Separate project-model oracle plus fresh-analysis comparisons; retained lexical-scope regression. Six shared reviewed dispatch contracts pass in the model and actual MRI 3.3.11 execution. | Broaden language forms and exact generated output expectations. A single executed path is not proof of static-inference completeness. |
| Production lifecycle orderings | Five direct source-snapshot schedules, nine coordinator controls, and seven dependency-refresh schedules: edits, root addition/removal, project replacement, close, and a newly opened project target. Separate tests preserve syntax and linter output; a cold-coordinator fixture-runtime test covers stdlib/default-gem refresh. | Server-process restart, installed-runtime acceptance, and overlapping isolated project passes through actual lifecycle boundaries. |
| Feature/lifecycle matrix | Existing model checks navigation, references, hover, hints, and selected diagnostics. | Record scenarios and missing combinations for every advertised core feature; add exact positive, negative, edit, and recovery cases. |
| Fault detection | All eight reviewed production mutations fail their intended semantic assertions in an isolated checkout; 39 driver controls pass. The campaign exposed and helped repair a scheduling test that let one mutation escape. Source/build identities, logs, patches, and the original failed run are retained. | Extend the inventory as additional exact feature and lifecycle contracts are added; this is not exhaustive defect coverage. |
| Replay and reduction | Seeds retain generated source, script, generator version, replay instructions, scoped source manifest, compiler/build metadata, and test-executable SHA-256. Six identity controls pass. Controlled schedules retain their order. | Structured expected/actual observations, identity-enforced replay, bounded trace reduction, and demonstrated same-failure replay. |
| Scale and bounded longer runs | Previous source passed explicit synthetic scale and sampled real-corpus scenarios. | Rerun final source; add bounded long edit/restart runs with progress and resource evidence under unchanged budgets. |
| CI and authoring guidance | Local/CI release runner executes deterministic and explicit scale gates and retains logs. | Integrate fault/coverage/replay acceptance and document adding a new red scenario and feature observer. |

## Evidence rules

- Observe production output without triggering a compensating write or solve.
- Treat an absent diagnostic publication separately from an explicit empty clear.
- Keep expected behavior independent of actual results. Never filter away wrong
  targets or relax a semantic expectation to make the suite pass.
- Fresh/incremental agreement proves consistency only for exercised observations;
  the same semantic error can occur in both engines.
- A fault run counts only when the intended assertion detects the injected
  behavior. Build errors, setup failures, unrelated panics, and timeouts do not
  count as successful detection.
- Record skipped, unsupported, and unexercised cases separately. Large generated
  projects do not imply broad Ruby semantics coverage.
- Preserve actual wire/editor, native-platform, and external beta validation as
  separate evidence. A publication recorded by the in-process test server is
  not proof that an editor received the notification over LSP transport.

## Observation controls

`test::simulation::observations` contains seven controls:

1. Publish a deliberately retained diagnostic over valid Ruby, observe that exact
   output, then publish an empty clear and observe the clear.
2. Remove a file's semantic facts, observe diagnostics, and require the semantic
   state fingerprint to remain unchanged. The fingerprint checks observation
   purity here; it does not replace semantic result assertions.
3. Require a missing publication to remain distinguishable from an empty clear.
4. Observe a deliberately published warning through inline tags.
5. Require inline diagnostic tags to leave deliberately removed facts untouched.
6. Open a late constant definition and require the consumer's published unresolved
   error to clear while its existing syntax warning remains.
7. Open a definition in another project and require the first project's facts and
   publication to remain unchanged.

The first two failed on the old helper for their intended reasons and passed after its
fact-collection/replacement path was removed. Initial local evidence:
`/tmp/ruby-fast-lsp-sim-observation-red.log` and
`/tmp/ruby-fast-lsp-sim-observation-green.log`. These are development checkpoints;
the final acceptance report must retain durable evidence for the final source.

## Defects exposed by observation changes

These are concrete discoveries, not a claim about future defect-detection rates.

| Finding | Regression evidence | Repair |
| --- | --- | --- |
| Opening a late definition repaired engine facts but left stale published errors. | `late_definition_open_clears_published_unresolved_constant` failed on the actual publication. | Republish same-engine open consumers after the existing dependency-open refresh. |
| Opening a file could reprocess open documents in another isolated project. | `opening_another_project_cannot_rebuild_or_publish_this_projects_state` detected a changed semantic fingerprint. | Restrict the refresh to the opened document's owning engine. |
| The new semantic refresh could erase current external linter messages. | `dependency_open_retains_current_linter_output_without_rerunning_it` failed before retention. | Retain presentation output under its exact source snapshot; invalidate on edit/close. |
| Nil-call warnings existed only after the test observer rebuilt a document's scopes. | Two existing nil-call tests and two added lifecycle/cold-index controls failed. | Collect candidates in the ordinary analysis pass and resolve through engine-owned receiver proof. Eight focused tests and the subsequent workspace checkpoint pass. Final performance acceptance remains pending. |
| A regular-expression constant declared below a call lacked its type in the first declaration pass. | Existing typed-constant publication test and new exact declaration-seed test failed. | Seed regular-expression literal types before body collection. Declaration-seed and publication regressions pass in the workspace checkpoint. |
| Delayed cold diagnostics restored an error after an edit, a dependency open, close/reopen, or indexing supersession. | All four `delayed_coordinator_diagnostics` cases failed on the actual submitted array before the repair. | Recheck lifecycle ownership after acquiring the document lock; project current facts and enqueue synchronously under the engine read lock. |
| Cold publication removed valid syntax warnings and external linter output. | Both `cold_coordinator_diagnostics_preserve_current_*` tests failed with missing diagnostics. | Merge current syntax and semantic diagnostics with exact-source linter output; the linter invocation count stays unchanged. |
| Delayed require refresh restored corrected errors, cleared newer missing requires, published after close or into a replacement project, and missed a newly opened project target. | All seven `test::simulation::dependency_refresh` schedules failed on exact submitted diagnostics before the repair. Each also checks engine facts before the delayed task resumes. | Conditional source-snapshot replacement, retained dependency-index and workspace guards, current-engine require resolution, and open-document-only publication. |
| Dependency refresh removed valid syntax and external linter diagnostics. | `dependency_refresh_preserves_current_syntax_and_linter_output` failed on the missing output. | Rebuild the complete current diagnostic projection while reusing exact-source linter output; its invocation count remains one. |

Several request helpers also converted JSON-RPC errors into empty results that
could satisfy negative assertions. They now fail explicitly, with a
control separating failed requests, absent results, and explicit empty arrays.
This control validates the helper boundary; it does not prove actual transport
delivery or cancellation behavior.

## Adding a red scenario

1. Reduce the problem to neutral Ruby source. State the full expected result:
   every target URI and range, or every edit and replacement text. Include an
   unrelated symbol with the same name when identity matters.
2. For an editor lifecycle bug, use `FakeEditor` to send open/edit/save/close
   operations. Observe queries or submitted diagnostic publications directly.
   Never rebuild facts or resolve the engine inside an assertion helper.
3. Run the smallest test before changing production code. Retain the specific
   assertion failure; a build error, setup panic, or timeout is not useful red
   evidence for the behavior under test.
4. Fix the owning production layer. Run the focused test, then related coverage.
   Restore the original buffer and repeat the observation to check recovery.
5. Add the semantic form to the generated model, renderer/source map, independent
   oracle, observer, and edit generator where applicable. A handwritten lifecycle
   test alone does not make that feature part of the generated simulator.
6. Record unsupported combinations in the matrix. Keep fresh-analysis agreement
   as an additional consistency check, with an independent expected result.

`src/test/simulation/exact.rs` is the small-contract example. It checks complete
method navigation/reference results and rename edits, applies the edit, removes
the calls, restores them, and closes/reopens the consumer. Its comparator controls
inject missing, extra, duplicate, wrong-URI, and wrong-range observations. These
are assertion controls; production fault-injection evidence remains outstanding.

## Feature and lifecycle matrix

This table describes simulator coverage, not whether a product feature exists.
The advertised capabilities are in `src/handlers/notification.rs`; call and type
hierarchy are registered dynamically. Integration tests outside the simulator
remain separate evidence. "Partial" means the graph model asserts selected
targets or labels rather than the entire independently expected response.

| Feature | Generated model oracle | Incremental vs fresh sequences | Independent exact lifecycle contract | Important remaining combinations |
| --- | --- | --- | --- | --- |
| Definition/navigation | Partial; methods, constants, namespace edges | Complete response, including link ranges | Method targets before/after rename, call removal/restoration, close/reopen | Exact generated spans; require/ERB/JRuby/dependency navigation under coordinator races |
| References | Partial; selected supported calls and exclusions | Complete location arrays | Complete method call set, two calls on one line, unrelated same-name owner, edit/reopen | Declaration inclusion flag; exact generated sets; reopened/alias/visibility combinations |
| Hover | Partial; namespace/method/type labels | Full response including ranges/markup | No dedicated complete independent contract yet | Negative, signature changes, dependencies and restart |
| Completion | None in ordinary generated sequences | Not yet observed | Existing integration tests only | Full items, replacement edits, resolve responses, stale filtering, edit/recovery |
| Signature help | None in ordinary generated sequences | Not yet observed | Existing integration tests only | Full overloads, active parameters, edit/recovery in generated sequences |
| Diagnostics | Selected valid/invalid model sites | Actual submitted publication arrays | Publication/empty-clear/purity/isolation controls; nil-call lifecycle; delayed coordinator edit/dependency/reopen/supersession; syntax/linter preservation | All codes and severities; versioned wire delivery; other delayed publishers and server-process restart |
| Rename | Not generated | Not yet observed | Full method workspace edit, applied text, unrelated owner, removal/restoration | Constants, locals, rejected edits, UTF-16, cross-project/external protection in simulation |
| Document symbols | Index-shape assertions are separate | Controlled schedule comparisons | No complete independent symbol response yet | Edit/recovery, nested scopes, malformed buffers, restart |
| Workspace symbols | None | None | Existing integration coverage is separate | Query aggregation and project removal/isolation |
| Inlay hints | Selected model types and labels | Complete arrays for open files | No complete independent hint array yet | Negative hints, metadata/ranges, edit/recovery for all supported types |
| Code actions | None | None | External-linter integration tests are separate | Full safe edits, stale diagnostics, failed tools, recovery |
| Implementation; call/type hierarchy | None in generated observer loop | None | Existing integration coverage is separate | Positive/negative and edit/recovery simulation contracts |
| Highlights; selection/folding ranges; semantic tokens | None in generated observer loop | None | Existing integration coverage is separate | Full ranges/data, malformed/Unicode buffers and edit/recovery |
| Formatting; on-type formatting; code lenses | None in generated observer loop | None | Existing integration coverage is separate | Exact edits/lenses, tools, ERB safeguards and recovery |

The new coordinator tests pause real project collection immediately before the
normal snapshot-guarded commit, then deliver an unsaved open or edit through the
normal document handler. They are distinct from the older five schedules that
call the snapshot guard directly. A cancellation control also proves that a new
indexing generation completes after collected work is cancelled, and dropping
a gate controller releases its worker. This is not a server-process restart.
Four further schedules pause the actual cold diagnostic path before it takes
the document lock, complete an edit, dependency open, close/reopen, or indexing
supersession, then observe immediately after the publication attempt. The old
path retained a stale array across this boundary; the repaired path projects
current state under the document and engine guards. Separate cold tests retain
syntax warnings and linter output without rerunning the linter.

Seven dependency-refresh schedules pause the real refresh after it snapshots
consumer sources, finish the newer edit/root/project/close/target-open operation,
and pause again immediately after the stale commit attempt. They compare the
exact submitted diagnostic array and engine facts before releasing the task.
Root-only and project-target changes explicitly retain the consumer source
snapshot so source validation alone cannot satisfy those cases. The close case
requires facts to refresh while the previous closed-document publication stays
unchanged. A separate test preserves syntax and linter output. All eight failed
on intended semantic assertions before the fix; the focused command is
`cargo test --locked --lib dependency_refresh -- --nocapture`.

The verification record is
[`dependency-refresh-races-2026-09-10.json`](../support/performance/dependency-refresh-races-2026-09-10.json):
eight new red-to-green regressions, 2,541 workspace tests passed with zero failures,
the three existing explicit scale/corpus deferrals, and six release profiler runs
under unchanged absolute budgets. This is in-process evidence; the new build
has not been installed into VS Code.

The existing `cold_runtime_require_roots_refresh_open_diagnostics_and_preserve_project_precedence`
test also passes in the workspace run. It exercises the full coordinator with
one exact fixture runtime probe, stdlib/default-gem targets, a retained true miss,
and project-local precedence without a consumer edit. Server-process restart,
installed-runtime acceptance, and a broader observation set remain required
before the production-scheduling criterion is complete.

Latest completed workspace checkpoint before the coordinator-gate additions:
**2,503 passed, zero failed, six ignored**. Its local evidence is
`/tmp/ruby-fast-lsp-sim-workspace-observers-exact.log`. The 72-test ordinary
simulator checkpoint had three explicitly ignored scale/corpus tests. The later
combined simulation checkpoint passed **82 tests, zero failures, three explicit
scale/corpus deferrals**, including all four coordinator and six identity
controls (`/tmp/ruby-fast-lsp-sim-identity-coordinator.log`). These counts are
neither feature-coverage nor code-coverage percentages.

After the frozen-array propagation and background diagnostic repairs, the
2026-09-10 workspace checkpoint passed **2,533 tests, zero failures, three
existing scale/corpus deferrals**. All six focused coordinator diagnostic
regressions passed after first failing on the intended assertions. Logs are
retained under `target/background-diagnostics-regression/`; the earlier
checkpoints above remain historical evidence.

## Test selection and ignored checks

Self-contained inference acceptance and reviewed precision fixtures now run in
ordinary `cargo test`. They also run individually in the correctness gate to
retain their complete JSON reports. Their default-suite validation passed five
matching report/control tests in 2.72 seconds with no ignores
(`/tmp/ruby-fast-lsp-default-acceptance.log`). The public query documentation
example is compile-checked and passes rather than being ignored.

Only three tests remain deliberately outside the ordinary run:

| Deferred test | Required execution |
| --- | --- |
| `generated_project_large_scale_smoke` | Explicit release-profile `large-scale-lsp` gate. |
| `generated_project_large_scale_engine_checks_all_edges` | Explicit release-profile `large-scale-engine` gate. |
| `generated_project_real_corpus_smoke` | Explicit read-only `SIM_REAL_CORPUS_ROOT`; absence is reported as `not_run`, never passed. |

The shared local/CI release validator rejects every other ignored test, changed
ignore reason, duplicate ignored name, or ignored summary without named output.
It records each allowed deferral with the gate that executes it. An exact gate
still requires exactly one passing test and zero ignored tests. Sixteen validator
controls pass; the three new controls first failed on the old validator for
unexpected ignores, missing names, and absent deferral evidence.

Do not quarantine a broken regression by adding `#[ignore]`. Keep its assertion
and fix the owning behavior. Do not make an external-input test silently return
success when its input is missing.

The subsequent complete workspace run passed **2,516 tests, zero failures, three
documented deferrals**. The strict release validator accepted the actual complete
log. Durable local evidence is under
`target/simulation-reliability/test-selection/`: `checkpoint.json`, `build.json`,
and the red/green validator, acceptance, doctest, and workspace logs. The retained
build identity was matched to the completed workspace test executable and every
declared source-file hash was rechecked. This is a correctness checkpoint, not
final scale, performance, or external validation.

## Neutral Ruby oracle controls

`support/simulation/oracle_cases.json` pairs six declarative model inputs with
separately handwritten Ruby programs and reviewed expected targets. The ordinary
Rust test `independent_oracle_matches_reviewed_neutral_ruby_dispatch_contracts`
checks the actual simulator oracle against those targets. Run the execution side
with `python3 support/simulation/run_oracle_controls.py`; use `SIM_RUBY` to select
an executable explicitly. Missing Ruby, a child failure or timeout, missing cases,
and a wrong target fail this required gate rather than silently skipping it.

The selected rules are instance inheritance, prepend before the class, include
before the superclass, last include winning, class-method inheritance, and
rejection of an explicit private call. Each positive Ruby program also executes
the bound method and verifies its marker, so its owner observation is tied to
actual dispatch. All six passed on MRI 3.3.11; the Rust oracle contract passed
against the same fixture. Exact expected/actual values, fixture and runner
hashes, runtime identity, commands, and output are retained in
`target/simulation-reliability/oracle-controls-*`. The fixture participates in
the compiled simulator source manifest. The local/CI simulation release gate
requires both sides; the newly configured remote CI job has not been run here.

These examples check selected dispatch rules. They do not prove coverage of all
Ruby implementations, dynamic loading, arbitrary metaprogramming, or static type
inference across all reachable paths.

## Fault campaign and the escaped scheduling defect

Inspect the concrete inventory with
`python3 support/simulation/run_fault_campaign.py --inventory`. Reserve exclusive
use of the repository's Cargo target, then run
`python3 support/simulation/run_fault_campaign.py --run`. The driver creates its
own detached worktree, overlays the current scoped source, runs a green baseline,
and applies each mutation separately. Production sources in the invoking
checkout are never mutated. The CI workflow runs this as a separate required
job and retains its evidence even on failure.

The eight mutations drop or duplicate definition results, corrupt a reference
range, omit or add an unrelated rename edit, drop a nil-call warning, omit a
consumer's diagnostic publication, and bypass both cold-source commit guards.
Each names the exact regression and semantic assertion that must detect it.
Compilation/setup errors, zero tests, timeouts, and unrelated assertions never
count as detection. Thirty-nine driver controls pass, including real Cargo
failure-output classification and scoped checkout/link/mode handling.

The first executed inventory exposed a useful weakness in the tests: bypassing
the cold-source guards passed the startup-edit regression. Its two producers
were released together, allowing interactive work or later coordinator work to
repair stale facts before observation. The test now finishes the edit while
cold work remains paused, then observes exact navigation immediately after the
cold commit attempt while the coordinator tail is still paused. Four baseline
coordinator controls pass under this stricter ordering. The campaign rerun proves
that the formerly escaped mutation now fails its intended assertion.

The original report is retained unchanged under
`target/simulation-reliability/fault-campaign-20260910T081529Z-loqtppb6`.
Its seven other semantic failures were initially classified as invalid because
the driver mistook Cargo's normal failing-test footer for a compile error. A
control made from the real output reproduced that classifier bug; the narrow
footer fix passes while compile-error controls remain invalid. The original
failed report is not rewritten as a passing run.

The corrected campaign completed with **eight detected, zero escaped, zero
invalid**. Its ordinary baseline passed **83 tests**, with only the three
documented scale/corpus deferrals, and every selected regression also passed
individually before mutation. The source manifest was rechecked against the
current checkout. See the portable summary in
[`support/simulation/fault_detection_report.json`](../support/simulation/fault_detection_report.json)
and the full local evidence under
`target/simulation-reliability/fault-campaign-20260910T082416Z-f_dddpls`.
The summary includes each exact assertion, mutation/log hash, mutant source and
binary identity, baseline identity, and replay command. The owned worktree was
removed successfully. This is evidence for the declared inventory only; it is
not a measured future-bug detection rate or completion of the full reliability
goal.

The final workspace checkpoint after the scheduling repair and neutral oracle
addition passed **2,517 tests, zero failures, three documented deferrals**. The
shared release validator accepted its actual output; the log and validation
record are retained under
`target/simulation-reliability/fault-oracle-workspace`. No remote CI run or final
performance/scale acceptance is implied by this checkpoint.
