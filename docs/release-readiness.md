# Broader public release readiness

This is the release acceptance checklist, separate from the feature roadmap in
[`NEXT.md`](../NEXT.md). A checked box requires evidence for the release
candidate's exact source and packaged artifacts. Historical measurements and
successful builds do not establish current runtime behavior. Publication and
announcements require explicit authorization.

## Candidate and evidence

- Baseline: `3c33d9e` (2026-09-09 assessment).
- Candidate: work in progress; no version or tag reserved yet.
- Initial local checks: root tests 1,650 passed / four failed / two ignored;
  ruby-analysis 640 passed; editor tests 68 passed; inference acceptance
  118/121 with three safety failures and `claim_eligible: false`; reviewed
  real-project precision passed.
- Default simulation: 56 test functions reported success; the two large-scale
  tests and real-corpus test returned early because their opt-in flags were
  absent. They are **not** validated by that result.

### Current audit checkpoint (2026-09-10 IST)

The current correctness run passed 2,485 Rust workspace tests (six explicitly
ignored), 68 editor tests, 26 package/gate tests, the unchanged 121/121 inference
acceptance contract, and reviewed precision. The release simulation gate also
completed successfully, including explicit scale and real-corpus execution.
Remaining editor workflows, comparative performance, and native platform
validation are still required before the candidate can be accepted.

| Area | Evidence obtained | Remaining acceptance |
| --- | --- | --- |
| Deterministic simulation | 63 release tests passed. The three default-ignored scale/corpus tests each passed explicit execution. A separate final-source run with `SIM_RANDOM_SEEDS=10` passed fixed/regression seeds and ten generated seeds in each seeded test. | Five controlled background schedules cover selected orderings, not exhaustive scheduler exploration or a long-running soak. |
| Synthetic scale | Both explicit 2,284-file / 23,328-method scenarios passed. All-edge checking observed 23,718 graph/model edges. | Passed for the generated model; this does not certify all Ruby semantics. |
| Medium-project performance | Absolute budgets passed: cold indexing 397 ms, edit p95 1.94 ms, references p95 5.33 ms, estimated heap 5.5 MB. | Quiet alternating comparison and current two-project memory acceptance. Absolute budgets alone do not establish regression acceptance or superiority to another server. |
| Package assembly | Rebuilt npm pack/install and assembled VSIX smoke passed on macOS ARM64. An earlier build's five bundled guests passed 12 stress iterations across six projects. | Repeat stress on the final frozen artifact; other native targets remain pending. |
| Real VS Code | Ten checks passed on the rebuilt package in VS Code 1.133.0: activation, Ruby/require navigation, hover, references, unsaved edits/restoration, ERB Ruby navigation and HTML completion. VSIX SHA256: `bc34ded6101c6b561144a7c514a4e6e4e2b2695b189ef41950f3c7388b17941c`. | Restart, runtime UI, formatting/linting, test execution, and folder lifecycle remain separate checks. |
| Installed runtimes | Exact MRI 3.3.11 and JRuby 9.2.21.0 with JDK 17.0.18 resolved `json`, `uri`, and project methods without fixture diagnostics. | Final build rerun and missing-runtime navigation. Wider version combinations remain external beta work. |
| Real corpus | Harness ownership regression and explicit scenario passed. Indexed 2,487 project sources / 2,620 total files and 30,213 methods. All 12 selected navigation/reference/hover samples and three type-hint files passed. | Samples do not certify every query or diagnostic across this corpus. |

Durable local logs and JSON results are under `target/release-evidence/`.
The shared runner is `node editors/scripts/release_checks.js correctness` or
`simulation`; it rejects zero-test successes. Real editor acceptance is
`node editors/scripts/smoke_vscode.js <candidate.vsix> <code-executable>`.

## Confirmed defects to close

- [ ] Final acceptance of monotonic, generation-owned progress reporting;
  reverse delivery and delayed discovery after replacement now have regressions.
- [ ] Final acceptance of standalone startup with active constant navigation:
  it must skip locked-gem discovery when there is no Gemfile. Reduced red/green
  regression and actual-runtime reproduction are retained.
- [ ] Final acceptance of bundled core navigation: register real source metadata
  before parallel template collection, including non-ASCII position coverage.
- [ ] Final acceptance of unavailable Auto runtime selection: use conservative
  core fallback while preserving an explicit compatibility override.

- [ ] Restore exhaustive control-flow receiver evidence for ordinary/pattern
  case joins and short-circuit assignments; restore rescue union navigation.
- [ ] Prevent invalidated mutable shapes from regaining obsolete field proof.
- [ ] Require correctness, editor, acceptance, and package checks before publish.
- [ ] Fail publishing on actual registry errors; never label every failure as
  an already-published version. Do not announce a successful release after
  partial publication.
- [ ] Unify local and CI package assembly, including core RBS navigation files,
  JRuby assets/licenses, extension versions, and actual packaged guest loading.
- [ ] Verify the post-0.3.0 bundled-extension compatibility correction in the
  assembled artifacts, not only source manifests.

## Required local validation

- [x] Full Rust workspace and editor tests pass with existing expectations.
- [x] Explicit inference acceptance: 121/121, baseline matches, claim eligible.
- [x] Explicit reviewed real-project precision passes.
- [x] Fixed and retained regression simulation seeds pass in release mode.
- [x] Generated semantic edits compare incremental results to fresh analysis
  and to independent expected behavior at supported sites.
- [x] Controlled background-work orderings prove stale commits are rejected;
  lifecycle, project isolation, cancellation, and recovery remain coherent.
- [x] Failure evidence includes seed, generator version, source, operations,
  ordering where applicable, and replay instructions. Retain reduced failures.
- [x] Explicit large-scale sampled-LSP and all-edge engine scenarios execute.
- [x] Additional seeds and bounded repeated editing run with coverage and
  exercised/skipped scenarios reported separately.
- [ ] Static require/default-gem/stdlib cold-index refresh regressions pass.
- [ ] Clean npm installation and extracted VSIX exercise the packaged binary,
  bundled extensions, runtime navigation, and editor assets on this host.
- [ ] All bundled extensions pass required load stress (missing artifacts fail).
- [ ] Final release-profile latency/CPU/memory measurements satisfy existing
  budgets; record machine, source identity, artifacts, and corpus identity.
- [ ] Supported local runtime and editor workflows are validated; missing
  runtime and unsupported configurations have explicit, tested behavior.

## Public beta and external validation

- [ ] Publishable compatibility table distinguishes tested, build-only, and
  unsupported OS/architecture/runtime/editor combinations.
- [ ] Native macOS Intel, Linux x64 GNU, and Windows x64 installed-artifact
  results recorded; local cross-compilation is not a substitute.
- [ ] Real editor acceptance covers initial indexing, unsaved edits, restart,
  dependency changes, multi-project routing, ERB, formatting, and test lenses.
- [ ] Installation, troubleshooting, competing-extension guidance, limitations,
  issue-report instructions, release notes, and beta feedback plan are ready.
- [ ] External beta feedback covers varied plain Ruby, Rails, and supported
  JRuby projects; crashes, incorrect edits, persistent false diagnostics, and
  installation failures are triaged before broader promotion.
- [ ] Verify the exact published artifacts after separately authorized release.

## Intentional boundaries

Full runtime metaprogramming, arbitrary string evaluation, unrestricted dynamic
dispatch, and native APIs without declarations remain unsupported proof
boundaries. Advanced Rails association/route forms, additional RBS algebra,
broader yield inference, extra CPU architectures, and whole-template formatting
are feature decisions, not automatic release blockers. Document supported
behavior and fail conservatively. Do not inflate coverage or weaken safety
expectations to present these boundaries as completed functionality.

## Beta acceptance plan

1. Freeze a candidate only after all locally executable required gates pass.
2. Distribute it only after authorization, with exact artifact/version identity,
   support table, known limitations, and rollback instructions.
3. Collect minimal reproductions plus OS, architecture, Ruby/Bundler/editor
   versions, dependency setup, logs, and the triggering lifecycle sequence.
4. Turn every confirmed blocker into a neutral regression; rerun the affected
   gates and final candidate acceptance after fixes.
5. Promote only after supported-platform checks and blocker triage are complete.
   Missing external evidence remains explicitly pending.
