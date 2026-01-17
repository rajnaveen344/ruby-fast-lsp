# Ruby Fast LSP - Claude Code Guide

This file provides context for AI assistants working on this project.

## Quick Reference

- **Project**: High-performance Ruby LSP written in Rust
- **Parser**: ruby-prism 1.4.0
- **Framework**: tower-lsp 0.20.0
- **Runtime**: tokio async

## Documentation

All AI-related documentation is maintained in the `.ai/` folder. Read these files for detailed context:

### Core Documentation (`.ai/steering/`)

| File                                                    | Purpose                                                          |
| ------------------------------------------------------- | ---------------------------------------------------------------- |
| [README.md](.ai/steering/README.md)                     | Entry point - architecture overview, feature status, quick start |
| [product.md](.ai/steering/product.md)                   | Feature overview and design philosophy                           |
| [structure.md](.ai/steering/structure.md)               | Project directory layout and code organization                   |
| [tech.md](.ai/steering/tech.md)                         | Tech stack, dependencies, common commands                        |
| [testing.md](.ai/steering/testing.md)                   | Testing strategy and test harness usage                          |
| [ruby-ast-mapping.md](.ai/steering/ruby-ast-mapping.md) | Ruby syntax to Prism AST node mapping                            |

### Additional Resources

- `.ai/docs/` - Deep-dive documentation on specific features (type inference, simulation testing, etc.)
- `.ai/specs/` - Feature specifications and implementation plans
- `.ai/diagrams/` - C4 architecture diagrams (LikeC4 format)

## Common Commands

```bash
cargo test                    # Run all tests
cargo test -- --nocapture     # With output
cargo insta review            # Update snapshots
cargo build --release         # Release build
./create_vsix.sh --current-platform-only   # Build VS Code extension
```

## Critical Reminders

1. **LSP positions are 0-indexed** - Line 1 in editor = line 0 in LSP
2. **Prism uses byte offsets** - Must convert to LSP positions
3. **FQN-based indexing** - All symbols use fully qualified names (e.g., `MyModule::MyClass`)
4. **Snapshot testing** - Use `cargo insta review` to accept/reject changes
5. **AST Traversal** - Use recursive traversal (visitor pattern) over ad-hoc matching for type inference to handle nesting/chaining correctly

## TigerBeetle Principles (MANDATORY)

**CRITICAL**: This project follows TigerBeetle's philosophy of correctness over convenience:

1. **Fail Fast and Loudly** - Use `assert!` and `panic!`, NOT `debug_assert!`

   - ❌ **NEVER** use `debug_assert!` - bugs must be caught in production too
   - ❌ **NEVER** silently return wrong results or default values
   - ❌ **NEVER** use wildcard `_` in match arms for panics/unreachable - be explicit
   - ✅ **ALWAYS** panic with clear error messages explaining what went wrong
   - ✅ **ALWAYS** crash the program if an invariant is violated

2. **Make Invalid States Unrepresentable**

   - Use type system to enforce invariants at compile time
   - Use assertions to enforce invariants at runtime
   - Example: `assert!(matches!(fqn, Namespace(_, _)))` to validate enum variants

3. **No Assumptions or Guessing**

   - If data is missing or invalid, PANIC - don't guess what it should be
   - Better to crash and know there's a bug than silently produce incorrect results
   - Example: `.expect("INVARIANT VIOLATED: ...")` instead of `.unwrap_or_default()`

4. **Clear Error Messages**
   - Every panic/assert must explain:
     - What invariant was violated
     - Why this is a bug
     - How to fix it
   - Format: `"INVARIANT VIOLATED: <what> is broken. This is a bug because <why>. Fix: <how>"`

**Why**: Production correctness is more important than "graceful degradation" that hides bugs.

## Key Entry Points

- `src/main.rs` - Application entry
- `src/server.rs` - LSP server core
- `src/handlers/` - Request/notification routing
- `src/capabilities/` - Feature implementations
- `src/indexer/` - Symbol indexing
- `src/inferrer/` - Type inference
- `src/analyzer_prism/` - AST analysis

## TDD Workflow

When the user provides a code scenario/example, follow this strict TDD process:

1. **Red**: Create an integration test that captures the expected behavior

   - Write the test first based on the scenario
   - Run the test to confirm it fails
   - Show the failing test output

2. **Green**: Implement the minimum code to make the test pass

   - If the change is substantial (architectural changes, new modules, cross-cutting concerns):
     - Use `EnterPlanMode` to design the feature
     - Ask clarifying questions about design decisions
   - Make targeted changes to fix the failing test
   - Run the test to confirm it passes

3. **Refactor** (if needed): Clean up while keeping tests green

**Important**: Always verify the test fails before implementing the fix. This validates the test actually tests the new behavior.
