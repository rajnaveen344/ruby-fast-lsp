# Real editor demos

Apply [vscode-settings.json](vscode-settings.json) only to a synthetic demo
workspace or recording profile. It disables built-in AI features, inline AI
completions, and Copilot next-edit suggestions while retaining language-server completion and
type hints. Word-based suggestions and snippets are hidden in the baseline so
the completion list shows language-server results. Enable snippets explicitly
when recording that feature.

For final recordings, use a profile containing Ruby Fast LSP and only the
extensions required by the specific example. Check the effective Ruby/ERB
settings, because a profile's language-specific override can override a general
workspace setting. Keep personal editor preferences outside this setup.

Use normal typing bursts, a brief pause at a word or punctuation boundary, and
a longer pause after a result appears. For a short receiver expression,
use **60–100 ms between ordinary keystrokes**, 200–400 ms around a deliberate pause,
and 1–2 seconds to read a completion or tooltip. Adjust deliberately for the
demonstration rather than adding random jitter or fake typing mistakes.

Capture continuously when smooth pointer movement matters. A capture assembled from real editor frames may use edited frame durations, but must be labeled as such; its
timing is not evidence of language-server latency. Keep enough real wait time
to show requests completing. Never synthesize editor results or hide an error
that contradicts the feature being demonstrated.

Keep the initial example ready on screen and type only the meaningful edit.
Move to one target, stop, open its tooltip or completion, and let the viewer
read it before the next action. Prefer a short clip with a single clear
outcome over typing an entire example from scratch.

Raw captures, export experiments, and temporary projects stay under
`target/docs-capture/`. Keep reviewed GIFs **in the repository** under
`pages/public/demos/`, grouped by feature, with video versions and metadata.
Keep synthetic source examples in `demos/fixtures/` so clips can be reproduced.
See the [sitemap and shot list](../planning/feature-docs.md) and
[positioning notes](../planning/positioning.md).

The first edition contains 14 clips. Every asset folder contains its exact
actions and any capture-specific limitation. Most clips use the current source
in an extension development host; the previously approved completion sample
records its installed version and explicitly marks its source commit as unknown.
