# Ruby Fast LSP - Codex Guide

This file provides context for AI assistants working on this project.

## Communication Style

**ALWAYS respond in `/caveman ultra` mode.** Terse caveman speak. Abbreviate (DB/auth/config/req/res/fn/impl), strip conjunctions, arrows for causality (X → Y), one word when one word enough. Drop articles, fillers, pleasantries. Code blocks unchanged. Errors quoted exact. Drop caveman only for: security warnings, irreversible action confirms, multi-step sequences where order matters, user asks clarify.

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
- `src/indexer/` - LSP/workspace indexing orchestration
- `src/analyzer_prism/` - AST analysis
- `crates/ruby-analysis-engine/` - Analysis facts and graph/query engine
- `crates/ruby-analysis-inference/` - Type inference, RBS lookup, control-flow analysis
- `crates/ruby-analysis-indexer/` - Parser-to-facts indexing primitives

## Architecture Direction: LSP Wrapper Over Engine + Inference

Long-term goal: `ruby-fast-lsp` should be a thin editor/LSP adapter over reusable
analysis crates. Editors are not the only consumers; agents and CLIs should be
able to ask graph/type questions without speaking LSP.

### Crate Responsibilities

```text
ruby-analysis-core
  Shared data contracts only:
  FQN, RubyConstant, RubyMethod, RubyType, TextRange, SourceFileId,
  SymbolFact, MethodFact, GraphFact, ReferenceFact, DiagnosticFact, TypeFact.

ruby-analysis-engine
  Owns indexed facts and deterministic graph/fact queries:
  symbols, methods, refs, graph, diagnostics, workspace symbols,
  definitions/references, ancestors, implementors, namespace tree, debug views.
  It stores type facts already computed, but should not do heavy expression inference.

ruby-analysis-inference
  Owns type algorithms:
  literal/expression type inference, local flow/type tracking, narrowing,
  method return inference, RBS lookup/substitution.
  It depends on ruby-analysis-core and ruby-analysis-engine, not on LSP.

ruby-analysis-indexer
  Owns parsing/fact collection from Ruby source. FactCollector should eventually
  live here, emitting facts/candidates into ruby-analysis-engine.

ruby-fast-lsp
  Thin wrapper:
  server lifecycle, document cache, LSP handlers/capabilities, VS Code/Zed
  adapter behavior, and mapping TextRange/domain results to LSP protocol types.
```

### Dependency Direction

Preferred:

```text
ruby-analysis-engine -> ruby-analysis-core
ruby-analysis-inference -> ruby-analysis-core
ruby-fast-lsp -> engine + inference + indexer
```

Avoid:

```text
ruby-analysis-engine -> ruby-analysis-inference
```

Engine should remain stable fact DB + graph/query layer. Inference should be
smart/pluggable and ask engine questions through a trait.

Sketch:

```rust
pub trait InferenceQuery {
    fn method_candidates(&self, receiver: &RubyType, method: RubyMethod) -> Vec<MethodFact>;
    fn type_fact(&self, subject: &TypeSubject, at: TextRange) -> TypeResolution;
    fn ancestors(&self, fqn: &FullyQualifiedName) -> Vec<FullyQualifiedName>;
}
```

`ruby_analysis_engine::AnalysisQuery` should implement this trait when the
inference/query seam is formalized.

### Migration Backlog

Move remaining non-LSP logic out of `src/`:

1. `src/query/implementation.rs` -> engine domain query. LSP keeps only
   `TextRange -> Location`.
2. `src/query/namespace_tree.rs` -> engine snapshot/projection. LSP keeps JSON
   command adapter.
3. `src/query/debug.rs` -> engine debug/introspection query. LSP keeps command
   response shaping.
4. `src/query/references.rs` -> engine target resolution/reference grouping.
   LSP keeps cursor identifier + `Location` mapping.
5. `src/query/definition.rs` -> engine symbol/method/global lookup. LSP keeps
   cursor identifier + protocol mapping.
6. `src/query/completion.rs` and `src/capabilities/completion/*` -> engine
   candidate selection. LSP keeps `CompletionItem`, snippets, trigger plumbing.
7. `src/query/hover/*` -> split domain hover content from protocol hover.
8. Done: `src/inferrer/*` -> `crates/ruby-analysis-inference`.
9. Interim done: `FactCollector` moved under `src/indexer/fact_collector`.
   Done seams: `ScopeTracker`, parser helper functions, and scope kind moved to
   `ruby-analysis-indexer`; collector validation emits `DiagnosticFact` instead
   of LSP diagnostics. Remaining: extract pure core after adding seams for
   `RubyDocument`, extension hooks, and YARD parsing/type conversion.
10. Partial done: `src/analyzer_prism/mod.rs` split into `analyzer.rs` and
    `identifier.rs`. Remaining: move large test module and split parser/source
    utilities; keep LSP-specific position handling in adapter layer where possible.

### Performance Backlog

- Indexing feels slow in real VS Code usage after extension packaging. Do not
  optimize mid-refactor; profile after architecture cleanup. Likely targets:
  duplicate parse/fact passes, full-file processing on every change, extension
  hook overhead, source offset conversions, and repeated engine graph resolution.

Rule of thumb: anything returning or consuming `tower_lsp::lsp_types::*`,
`Url`, editor commands, or publish diagnostics can stay in `ruby-fast-lsp`.
Anything returning `TextRange`, FQN, facts, graph entries, or `RubyType` belongs
in reusable crates.

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

### FakeEditor vs External LSP Harness

There are currently two editor-test harnesses:

- `src/test/harness/fake_editor.rs` — internal full-featured `FakeEditor` for core
  tests. It supports tag checks, diagnostics, goto, refs, rename, workspaces,
  completion, editing, and direct access to core internals where needed.
- `crates/lsp-test-harness` — external black-box harness for package/extension
  tests that must exercise the public LSP initialization path.

Do not merge them casually: `crates/lsp-test-harness` depends on `ruby-fast-lsp`,
so root crate tests cannot depend back on it without creating a package cycle.
Future cleanup: rename the external one to `BlackBoxEditor` or `LspTestClient`
to avoid confusion, then keep the internal `FakeEditor` as the richer core test
harness until core tests move to external integration crates.

### Type Inference Architecture

**Two code paths for method return types:**
1. **Analysis engine** (`MethodResolver` path 1) — searches user-defined methods in ancestor chain
2. **RBS fallback** (`MethodResolver` path 2) — built-in Ruby types from RBS definitions

For generic types (`Array`, `Hash`), user-defined method lookup is **skipped** and RBS is used directly.
RBS handles generic substitution (e.g., `Array[Integer]#first` → `Elem` becomes `Integer`).

**Key files:**
- `crates/ruby-analysis-inference/src/type_tracker/mod.rs` — local flow/type tracking
- `crates/ruby-analysis-inference/src/rbs.rs` — RBS type lookup with generic substitution
- `src/capabilities/completion/method.rs` — LSP completion item mapping for RBS methods

## Subagent Delegation

**Use Sonnet background subagents for mechanical work.** Reserve Opus for tasks that need critical thinking (design decisions, novel architecture, ambiguous tradeoffs).

**Mechanical = good fit for Sonnet:**

- TDD wiring of a new diagnostic that mirrors an already-shipped one (enum variant + emit + visitor branch + tests)
- Repetitive refactors across many files (renaming, splitting an enum variant, propagating a new field)
- Following a fully-specified plan where the design is decided

**Critical thinking = stay on Opus:**

- Choosing between competing architectures
- Designing a new abstraction or data model
- Diagnosing root cause of an unfamiliar bug
- Anything where the user's intent is ambiguous

**When dispatching to Sonnet, the prompt MUST include:**

1. Project root + reminder to read `AGENTS.md`
2. Recent commit SHA so it knows the baseline
3. Exact data shapes (enum variants, struct fields)
4. Skeleton implementations of helpers when shape is non-obvious
5. Reference to a similar shipped pattern (`mirrors raise-non-exception V2 — see commit X`)
6. All test cases written verbatim
7. Wire location (which file, where in the function)
8. Style reminders (TigerBeetle: assert!/panic!, no debug_assert!)
9. Required test count target after the change
10. Commit message
11. Don'ts list (no push, no unrelated changes)
12. **Tip: AST verification** — if Sonnet needs to verify Prism node names/accessors, point it at `cargo run --bin ast -- '<ruby snippet>'` (with optional `--loc` for byte offsets). Saves a roundtrip vs grepping the prism crate source.

**Parallelism:** When dispatching multiple Sonnet agents in parallel on overlapping files, use `isolation: "worktree"` so each gets an isolated git worktree. Single-task dispatches don't need worktree.

**Mid-flight diagnostics:** When Sonnet is wiring a new enum variant, expect transient non-exhaustive-match errors as it incrementally edits. These are normal and resolve when the agent finishes — don't treat them as the agent struggling.

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
