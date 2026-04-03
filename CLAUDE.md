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
cargo build --release         # Release build
./create_vsix.sh --current-platform-only   # Build VS Code extension
```

## Critical Reminders

1. **LSP positions are 0-indexed** - Line 1 in editor = line 0 in LSP
2. **Prism uses byte offsets** - Must convert to LSP positions
3. **FQN-based indexing** - All symbols use fully qualified names (e.g., `MyModule::MyClass`)
4. **AST Traversal** - Use recursive traversal (visitor pattern) over ad-hoc matching for type inference to handle nesting/chaining correctly

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

## Testing

### Tag-Based Test Harness (`check()`)

Single-file tests use `check()` with inline tags. No fixtures needed:

```rust
use crate::test::harness::check;

#[tokio::test]
async fn my_test() {
    check(r#"
class User
  def name
    "hello"
  end
end

user = User.new
user.n$0
<complete items="name">
"#).await;
}
```

**Supported tags:**

| Tag | Requires `$0` | Purpose |
|-----|---------------|---------|
| `<complete items="a,b" excludes="c">` | Yes | Completion items at cursor |
| `<hint label="...">` | No | Inlay hint at position |
| `<def>...</def>` | Yes | Goto definition range |
| `<ref>...</ref>` | Yes | Reference range |
| `<type>...</type>` | Yes | Expected type at cursor |
| `<err>...</err>` | No | Expected error diagnostic |
| `<err none>...</err>` | No | Assert NO errors in range |
| `<warn>...</warn>` | No | Expected warning diagnostic |
| `<lens title="...">` | No | Expected code lens |
| `<th supertypes="A,B" subtypes="C,D">` | Yes | Type hierarchy |

Multi-file tests use `check_multi_file(&[("main.rb", "..."), ("other.rb", "...")])`.

### FakeEditor (Lifecycle/Re-indexing Tests)

Use `FakeEditor` to test behavior across edits — simulates open/edit/save cycle:

```rust
use crate::test::harness::FakeEditor;

#[tokio::test]
async fn types_survive_reindex() {
    let mut editor = FakeEditor::new().await;
    let code = "a = [1, 2, 3].first";

    // First indexing
    editor.open("test.rb", code);
    editor.check("test.rb", r#"a<hint label="Integer"> = [1, 2, 3].first"#).await;

    // Simulate edit (re-indexes)
    editor.set("test.rb", code);
    editor.check("test.rb", r#"a<hint label="Integer"> = [1, 2, 3].first"#).await;
}
```

**Key methods:**
- `editor.open("file.rb", content)` — first open + index
- `editor.set("file.rb", new_content)` — edit + re-index (bumps version)
- `editor.check("file.rb", fixture)` — assert with tags (content must match)
- `editor.close("file.rb")` — close file

**When to use FakeEditor vs check():**
- `check()` — single indexing pass, sufficient for most feature tests
- `FakeEditor` — when behavior differs between initial indexing and re-indexing (e.g., user index state changes after workspace indexing completes)

### Type Inference Architecture

**Two code paths for method return types:**
1. **User index** (`MethodResolver` path 1) — searches user-defined methods in ancestor chain
2. **RBS fallback** (`MethodResolver` path 2) — built-in Ruby types from RBS definitions

For generic types (`Array`, `Hash`), the user index is **skipped** and RBS is used directly.
RBS handles generic substitution (e.g., `Array[Integer]#first` → `Elem` becomes `Integer`).

**Key files:**
- `src/inferrer/method/resolver.rs` — `resolve_method_return_type()` orchestrates both paths
- `src/inferrer/rbs.rs` — RBS type lookup with generic substitution
- `src/capabilities/completion/method.rs` — method completion with ancestor chain walking

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
