# Source folder organization

The correctness gate runs `python3 -B support/structure/check.py` and the tests
in this folder before the workspace tests. Run those checks locally with:

```sh
python3 -B support/structure/check.py
python3 -B -m unittest discover -s support/structure -p 'test_*.py'
```

The limit is ten immediate entries: files, subfolders, `mod.rs`, and READMEs
all count. Git-tracked and new non-ignored files are included; removed files and
ignored build output are omitted. Counts are recursive within the maintained
roots recorded in `policy.json`. JSON output is available with `--json`.

The entire `ruby-analysis` tree is strict: it cannot acquire a legacy allowance
or an excluded subtree. Other existing oversized folders retain an exact entry
baseline. A same-size replacement still introduces a new entry and fails. When
removing entries, trim the baseline; once a folder meets the ordinary limit,
remove its allowance. Do not expand the baseline to admit new work.

Bundled RBS/stub snapshots and vendored dependency code have explicit ownership
exclusions. Maintained support folders follow the ordinary limit. The enclosing maintained folder still
counts each data directory once. These are data-layout decisions, not approved
exceptions for source code.

There are currently no source-family exceptions. A future exception must name
a cohesive family with no useful semantic split, explain it in that folder's
README, and declare a finite `max_entries` and matching `readme_excerpt` under
`exceptions`. The checker validates the documentation and bound; reviewers must
evaluate the semantic justification. An exception and legacy debt are separate
concepts and cannot be combined for the same folder.

Source files (`.rs`, `.py`, `.js`, `.mjs`, `.ts`, `.sh`, `.rb`) inside the
audited roots are limited to 1,000 lines, tests included. No file currently
has a ceiling under `legacy_lines`. Do not add one to admit new work; split the
file by responsibility instead.

## Module layering

The `layering` policy forbids upward imports by crate root module. Rust files
under `src/loader`, `src/environment`, and `src/utils` may not name
`crate::server`, `crate::lsp`, or `crate::features`; files under `src/server`
may not name `crate::lsp` or `crate::features`; and files under
`src/features` may not name `crate::lsp`. Features read `server` state and
`loader` products; an item a feature needs from `lsp` moves to its owner
instead of gaining an exemption. The check reads `crate::` paths, including
`crate::{...}` groups by their top-level module, and ignores `//` comments.
Each violation reports its file and line.

Tests are held to the same rule. A test that drives the server, lsp
lifecycle handlers, or feature handles belongs in `src/test/integration/`. The only escape is
`test_exemptions`, an exact list of test-module files (under a `tests/`
folder, or named `tests.rs` or `*_tests.rs`), each with a reason. An
exemption that no longer violates fails the check, so remove it when the test
moves. Do not add an exemption for production code or to admit new work.

Grouping decisions remain a code-review responsibility. This check cannot tell
whether a name is meaningful or whether unrelated code was merged into one file
under the line limit. Preserve module ownership, use semantic groups, and update source-path
references and reading guides with each move.
