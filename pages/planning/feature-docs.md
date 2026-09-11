# Feature documentation sitemap

The first edition has an overview, 19 content pages, and 14 real editor demos.
Routes use hashes under `/ruby-fast-lsp/`, so static hosting can open links
without a server-side router. The site is built locally; publication is separate.

## Overview and README

The overview leads with “See the types flowing through your Ruby” and one
Hash-shape demo. Four static previews introduce propagation, Hash contents,
type hints, and explained uncertainty. Six feature groups link to the guide.
The repository README reuses the same Hash-shape GIF and links to the guide.

## Pages

| Group                    | Routes after `#/`                                                                            | Demos                                |
| ------------------------ | -------------------------------------------------------------------------------------------- | ------------------------------------ |
| Start                    | `install`                                                                                    | —                                    |
| Understand types         | `types/hash-shapes`, `types/propagation`, `types/hints`, `types/unknown`, `types/signatures` | First four                           |
| Navigate                 | `navigate/definition`, `navigate/rename`, `navigate/symbols`                                 | First two                            |
| Write and edit           | `editing/completion`                                                                         | Completion                           |
| Diagnose and fix         | `diagnostics/live`, `diagnostics/fixes`                                                      | Live correction; safe formatting     |
| Projects and runtimes    | `projects/indexing`, `projects/isolation`, `projects/jruby`                                  | All three                            |
| Frameworks and templates | `frameworks/templates`, `frameworks/tests`                                                   | ERB navigation; focused Minitest run |
| Reference                | `extensions`, `support`                                                                      | —                                    |

Related capabilities share a page when that makes the guide easier to browse:
references and hierarchies accompany navigation; editor structure accompanies
symbols; formatting accompanies linters; framework support accompanies templates.

## Media contract

Each demo has a GIF, MP4, poster, written steps, and metadata in
`public/demos/<group>/<feature>/`. Generic source fixtures live under
`demos/fixtures/`. Raw frames and temporary workspaces stay under
`target/docs-capture/`.

The recordings use actual editor frames. Typing playback uses ordinary gaps of
60–100 ms where typing is shown, with longer reading pauses. They are sampled
and retimed, not continuous video or latency evidence. The website and README
use automatically looping GIFs. The website offers a pause control and starts
with a still poster when reduced motion is preferred.

## Future examples

Add nuance when it explains a supported behavior: multiple definition targets,
cross-file edits, correlated Hash alternatives, framework-specific navigation,
and cold indexing. Keep the initial examples short. Do not turn this document
into a second semantic contract or a session log.

Use the maintained [usage guide](../../docs/usage.md),
[feature contracts](../../docs/README.md#feature-contracts), and
[extension manifests](../../extensions/README.md) for precise support boundaries.
The [positioning brief](positioning.md) links the comparison sources.
