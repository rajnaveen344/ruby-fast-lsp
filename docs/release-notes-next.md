# Unreleased public beta candidate

These are draft notes for the next candidate. Its version, tag, and release date
are unassigned. Publication and announcement have not been authorized. The
server uses SemVer and the editor extension uses CalVer; the exact server-to-
extension version mapping will be recorded at release.

## Changes

- Corrected inference and navigation for ordinary `case`, pattern `case`,
  short-circuit assignments, and rescue flow. Reachable alternatives remain
  represented, and Hash mutation or invalidation preserves uncertainty instead
  of restoring an obsolete concrete type.
- Indexing progress counters remain monotonic when files finish concurrently.
  Delayed reports from an earlier indexing run are rejected after restart so
  they cannot overwrite the current run's status.
- Plain Ruby folders without a Gemfile skip automatic gem discovery even when
  an active document contains constants. Bundled core declarations retain their
  source positions for navigation, including when the requested runtime is
  unavailable. Automatic runtime fallback preserves explicit compatibility
  choices.
- npm and VSIX packaging share validation of required assets, licenses, and
  checksums. Core runtime constants retain packaged navigation sources, and
  JRuby decompiler assets include the required license. VSIX assembly validates
  all five bundled framework extensions: Rails, RSpec, Minitest, Sinatra, and
  Cucumber.
- Publication now depends on correctness, inference acceptance, editor, and
  package checks, plus deterministic simulation and explicit large-scale
  simulation runs. Simulation covers semantic edit sequences, comparison with
  fresh analysis, and controlled background-work orderings. Skipped scenarios
  are reported separately, and discovered failures retain replay evidence.

These changes address specific regressions and release safeguards. Ruby's
dynamic behavior and the documented inference, RBS, Rails, and ERB boundaries
still apply; this candidate does not claim complete Ruby analysis.

## Compatibility and validation still pending

Use the [public beta guide](public-beta.md) for installation, rollback,
compatibility boundaries, acceptance scenarios, and feedback instructions.
The [release-readiness checklist](release-readiness.md) records evidence for the
exact candidate and its packaged artifacts.

Native installed-artifact acceptance on macOS Intel, Linux x64 GNU, and Windows
x64 remains pending. Real editor acceptance and wider framework/runtime beta
feedback also remain pending. Local automated results do not establish success
on those platforms or in a real editor, and historical performance results do
not certify this candidate. Final validation results must be recorded before
the candidate is presented as ready for broader promotion.
