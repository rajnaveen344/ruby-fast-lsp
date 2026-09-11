# Feature positioning

Research checked 2026-09-11 against official project documentation. This is a
content brief, not a completed comparative benchmark. Record exact published
versions and run identical fixtures before publishing exclusive feature claims
or a pass/fail comparison table.

## Lead with everyday editor tools

Headline: **Ruby tools for everyday coding.**

Introduce navigation, completion, hover, rename, diagnostics, and formatting
before specialized type features. Describe what a Ruby developer can do in the
editor. Avoid implementation terminology and explanations of individual fixes.

Give type inference and Hash shapes their own overview section and feature
pages. These topics add depth after the everyday editor workflow:

| Story                             | What the demo should prove                                                                                       | Current contract                                                         |
| --------------------------------- | ---------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| Infer types from Ruby             | Inspect method results and collection element types without extra annotations                                    | [Higher-order calls](../../docs/features/higher-order-call-inference.md) |
| Understand the contents of a Hash | Compact generic hints, formatted nested shapes on hover, literal-key completion, and a known field's result type | [Hash shapes](../../docs/features/structural-hash-shapes.md)             |
| Read and follow inferred types    | Read types on hover or inline, and follow linked types to their declarations                                     | [Usage](../../docs/usage.md)                                             |
| Understand unknown types          | Read available explanations and provide more context with signatures when needed                                 | [Hash proof boundaries](../../docs/features/structural-hash-shapes.md)   |

Then show secondary strengths: independently indexed Ruby projects, exact
runtime/dependency selection, and JRuby/Java navigation. Explain their concrete
benefit with a working example. Rust implementation belongs in a technical
note; performance superiority requires comparable measurements.

These are strengths to demonstrate. They are not yet claims that every other
Ruby tool lacks every behavior in this table.

## Comparison with Ruby LSP

Ruby LSP already documents hover, navigation, completion, signatures, diagnostics,
inlay hints, templates, and framework integrations. Its completion guide describes
known receivers such as literals, constants, and direct object construction.
Its guessed-types feature also describes using identifier names, with explicit
limitations. Its documented inlay options expose implicit rescue and omitted Hash
values. See the official [completion](https://shopify.github.io/ruby-lsp/#completion),
[guessed types](https://shopify.github.io/ruby-lsp/#guessed-types), and
[inlay hints](https://shopify.github.io/ruby-lsp/#inlay-hints) sections.

Our proposed emphasis is inferred value flow, structural Hash contents, and
visible variable/return types. Do not describe Ruby LSP as syntax-only or as
having no type awareness. An omission from a documentation page does not prove
an implementation lacks a capability, including capabilities provided by add-ons.

## Comparison with Solargraph

Solargraph explicitly provides type inference and a type checker combining
inference with YARD annotations. It can report unknown methods and type
mismatches, and provides multiple checking levels. See its official
[type-checking guide](https://solargraph.org/guides/type-checking) and
[YARD guide](https://solargraph.org/guides/yard).

Neither inference without annotations nor YARD support is an exclusive selling
point against Solargraph. Compare the exact behavior of nested Hash fields,
branch correlations, aliases and mutation, collection/block propagation, and
live type presentation. These are candidate differentiators to verify with
identical examples, not established competitor failures.

## How to make a publishable comparison

Use independent, fresh profiles with one Ruby provider each. Keep the same
synthetic fixture, Ruby/dependency versions, and editor actions. State defaults,
extra configuration, signatures, and add-ons. Exercise at least:

1. Method result to local binding to method completion, with no user annotations.
2. A frozen Symbol collection used from another file and inside a block.
3. A nested Hash field read, key completion, and formatted shape tooltip.
4. A correlated shape union narrowed by a discriminator, followed by a known
   mutation and an unsupported escape.
5. An edit that changes a known type and refreshes hints/completion/diagnostics.

Record outcomes as demonstrated support, partial behavior, or not established.
Keep behavior evidence separate from timing; edited promotional clips cannot
substantiate latency claims. Recheck any public comparison when its versions
change.

## Homepage order

1. Headline, installation link, and everyday navigation, editing, and diagnostics.
2. A completion recording demonstrating an ordinary editor workflow.
3. A separate type inference section, then project and framework features.
4. A short, sourced “How it compares” section; link detailed reproduction results
   only after the comparison runs exist.
5. Support limits and contribution links.

Avoid agent-centric copy and unqualified speed/accuracy claims. The primary
audience is a Ruby developer deciding whether this tool helps with their code.
