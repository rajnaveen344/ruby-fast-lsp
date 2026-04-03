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

FakeEditor routes all operations through the **real LSP handlers** (`handle_did_open`,
`handle_did_change`, etc.), ensuring tests exercise the exact same code paths as a real editor.

#### Tag-based assertions (simple cases)

```rust
use crate::test::harness::FakeEditor;

#[tokio::test]
async fn types_survive_reindex() {
    let mut editor = FakeEditor::new().await;
    let code = "a = [1, 2, 3].first";

    editor.open("test.rb", code).await;
    editor.check("test.rb", r#"a<hint label="Integer"> = [1, 2, 3].first"#).await;

    editor.set("test.rb", code).await;
    editor.check("test.rb", r#"a<hint label="Integer"> = [1, 2, 3].first"#).await;
}
```

#### Programmatic assertions (complex scenarios)

```rust
#[tokio::test]
async fn completion_filtering() {
    let mut editor = FakeEditor::new().await;
    editor.open("test.rb", "user = User.new\nuser.").await;

    // Type "na" after the dot
    editor.type_at("test.rb", 1, 5, "na").await;
    let items = editor.complete_with_trigger("test.rb", 1, 7, ".").await;
    assert!(items.iter().any(|i| i.label == "name"));

    // Backspace and retype
    editor.backspace_at("test.rb", 1, 7, 2).await;
    editor.type_at("test.rb", 1, 5, "to").await;
    let items = editor.complete_with_trigger("test.rb", 1, 7, ".").await;
    assert!(items.iter().any(|i| i.label == "to_s"));
}
```

**Lifecycle methods** (all async, route through real handlers):
- `editor.open("file.rb", content).await` — triggers `handle_did_open`
- `editor.set("file.rb", new_content).await` — triggers `handle_did_change`
- `editor.save("file.rb").await` — triggers `handle_did_save`
- `editor.close("file.rb").await` — triggers `handle_did_close`

**Editing methods** (simulate typing):
- `editor.type_at("file.rb", line, char, "text").await` — insert text at position
- `editor.backspace_at("file.rb", line, char, count).await` — delete before position

**Query methods** (return raw LSP results for programmatic assertions):
- `editor.complete_at(file, line, char)` — completion items (no trigger context)
- `editor.complete_with_trigger(file, line, char, ".")` — completion with trigger
- `editor.hover_at(file, line, char)` — hover information
- `editor.goto_def_at(file, line, char)` — definition locations
- `editor.references_at(file, line, char)` — reference locations
- `editor.inlay_hints(file)` — all inlay hints for file
- `editor.code_lens(file)` — all code lenses for file
- `editor.diagnostics(file)` — all diagnostics for file
- `editor.rename_at(file, line, char, "new_name")` — rename workspace edit

**Apply methods**:
- `editor.apply_edit(&workspace_edit).await` — apply rename/code action results
- `editor.content("file.rb")` — get current file content

**When to use FakeEditor vs check():**
- `check()` — single indexing pass, sufficient for most feature tests
- `FakeEditor` — lifecycle tests, completion filtering, multi-step scenarios, snippet testing

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
