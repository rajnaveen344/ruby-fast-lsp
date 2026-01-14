# TigerStyle Coding Guidelines

This document captures coding principles adapted from [TigerBeetle's TigerStyle](https://tigerstyle.dev/) methodology for use in the Ruby Fast LSP project.

## Core Philosophy

> "Do the hard thing today to make tomorrow easy."

TigerStyle prioritizes three values **in order**: Safety → Performance → Developer Experience. All three matter, but safety is non-negotiable.

---

## 1. Safety

### Zero Technical Debt

- **Do it right the first time.** Code is easier to change while it's "hot."
- Quality builds momentum. What we ship is solid.
- Solve problems in design, not production.

### Assertions & Invariants

- **Assert function arguments, return values, and invariants.**
- Assert both the _positive space_ (what you expect) and the _negative space_ (what you don't expect).
- Target minimum 2 assertions per function.
- **Pair assertions**: enforce properties at multiple code paths (e.g., before write and after read).
- Split compound assertions: `assert(a); assert(b);` over `assert(a && b);`.
- Use compile-time assertions for type sizes and constant relationships.

### Explicit Limits

- **Put a limit on everything** — loops, queues, buffers, recursion depth.
- Bound all resources, concurrency, and execution.
- Prefer fixed-size types (`u32`) over architecture-specific types (`usize`).
- Avoid unbounded recursion.

### Logical Interfaces

- Minimize surface area of interfaces.
- Define fault models explicitly.
- Push control flow up and data flow down.
- Simplify function signatures to reduce branching at call sites.

### Error Handling

- **Handle all errors.** 92% of catastrophic failures stem from incorrect error handling.
- Split complex `else if` chains into explicit `else { if { } }` trees.
- State invariants positively — prefer `if (index < length)` over negations.

### Memory Safety

- Prefer static allocation when possible.
- Declare variables at the smallest possible scope.
- Minimize variables in scope to prevent misuse.

---

## 2. Performance

### Think Performance from the Start

> "The lack of back-of-the-envelope performance sketches is the root of all evil."

- Design for performance upfront — the 1000x wins come from design, not profiling.
- Work with mechanical sympathy.

### The Four Primary Resources

Perform sketches against:

1. **Network** (bandwidth, latency)
2. **Disk** (bandwidth, latency)
3. **Memory** (bandwidth, latency)
4. **CPU** (bandwidth, latency)

Optimize in that order, compensating for frequency of use.

### Batching & Amortization

- Separate control plane from data plane.
- Batch accesses to amortize costs.
- Let the CPU sprint through large units of work — be predictable.

### Zero Copy

- Do things in the most direct way possible.
- Minimize memory copies in hot paths.
- Don't thrash the CPU cache.

---

## 3. Developer Experience

### Naming Things

> "Great names are the essence of great code."

- **Use `snake_case`** for functions, variables, and file names.
- **Don't abbreviate** — use full, descriptive names.
- Add units/qualifiers as suffixes: `latency_ms_max` not `max_latency_ms`.
- Use same character count for related names so they align: `source`/`target`.
- Prefix helper functions with the calling function's name: `read_sector_callback()`.

### Code Organization

- **Hard limit: 70 lines per function.** Art is born of constraints.
- Centralize control flow in parent functions; keep helpers pure.
- Push `if`s up and `for`s down.
- Order matters: put important things near the top.
- Run the formatter. Respect the 100-column hard limit.

### Comments & Documentation

- **Always say why.** Code alone is not documentation.
- Comments are complete sentences with proper punctuation.
- For tests, explain goal and methodology at the top.
- Write descriptive commit messages that inform and delight.

### Scoping & Variables

- Calculate/check variables close to where they're used.
- Don't introduce variables before they're needed.
- Don't duplicate variables or create aliases.
- Shrink scope to reduce bugs.

### Dependencies & Tooling

- Minimize dependencies — they invite risk and slow installs.
- Use a small, standard toolbox rather than specialized instruments.
- Invest in existing tools rather than adding new ones.

---

## Quick Checklist

Before committing, verify:

- [ ] All error paths are handled
- [ ] Assertions check arguments, returns, and invariants
- [ ] Bounds exist on loops, buffers, and recursion
- [ ] Function is under 70 lines
- [ ] Variables are scoped tightly
- [ ] Names are descriptive with units/qualifiers as suffixes
- [ ] Comments explain _why_, not just _what_
- [ ] No unnecessary dependencies added

---

## Reference

- [TigerStyle Guide](https://tigerstyle.dev/)
- [Full TigerStyle Essay](https://github.com/tigerbeetle/tigerbeetle/blob/main/docs/TIGER_STYLE.md)
- [NASA Power of Ten Rules](https://spinroot.com/gerard/pdf/P10.pdf)
