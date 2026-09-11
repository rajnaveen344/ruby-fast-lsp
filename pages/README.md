# Ruby Fast LSP feature guide

A static React/Vite documentation site with an overview and 19 focused pages.
The overview starts with navigation, completion, hover, diagnostics, and
formatting, followed by type inference and project tools.
Feature pages explain the editor actions, examples, prerequisites, and limits.

The site includes **14 real editor demos**. GIFs, MP4s, posters, transcripts, and
capture metadata live in [public/demos](public/demos). The root repository README
reuses the completion GIF. Playback timings are edited for readability and are
not performance measurements.

## Develop locally

```sh
cd pages
npm ci
portless ruby-fast-lsp npm run dev
```

Open the URL printed by Portless with `/ruby-fast-lsp/` appended. Portless is a
local development tool; it is not a project dependency. Without Portless, use
`npm run dev` and the URL printed by Vite.

## Build

```sh
npm run build
npm run preview
```

The static output is `dist/`. The deployment base is `/ruby-fast-lsp/`; override
it with `npm run build -- --base=/` for a domain root. Hash routes keep direct
feature links usable on static hosting. Building does not publish the site.

## Maintain

- [src/content](src/content) owns grouped feature content and navigation.
- [src/components](src/components) owns media, code, and shared navigation UI.
- [Recording guide](demos/README.md) documents generic fixtures and capture settings.
- [Sitemap](planning/feature-docs.md) maps the current first edition.
- [Positioning](planning/positioning.md) records primary sources for comparisons.

Write feature pages around everyday use: what the feature does, how to use it,
and practical limitations. Keep bug-specific examples, implementation details,
and regression history in the linked technical guides or tests. Demo transcripts
describe the recorded actions and should remain accurate to their capture.

GIFs loop automatically when loaded near the viewport. Visitors can pause a
loop to show its poster; reduced-motion preferences start with the poster.
MP4 versions remain in the repository as alternate assets. Each clip demonstrates
only the actions in its transcript, not every behavior described on the feature page.
