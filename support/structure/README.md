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

Grouping decisions remain a code-review responsibility. This check cannot tell
whether a name is meaningful or whether unrelated code was merged into one file
under the line limit. Preserve module ownership, use semantic groups, and update source-path
references and reading guides with each move.
