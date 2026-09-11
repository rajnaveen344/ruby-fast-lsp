# Documentation

Start with the root [README](../README.md) for installation and project priorities.
[Usage](usage.md) covers editor workflows, runtime/dependency behavior, support
boundaries, and bug reports.

## Feature contracts

- [Definition navigation](features/definition-navigation.md): token identity,
  effective implementations, ambiguity, and semantic ordering.
- [Structural Hash shapes](features/structural-hash-shapes.md): supported reads,
  mutations, correlations, and proof limits.
- [Higher-order calls](features/higher-order-call-inference.md): collection blocks,
  signatures, forwarding, and supported callable forms.
- [Callable bodies](features/callable-body-inference.md): parameter-dependent
  lambda/proc results, captures, aliases, and lifecycle behavior.

## Development

- [Architecture](../src/ARCHITECTURE.md) and [analysis library](../crates/ruby-analysis/README.md).
- [Server state ownership](development/server-state.md).
- [Testing](../src/test/README.md) and [simulation](development/simulation.md).
- [Performance workflow](development/performance.md).
- [Release checklist](development/release.md).
- [Contributor rules](../AGENTS.md) and [optional focused workflows](../.agents/README.md).

Keep guides about current behavior and repeatable workflows. Executable checks,
acceptance data, packaged runtime inputs, and immutable measurement records belong
in [support/](../support/README.md). Release logs and local profiles belong under
`target/`; commit history retains completed plans and old release drafts.
