# Deterministic Simulation Testing (DST) for Ruby Fast LSP

## Implementation Status ✅

| Phase                              | Status      | Description                                                          |
| ---------------------------------- | ----------- | -------------------------------------------------------------------- |
| **Phase 1: Foundation**            | ✅ Complete | Dependencies, module structure, Model, basic harness                 |
| **Phase 2: Edit Testing**          | ✅ Complete | `DidChange` with text sync verification                              |
| **Phase 3: Query Testing**         | ✅ Complete | All 15 operations with random sequences                              |
| **Phase 4: Marker Strategy**       | ✅ Complete | GotoDefinition, References, DocumentSymbols, FoldingRanges, CodeLens |
| **Phase 5: Type Inference**        | ✅ Complete | Type assignments, narrowing, stability, inlay hints                  |
| **Phase 6: Complex Mixins**        | ✅ Complete | Diamond inheritance, deep chains, edge cases                         |
| **Phase 7: Deterministic Editing** | ✅ Complete | Position tracking after edits, safe edit generators                  |
| **Phase 8: CI Integration**        | ⏳ Pending  | GitHub Actions workflow                                              |

### Tests Implemented (21 tests)

| Test                                    | Type     | Description                                         |
| --------------------------------------- | -------- | --------------------------------------------------- |
| **`sim`**                               | TRUE SIM | THE main simulation: tracked code + edits + queries |
| `test_position_before_edit_unchanged`   | Unit     | Positions before edit stay unchanged                |
| `test_position_after_edit_line_shifted` | Unit     | Positions after edit shift correctly                |
| `test_position_inside_edit_destroyed`   | Unit     | Positions inside deleted range are destroyed        |
| `test_position_on_same_line_after_edit` | Unit     | Same-line character adjustment works                |
| `test_multiline_edit_collapse`          | Unit     | Multi-line edits adjust positions correctly         |
| `test_tracked_code_apply_edit`          | Unit     | TrackedCode.apply_edit updates markers              |
| `test_apply_edit_insert`                | Unit     | Model edit insertion works                          |
| `test_apply_edit_replace`               | Unit     | Model edit replacement works                        |
| `test_apply_edit_multiline`             | Unit     | Model multiline edit works                          |
| `test_model_open_close`                 | Unit     | Model open/close lifecycle                          |
| `test_model_edit`                       | Unit     | Model edit function                                 |
| `soak`                                  | Soak     | Long-running test (ignored by default)              |

### The TRUE Simulation Runner

The `simulation_runner` test is the ONE comprehensive simulation that:

1. **Generates tracked code** with known definition/reference positions (18 different scenarios)
2. **Performs random operations** (edits, queries, saves, type checks) in random order (10-50 steps)
3. **Verifies definitions AND types survive** after each edit
4. **Tests all LSP operations** don't crash

```
┌─────────────────────────────────────────────────────────────────────┐
│                    SIMULATION RUNNER FLOW                           │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  proptest generates:                                                │
│  ├── tracked_code() → Ruby with known markers (18 scenarios)        │
│  ├── edit_lines, edit_texts → random edit parameters                │
│  ├── query_lines, query_chars → random query positions              │
│  ├── verify_indices → which markers to check                        │
│  └── step_order → random sequence of operations (0-14)              │
│                                                                     │
│  For each iteration:                                                │
│  1. Open tracked file                                               │
│  2. Execute 10-50 random steps:                                     │
│     - Edit (insert text at random line)                             │
│     - VerifyDefinition (check marker still resolves)                │
│     - VerifyType (check type inlay hints) [NEW]                     │
│     - VerifyCompletion (check expected methods) [NEW]               │
│     - QuerySymbols, QueryCompletion, QueryReferences                │
│     - QueryHover, QueryInlayHints [NEW]                             │
│     - QuerySemanticTokens, QueryFoldingRanges, QueryCodeLens        │
│     - Save                                                          │
│  3. Report: errors, warnings, stats (defs, types, completions)      │
│  4. Assert: no hard errors                                          │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Simulation Step Types (14 types)

| Step Type             | Weight | Description                                          |
| --------------------- | ------ | ---------------------------------------------------- |
| `Edit`                | 15%    | Safe edit with deterministic position tracking       |
| `VerifyDefinition`    | 20%    | Check definition marker still resolves (after edits) |
| `VerifyCompletion`    | 7%     | Check completion includes expected methods           |
| `Save`                | 5%     | Save the file (triggers full reindex)                |
| `QuerySymbols`        | 7%     | Query document symbols                               |
| `QueryCompletion`     | 7%     | Query completion at random position                  |
| `QueryReferences`     | 7%     | Query references at random position                  |
| `QueryHover`          | 7%     | Query hover at random position                       |
| `QueryInlayHints`     | 5%     | Query inlay hints for entire file                    |
| `QuerySemanticTokens` | 5%     | Query semantic tokens                                |
| `QueryFoldingRanges`  | 5%     | Query folding ranges                                 |
| `QueryCodeLens`       | 5%     | Query code lens                                      |

### Deterministic Edit Tracking

The simulation now supports **deterministic edit tracking**. When an edit is applied:

1. **Position Adjustment**: All marker positions are adjusted based on the edit range and new text
2. **Safe Edits**: Only "safe" edits are used that don't destroy markers:
   - Insert blank lines
   - Insert comments
   - Append to file
3. **Verification After Edits**: Definitions are verified to still resolve correctly after edits

```rust
/// Safe edit types that won't destroy markers
enum SafeEdit {
    InsertBlankLine { line: u32 },
    InsertComment { line: u32, text: String },
    InsertMethod { before_end_line: u32, method_name: String },
    AppendToFile { text: String },
}
```

Position adjustment handles:

- Positions before edit: unchanged
- Positions after edit: shifted by line delta
- Positions inside deleted range: marker destroyed (removed)
- Same-line positions after edit: character adjusted

### Bugs Found by Simulation Testing 🐛

1. **Document Symbols Bug** (Fixed): Multiple top-level classes only returned first class due to scope_id collision with top-level scope (scope_id=0).

2. **Array Element Type Inference** (Open): `[1, 2, 3].first` returns `: Elem` instead of `Integer`. The type system uses a generic `Elem` placeholder instead of inferring the actual element type.

3. **Method Chain Type Loss** (Open): Chained method calls lose type information:
   - `"hello".upcase` → expected `String`, got "no hint"
   - `[1,2,3].first.to_s` → expected `String`, got "no hint"
   - `{ a: 1 }[:a].to_s` → expected `String`, got "no hint"

4. **Hash Access Type Inference** (Open): `{ a: 1 }[:a]` doesn't propagate the value type for subsequent method calls.

---

## TL;DR - Operations Coverage

| Category               | Operations                                                        | Assertion Level          |
| ---------------------- | ----------------------------------------------------------------- | ------------------------ |
| **Document Lifecycle** | `DidOpen`, `DidChange`, `DidSave`, `DidClose`                     | Level 1: Text Sync       |
| **Navigation**         | `Definition`, `References`                                        | Level 2: Marker Strategy |
| **Intelligence**       | `Completion`, `Hover`, `InlayHints`, `SemanticTokens`             | Level 1-2                |
| **Structure**          | `DocumentSymbols`, `WorkspaceSymbols`, `FoldingRange`, `CodeLens` | Level 2: Completeness    |
| **Formatting**         | `OnTypeFormatting`                                                | Level 3: Idempotency     |

**Total: 15 LSP operations** covered by simulation testing.

---

## Core Philosophy

**Manual unit tests cannot cover the chaotic order of user keystrokes.**

Instead, we use **Stateful Property-Based Testing**. We define an abstract **Model** (a perfect simplification) and verify that the real LSP implementation never diverges from it, regardless of the sequence of events.

```
┌─────────────────────────────────────────────────────────────────────┐
│                         The DST Loop                                │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│   ┌──────────────┐         ┌──────────────┐                        │
│   │   proptest   │ ──────▶ │  Transitions │ ──┬──▶ Model (Oracle)  │
│   │   (PRNG)     │         │  (User Acts) │   │                    │
│   └──────────────┘         └──────────────┘   └──▶ Real LSP Server │
│         │                                              │            │
│         │                                              ▼            │
│         │                                     ┌──────────────┐      │
│         │                                     │   ASSERT:    │      │
│         │                                     │ Model == LSP │      │
│         │                                     └──────────────┘      │
│         │                                              │            │
│         ▼                                              ▼            │
│   On Failure: Print Seed + Minimal Steps (Shrinking)               │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 1. The Stack

| Component     | Choice                    | Notes                                          |
| ------------- | ------------------------- | ---------------------------------------------- |
| **Framework** | `proptest`                | Rust's standard for property-based testing     |
| **Method**    | `proptest::state_machine` | Designed for stateful systems                  |
| **Seed**      | Handled by proptest       | On failure, prints seed for exact reproduction |

```toml
# Cargo.toml
[dev-dependencies]
proptest = "1.4"
proptest-state-machine = "0.2"
```

---

## 2. The Architecture

### A. The Model (Oracle)

A simplified, **infallible** representation of truth. For an LSP, this is just a `HashMap`:

```rust
/// The Model: Simple, correct, impossible to get wrong
type Model = HashMap<String, String>; // Filename -> Content
```

The Model doesn't parse Ruby. It doesn't build ASTs. It just tracks what text should be in each file. If the LSP's internal buffer ever differs from the Model, **something is broken**.

### B. The Transitions (All LSP Operations)

Define ALL possible moves a user can make. `proptest` generates random sequences of these.

#### Document Lifecycle (Mutations)

| Transition  | Description              | Strategy                                                                   |
| ----------- | ------------------------ | -------------------------------------------------------------------------- |
| `DidOpen`   | Open a file with content | Generate random filename + random Ruby content                             |
| `DidChange` | Edit an open file        | Pick valid file from Model, generate valid range, replace with random text |
| `DidSave`   | Save a file              | Pick valid file from Model                                                 |
| `DidClose`  | Close a file             | Pick valid file from Model                                                 |

#### Navigation Queries (Read-only)

| Transition   | Description         | Strategy                          | Assertion Level  |
| ------------ | ------------------- | --------------------------------- | ---------------- |
| `Definition` | Go to definition    | Pick valid file + random position | Level 2 (Marker) |
| `References` | Find all references | Pick valid file + random position | Level 2 (Marker) |

#### Intelligence Queries (Read-only)

| Transition          | Description                | Strategy                          | Assertion Level        |
| ------------------- | -------------------------- | --------------------------------- | ---------------------- |
| `Completion`        | Get completion suggestions | Pick valid file + random position | Level 1 (No crash)     |
| `CompletionResolve` | Resolve completion item    | Pick from previous completion     | Level 1 (No crash)     |
| `Hover`             | Get hover information      | Pick valid file + random position | Level 1 (No crash)     |
| `InlayHints`        | Get inlay hints for range  | Pick valid file + random range    | Level 2 (Completeness) |
| `SemanticTokens`    | Get semantic highlighting  | Pick valid file                   | Level 2 (Determinism)  |

#### Document Structure Queries (Read-only)

| Transition         | Description                 | Strategy                     | Assertion Level        |
| ------------------ | --------------------------- | ---------------------------- | ---------------------- |
| `DocumentSymbols`  | List symbols in document    | Pick valid file              | Level 2 (Completeness) |
| `WorkspaceSymbols` | Search symbols in workspace | Generate random query string | Level 1 (No crash)     |
| `FoldingRange`     | Get foldable regions        | Pick valid file              | Level 2 (Determinism)  |
| `CodeLens`         | Get code lens annotations   | Pick valid file              | Level 2 (Determinism)  |

#### Formatting (Read-only, but produces edits)

| Transition         | Description         | Strategy                       | Assertion Level       |
| ------------------ | ------------------- | ------------------------------ | --------------------- |
| `OnTypeFormatting` | Format on keystroke | Pick valid file + trigger char | Level 3 (Idempotency) |

**Key insight for `DidChange`**: Generate ranges within valid bounds of the Model's current content. This ensures we're testing real edit scenarios, not garbage input.

```rust
#[derive(Debug, Clone)]
enum Transition {
    // === Document Lifecycle ===
    DidOpen { filename: String, content: String },
    DidChange { filename: String, range: Range, new_text: String },
    DidSave { filename: String },
    DidClose { filename: String },

    // === Navigation ===
    Definition { filename: String, position: Position },
    References { filename: String, position: Position, include_declaration: bool },

    // === Intelligence ===
    Completion { filename: String, position: Position },
    CompletionResolve { item: CompletionItem },
    Hover { filename: String, position: Position },
    InlayHints { filename: String, range: Range },
    SemanticTokens { filename: String },

    // === Document Structure ===
    DocumentSymbols { filename: String },
    WorkspaceSymbols { query: String },
    FoldingRange { filename: String },
    CodeLens { filename: String },

    // === Formatting ===
    OnTypeFormatting { filename: String, position: Position, ch: char },
}
```

---

## 3. Implementation

### 3.1 The State Machine

```rust
use proptest::prelude::*;
use proptest_state_machine::{prop_state_machine, ReferenceStateMachine};
use std::collections::HashMap;

/// The Model: What the LSP's state SHOULD be
#[derive(Clone, Debug, Default)]
struct LspModel {
    files: HashMap<String, String>,
}

/// The real system under test
struct LspUnderTest {
    server: RubyLanguageServer,
}

impl ReferenceStateMachine for LspModel {
    type State = Self;
    type Transition = Transition;

    fn init_state() -> BoxedStrategy<Self::State> {
        Just(LspModel::default()).boxed()
    }

    fn transitions(state: &Self::State) -> BoxedStrategy<Self::Transition> {
        let open_files: Vec<String> = state.files.keys().cloned().collect();

        if open_files.is_empty() {
            // Must open a file first
            (any::<String>(), ruby_content())
                .prop_map(|(f, c)| Transition::DidOpen {
                    filename: f,
                    content: c
                })
                .boxed()
        } else {
            prop_oneof![
                // === DOCUMENT LIFECYCLE (Weight: 30%) ===

                // Open new file (10%)
                10 => (any::<String>(), ruby_content())
                    .prop_map(|(f, c)| Transition::DidOpen { filename: f, content: c }),

                // Edit existing file - THE CRITICAL TEST (15%)
                15 => select(open_files.clone())
                    .prop_flat_map(move |filename| {
                        let content = state.files.get(&filename).unwrap().clone();
                        valid_edit_for(&content)
                            .prop_map(move |(range, new_text)| Transition::DidChange {
                                filename: filename.clone(),
                                range,
                                new_text,
                            })
                    }),

                // Save file (2%)
                2 => select(open_files.clone())
                    .prop_map(|f| Transition::DidSave { filename: f }),

                // Close file (3%)
                3 => select(open_files.clone())
                    .prop_map(|f| Transition::DidClose { filename: f }),

                // === NAVIGATION QUERIES (Weight: 20%) ===

                // Go to definition (10%)
                10 => select(open_files.clone())
                    .prop_flat_map(|f| random_position().prop_map(move |p|
                        Transition::Definition { filename: f.clone(), position: p }
                    )),

                // Find references (10%)
                10 => select(open_files.clone())
                    .prop_flat_map(|f| (random_position(), any::<bool>()).prop_map(move |(p, incl)|
                        Transition::References {
                            filename: f.clone(),
                            position: p,
                            include_declaration: incl,
                        }
                    )),

                // === INTELLIGENCE QUERIES (Weight: 25%) ===

                // Completion (10%)
                10 => select(open_files.clone())
                    .prop_flat_map(|f| random_position().prop_map(move |p|
                        Transition::Completion { filename: f.clone(), position: p }
                    )),

                // Hover (5%)
                5 => select(open_files.clone())
                    .prop_flat_map(|f| random_position().prop_map(move |p|
                        Transition::Hover { filename: f.clone(), position: p }
                    )),

                // Inlay hints (5%)
                5 => select(open_files.clone())
                    .prop_flat_map(|f| random_range().prop_map(move |r|
                        Transition::InlayHints { filename: f.clone(), range: r }
                    )),

                // Semantic tokens (5%)
                5 => select(open_files.clone())
                    .prop_map(|f| Transition::SemanticTokens { filename: f }),

                // === DOCUMENT STRUCTURE (Weight: 20%) ===

                // Document symbols (8%)
                8 => select(open_files.clone())
                    .prop_map(|f| Transition::DocumentSymbols { filename: f }),

                // Workspace symbols (4%)
                4 => "[a-zA-Z]{0,10}".prop_map(|q| Transition::WorkspaceSymbols { query: q }),

                // Folding ranges (4%)
                4 => select(open_files.clone())
                    .prop_map(|f| Transition::FoldingRange { filename: f }),

                // Code lens (4%)
                4 => select(open_files.clone())
                    .prop_map(|f| Transition::CodeLens { filename: f }),

                // === FORMATTING (Weight: 5%) ===

                // On-type formatting (5%)
                5 => select(open_files.clone())
                    .prop_flat_map(|f| (random_position(), prop_oneof![
                        Just('\n'),  // newline - triggers end insertion
                        Just('d'),   // 'end' completion
                    ]).prop_map(move |(p, ch)|
                        Transition::OnTypeFormatting {
                            filename: f.clone(),
                            position: p,
                            ch
                        }
                    )),
            ].boxed()
        }
    }

    fn apply(state: Self::State, transition: &Self::Transition) -> Self::State {
        let mut state = state;
        match transition {
            Transition::DidOpen { filename, content } => {
                state.files.insert(filename.clone(), content.clone());
            }
            Transition::DidChange { filename, range, new_text } => {
                if let Some(content) = state.files.get_mut(filename) {
                    *content = apply_edit(content, range, new_text);
                }
            }
            Transition::DidClose { filename } => {
                state.files.remove(filename);
            }
            // Read-only operations don't change state
            _ => {}
        }
        state
    }

    fn preconditions(state: &Self::State, transition: &Self::Transition) -> bool {
        match transition {
            Transition::DidChange { filename, .. } |
            Transition::DidClose { filename } |
            Transition::Hover { filename, .. } |
            Transition::Definition { filename, .. } |
            Transition::DocumentSymbols { filename } => {
                state.files.contains_key(filename)
            }
            Transition::DidOpen { .. } => true,
        }
    }
}
```

### 3.2 The Test

```rust
prop_state_machine! {
    #![proptest_config(ProptestConfig {
        cases: 256,
        max_shrink_iters: 10000,
        ..Default::default()
    })]

    #[test]
    fn lsp_model_correspondence(
        sequential 1..100 => LspModel
    ) {
        // Initialize
        let mut model = LspModel::default();
        let mut lsp = RubyLanguageServer::new();

        // Apply each transition to BOTH model and LSP
        for transition in transitions {
            // Apply to model (only mutations change model state)
            model = LspModel::apply(model, &transition);

            // Apply to real LSP - ALL operations should complete without panic
            match &transition {
                // === Document Lifecycle (Mutations) ===
                Transition::DidOpen { filename, content } => {
                    lsp.handle_did_open(filename, content);
                }
                Transition::DidChange { filename, range, new_text } => {
                    lsp.handle_did_change(filename, range, new_text);
                }
                Transition::DidSave { filename } => {
                    lsp.handle_did_save(filename);
                }
                Transition::DidClose { filename } => {
                    lsp.handle_did_close(filename);
                }

                // === Navigation Queries ===
                Transition::Definition { filename, position } => {
                    let _ = lsp.goto_definition(filename, position);
                }
                Transition::References { filename, position, include_declaration } => {
                    let _ = lsp.find_references(filename, position, *include_declaration);
                }

                // === Intelligence Queries ===
                Transition::Completion { filename, position } => {
                    let _ = lsp.completion(filename, position);
                }
                Transition::CompletionResolve { item } => {
                    let _ = lsp.completion_resolve(item);
                }
                Transition::Hover { filename, position } => {
                    let _ = lsp.hover(filename, position);
                }
                Transition::InlayHints { filename, range } => {
                    let _ = lsp.inlay_hints(filename, range);
                }
                Transition::SemanticTokens { filename } => {
                    let _ = lsp.semantic_tokens(filename);
                }

                // === Document Structure ===
                Transition::DocumentSymbols { filename } => {
                    let _ = lsp.document_symbols(filename);
                }
                Transition::WorkspaceSymbols { query } => {
                    let _ = lsp.workspace_symbols(query);
                }
                Transition::FoldingRange { filename } => {
                    let _ = lsp.folding_range(filename);
                }
                Transition::CodeLens { filename } => {
                    let _ = lsp.code_lens(filename);
                }

                // === Formatting ===
                Transition::OnTypeFormatting { filename, position, ch } => {
                    let _ = lsp.on_type_formatting(filename, position, *ch);
                }
            }

            // CRITICAL INVARIANT: Text synchronization (after every operation)
            for (filename, model_content) in &model.files {
                let lsp_content = lsp.get_file_content(filename);
                assert_eq!(
                    model_content,
                    &lsp_content,
                    "LSP state diverged from Model for file '{}'!\n\
                     Model: {:?}\n\
                     LSP:   {:?}",
                    filename, model_content, lsp_content
                );
            }
        }
    }
}
```

---

## 4. What to Assert (The Invariants)

We assert **strictly ordered levels** of correctness. If Level 1 fails, Level 2+ are irrelevant.

### Level 1: Basic Safety (The Fuzzer Layer)

| Invariant          | Description                 | What It Catches                           | Operations Affected |
| ------------------ | --------------------------- | ----------------------------------------- | ------------------- |
| **No Panics**      | Server never crashes        | `unwrap()` on None, index out of bounds   | ALL operations      |
| **Text Sync**      | `Model.text == LSP.text`    | Incremental sync bugs, rope/buffer errors | DidOpen, DidChange  |
| **Valid Response** | Response structure is valid | Malformed LSP responses                   | ALL queries         |

```rust
// Level 1: After EVERY operation
assert!(!panicked, "Server crashed on operation {:?}", transition);
assert_eq!(model.files, lsp.files, "Text synchronization failed!");

// For queries: response must be structurally valid
match &transition {
    Transition::Definition { .. } => {
        // If we got locations, they must have valid URIs and ranges
        if let Some(locations) = result {
            for loc in locations {
                assert!(loc.uri.scheme() == "file");
                assert!(loc.range.start.line <= loc.range.end.line);
            }
        }
    }
    Transition::Completion { .. } => {
        // Completion items must have non-empty labels
        for item in result.items {
            assert!(!item.label.is_empty());
        }
    }
    // ... similar for all query types
}
```

**If text sync fails, your parser is looking at the wrong code.** Everything else is meaningless.

### Level 2: Semantic Logic (The Marker Layer)

How do we test "Go to Definition" without writing a second parser? **Construction** — generate code where the answer is known.

#### 2.1 Navigation: Definition & References (Marker Strategy)

```rust
/// Generate Ruby with KNOWN definition and reference positions
fn ruby_with_markers() -> impl Strategy<Value = MarkedRuby> {
    (class_name(), identifier()).prop_map(|(class_name, method_name)| {
        // We CONSTRUCT the code, so we KNOW the positions
        let code = format!(
            "class {class_name}\n\
               def {method_name}\n\
                 nil\n\
               end\n\
             end\n\
             \n\
             {class_name}.new.{method_name}"
        );

        MarkedRuby {
            content: code,
            // Class definition: line 0, starts at "class "
            class_def_pos: Position { line: 0, character: 6 },
            // Method definition: line 1, starts at "def "
            method_def_pos: Position { line: 1, character: 4 },
            // Class reference: line 6, column 0
            class_ref_pos: Position { line: 6, character: 0 },
            // Method reference: line 6, after ".new."
            method_ref_pos: Position { line: 6, character: (class_name.len() + 5) as u32 },
        }
    })
}

proptest! {
    #[test]
    fn goto_definition_finds_class(marked in ruby_with_markers()) {
        let lsp = setup_lsp_with(&marked.content);
        let result = lsp.goto_definition(marked.class_ref_pos);
        assert_eq!(result.unwrap().range.start.line, marked.class_def_pos.line);
    }

    #[test]
    fn goto_definition_finds_method(marked in ruby_with_markers()) {
        let lsp = setup_lsp_with(&marked.content);
        let result = lsp.goto_definition(marked.method_ref_pos);
        assert_eq!(result.unwrap().range.start.line, marked.method_def_pos.line);
    }

    #[test]
    fn find_references_includes_definition(marked in ruby_with_markers()) {
        let lsp = setup_lsp_with(&marked.content);
        let refs = lsp.find_references(marked.class_def_pos, true); // include_declaration=true

        // Should find at least 2: definition + usage
        assert!(refs.len() >= 2, "Expected at least 2 references, got {}", refs.len());
    }
}
```

#### 2.2 Document Symbols: Completeness

```rust
/// Generate Ruby with N known symbols, verify LSP finds exactly N
fn ruby_with_n_classes(n: usize) -> impl Strategy<Value = (String, Vec<String>)> {
    prop::collection::vec(class_name(), n)
        .prop_map(|names| {
            let code = names.iter()
                .map(|n| format!("class {}\nend", n))
                .collect::<Vec<_>>()
                .join("\n\n");
            (code, names)
        })
}

proptest! {
    #[test]
    fn document_symbols_finds_all_classes(
        (code, expected_names) in ruby_with_n_classes(1..10)
    ) {
        let lsp = setup_lsp_with(&code);
        let symbols = lsp.document_symbols();

        let found_names: HashSet<_> = symbols.iter().map(|s| &s.name).collect();
        for name in &expected_names {
            assert!(found_names.contains(name),
                "LSP missed class '{}' in code:\n{}", name, code);
        }
    }
}
```

#### 2.3 Inlay Hints: Completeness for `end` Keywords

```rust
/// Generate Ruby with N nested blocks, verify N inlay hints
fn ruby_with_n_ends(n: usize) -> impl Strategy<Value = (String, usize)> {
    prop::collection::vec(class_name(), n)
        .prop_map(|names| {
            // Nested classes = N `end` keywords
            let mut code = String::new();
            for name in &names {
                code.push_str(&format!("class {}\n", name));
            }
            for _ in 0..names.len() {
                code.push_str("end\n");
            }
            (code, names.len())
        })
}

proptest! {
    #[test]
    fn inlay_hints_marks_all_ends(
        (code, expected_count) in ruby_with_n_ends(1..5)
    ) {
        let lsp = setup_lsp_with(&code);
        let hints = lsp.inlay_hints(full_range(&code));

        assert_eq!(hints.len(), expected_count,
            "Expected {} inlay hints for {} `end` keywords", expected_count, expected_count);
    }
}
```

#### 2.4 Semantic Tokens: Determinism

```rust
proptest! {
    #[test]
    fn semantic_tokens_deterministic(code in ruby_content()) {
        let lsp = setup_lsp_with(&code);

        let tokens1 = lsp.semantic_tokens();
        let tokens2 = lsp.semantic_tokens();

        assert_eq!(tokens1, tokens2, "Semantic tokens not deterministic!");
    }
}
```

#### 2.5 Folding Ranges: Completeness

```rust
/// Generate Ruby with N foldable regions
fn ruby_with_n_foldable(n: usize) -> impl Strategy<Value = (String, usize)> {
    prop::collection::vec(class_name(), n)
        .prop_map(|names| {
            let code = names.iter()
                .map(|n| format!("class {}\n  # body\nend", n))
                .collect::<Vec<_>>()
                .join("\n\n");
            (code, names.len())
        })
}

proptest! {
    #[test]
    fn folding_ranges_finds_all_classes(
        (code, expected_count) in ruby_with_n_foldable(1..5)
    ) {
        let lsp = setup_lsp_with(&code);
        let ranges = lsp.folding_range();

        assert!(ranges.len() >= expected_count,
            "Expected at least {} folding ranges, got {}", expected_count, ranges.len());
    }
}
```

#### 2.6 Code Lens: Mixin Counts

```rust
/// Generate module with N includes
fn ruby_with_n_includes(n: usize) -> impl Strategy<Value = (String, usize)> {
    (class_name(), prop::collection::vec(class_name(), n))
        .prop_map(|(module_name, included_in)| {
            let mut code = format!("module {}\nend\n\n", module_name);
            for class in &included_in {
                code.push_str(&format!("class {}\n  include {}\nend\n\n", class, module_name));
            }
            (code, included_in.len())
        })
}

proptest! {
    #[test]
    fn code_lens_counts_includes(
        (code, expected_count) in ruby_with_n_includes(1..5)
    ) {
        let lsp = setup_lsp_with(&code);
        let lenses = lsp.code_lens();

        // Find the lens for the module
        let module_lens = lenses.iter().find(|l| l.command.as_ref()
            .map(|c| c.title.contains("include"))
            .unwrap_or(false));

        if expected_count > 0 {
            assert!(module_lens.is_some(), "Expected code lens for module with includes");
        }
    }
}
```

#### 2.7 Completion: Contains Expected Items

```rust
/// Generate Ruby where we know what completions should appear
fn ruby_with_known_completions() -> impl Strategy<Value = (String, Position, Vec<String>)> {
    (class_name(), identifier()).prop_map(|(class_name, var_name)| {
        let code = format!(
            "class {class_name}\n\
               def initialize\n\
                 @{var_name} = 1\n\
               end\n\
               def foo\n\
                 @\n\
               end\n\
             end"
        );
        // Position after "@" on line 5
        let pos = Position { line: 5, character: 5 };
        // Should complete with @{var_name}
        let expected = vec![format!("@{}", var_name)];
        (code, pos, expected)
    })
}

proptest! {
    #[test]
    fn completion_includes_instance_vars(
        (code, pos, expected) in ruby_with_known_completions()
    ) {
        let lsp = setup_lsp_with(&code);
        let completions = lsp.completion(pos);

        let labels: HashSet<_> = completions.items.iter().map(|i| &i.label).collect();
        for exp in &expected {
            assert!(labels.iter().any(|l| l.contains(exp)),
                "Expected completion containing '{}', got {:?}", exp, labels);
        }
    }
}
```

### Level 3: Stability (The Formatting Layer)

| Property          | Assertion                        | What It Catches              |
| ----------------- | -------------------------------- | ---------------------------- |
| **Idempotency**   | `Format(Format(x)) == Format(x)` | Formatting instability       |
| **AST Stability** | `AST(Format(x)) == AST(x)`       | Formatting changes semantics |

```rust
proptest! {
    #[test]
    fn formatting_is_idempotent(code in ruby_content()) {
        let formatted_once = lsp.format(&code);
        let formatted_twice = lsp.format(&formatted_once);

        assert_eq!(formatted_once, formatted_twice,
            "Formatting is not idempotent!\n\
             Original:      {:?}\n\
             After 1 pass:  {:?}\n\
             After 2 passes: {:?}",
            code, formatted_once, formatted_twice);
    }

    #[test]
    fn formatting_preserves_semantics(code in valid_ruby()) {
        let formatted = lsp.format(&code);

        let original_ast = parse(&code);
        let formatted_ast = parse(&formatted);

        assert_eq!(original_ast, formatted_ast,
            "Formatting changed the AST!\n\
             Original code: {:?}\n\
             Formatted:     {:?}",
            code, formatted);
    }
}
```

---

## 5. Ruby Content Generators ✅

The generators produce **rich, diverse Ruby code** to exercise all LSP features.

### 5.1 Coverage Matrix

| Category       | Generators                                                                                       | What They Produce                                              |
| -------------- | ------------------------------------------------------------------------------------------------ | -------------------------------------------------------------- |
| **Methods**    | `ruby_instance_method`, `ruby_class_method`, `ruby_method_with_visibility`, `ruby_attr_accessor` | `def foo`, `def self.foo`, `private def foo`, `attr_reader :x` |
| **Variables**  | `ruby_instance_var`, `ruby_class_var`, `ruby_constant`                                           | `@foo = 1`, `@@count = 0`, `LIMIT = 42`                        |
| **Mixins**     | `ruby_include`, `ruby_extend`, `ruby_prepend`                                                    | `include Foo`, `extend Bar`, `prepend Baz`                     |
| **Structures** | `ruby_class`, `ruby_module`, `ruby_nested_class`, `ruby_nested_modules`                          | Classes, modules, nesting                                      |
| **Complex**    | `ruby_singleton_class`, `ruby_class_with_initialize`, `ruby_mixin_hierarchy`                     | `class << self`, constructors, mixin trees                     |

### 5.2 Example Generated Code

```ruby
# === ruby_class() - Class with inheritance, mixins, class vars, constants ===
class Foo < Bar
  include Baz
  @@counter = 0
  LIMIT = 42

  def self.create(name)
    nil
  end

  private
  def internal_method
    nil
  end

  attr_reader :name, :age
end

# === ruby_nested_class() - Module with nested class ===
module Services
  class UserService
    def call(user)
      nil
    end
  end
end

# === ruby_nested_modules() - Deep nesting (A::B::C style) ===
module Api
  module V1
    module Users
      def index
        nil
      end
    end
  end
end

# === ruby_singleton_class() - class << self ===
class Config
  class << self
    def load
      nil
    end
  end
end

# === ruby_class_with_initialize() - Constructor with ivars ===
class User
  def initialize(name, email)
    @name = name
    @email = email
  end
end

# === ruby_mixin_hierarchy() - Full mixin test setup ===
module Loggable
  def log
    "from Loggable"
  end
end

class Service
  include Loggable
end

class Worker
  extend Loggable
end

class Processor
  prepend Loggable
end
```

### 5.3 Distribution Weights

The main `ruby_content()` generator uses weighted probabilities:

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Content Generation Weights                        │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  Simple Structures (40%)                                            │
│  ├── ruby_class()                          10%                      │
│  └── ruby_module()                         10%                      │
│                                                                     │
│  Nested Structures (25%)                                            │
│  ├── ruby_nested_class()                    5%                      │
│  ├── ruby_nested_modules()                  5%                      │
│  └── ruby_singleton_class()                 5%                      │
│                                                                     │
│  Complex Structures (25%)                                           │
│  ├── ruby_class_with_initialize()           5%                      │
│  ├── ruby_module_with_mixins()              5%                      │
│  ├── ruby_class_with_includes()             5%                      │
│  └── ruby_mixin_hierarchy()                 5%                      │
│                                                                     │
│  Edge Cases (15%)                                                   │
│  ├── Simple assignment                      3%                      │
│  ├── Empty file                             2%                      │
│  ├── Just comments                          2%                      │
│  └── Invalid syntax (error recovery)        3%                      │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### 5.4 Position & Range Generators

```rust
/// Generate random position within document bounds
fn random_position() -> impl Strategy<Value = Position> {
    (0..100u32, 0..200u32).prop_map(|(line, character)| Position { line, character })
}

/// Generate random range (start <= end)
fn random_range() -> impl Strategy<Value = Range> {
    (random_position(), random_position()).prop_map(|(a, b)| {
        if a.line < b.line || (a.line == b.line && a.character <= b.character) {
            Range { start: a, end: b }
        } else {
            Range { start: b, end: a }
        }
    })
}

/// Generate valid position within given content
fn valid_position_for(content: &str) -> impl Strategy<Value = Position> {
    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    let line_count = lines.len().max(1);

    (0..line_count).prop_flat_map(move |line| {
        let line_len = lines.get(line).map(|l| l.len()).unwrap_or(0).max(1);
        (Just(line), 0..=line_len).prop_map(|(l, c)| Position {
            line: l as u32,
            character: c as u32,
        })
    })
}

/// Generate valid edit range and replacement text for given content
fn valid_edit_for(content: &str) -> impl Strategy<Value = (Range, String)> {
    // ... generates valid ranges within document bounds
}
```

### 5.5 Tracked Code Generators (18 scenarios)

The simulation runner uses `tracked_code()` which generates one of 18 scenarios:

| Generator                             | Weight | What It Tests                            |
| ------------------------------------- | ------ | ---------------------------------------- | --- | ---------------- |
| **Structural Tests**                  |        |                                          |
| `tracked_class_with_method_call()`    | 3      | Method calls within a class              |
| `tracked_mixin_method_call()`         | 3      | Method calls through `include`           |
| `tracked_inheritance()`               | 3      | Parent class references in `< Parent`    |
| `tracked_instance_variable()`         | 2      | `@var` definition and references         |
| `tracked_nested_constant()`           | 2      | `A::B::CONST` namespaced access          |
| `tracked_multi_class()`               | 2      | Cross-class references                   |
| `tracked_prepend_override()`          | 2      | `prepend` method resolution order        |
| `tracked_extend()`                    | 2      | Class methods from `extend`              |
| **Complex Mixin Tests**               |        |                                          |
| `tracked_diamond_mixin()`             | 1      | Diamond inheritance (C3 linearization)   |
| `tracked_deep_include_chain()`        | 1      | N-level deep include chains (3-5 levels) |
| `tracked_mixin_counts()`              | 1      | Include/extend/prepend counting          |
| `tracked_completion_through_mixins()` | 1      | Completion through ancestor chain        |
| `tracked_mixin_edge_cases()`          | 1      | Self-include, circular, missing modules  |
| **Type Inference Tests** (NEW)        |        |                                          |
| `tracked_type_assignments()`          | 2      | String/Integer/Array/Hash type inference |
| `tracked_type_narrowing()`            | 1      | Type narrowing after `                   |     | =`, conditionals |
| `tracked_type_stability()`            | 2      | Type survives unrelated edits            |
| `tracked_method_chain_types()`        | 1      | Type flow through method chains          |
| `tracked_inlay_hints()`               | 2      | Inlay hint type display                  |

### 5.6 What We Now Test

| Ruby Construct                    | Generator                             | Tested? |
| --------------------------------- | ------------------------------------- | ------- |
| Simple classes                    | `ruby_class()`                        | ✅      |
| Class inheritance                 | `ruby_class()`                        | ✅      |
| Simple modules                    | `ruby_module()`                       | ✅      |
| Instance methods                  | `ruby_instance_method()`              | ✅      |
| Class methods (`def self.foo`)    | `ruby_class_method()`                 | ✅      |
| Visibility modifiers              | `ruby_method_with_visibility()`       | ✅      |
| `attr_reader/writer/accessor`     | `ruby_attr_accessor()`                | ✅      |
| Instance variables (`@foo`)       | `ruby_instance_var()`                 | ✅      |
| Class variables (`@@foo`)         | `ruby_class_var()`                    | ✅      |
| Constants (`FOO = 1`)             | `ruby_constant()`                     | ✅      |
| `include`                         | `ruby_include()`                      | ✅      |
| `extend`                          | `ruby_extend()`                       | ✅      |
| `prepend`                         | `ruby_prepend()`                      | ✅      |
| Nested classes                    | `ruby_nested_class()`                 | ✅      |
| Deep module nesting               | `ruby_nested_modules()`               | ✅      |
| Singleton class (`class << self`) | `ruby_singleton_class()`              | ✅      |
| Constructor with ivars            | `ruby_class_with_initialize()`        | ✅      |
| Mixin hierarchies                 | `ruby_mixin_hierarchy()`              | ✅      |
| Invalid syntax (error recovery)   | `ruby_content()`                      | ✅      |
| Diamond inheritance               | `tracked_diamond_mixin()`             | ✅      |
| Deep include chains (N levels)    | `tracked_deep_include_chain()`        | ✅      |
| Include/extend/prepend counts     | `tracked_mixin_counts()`              | ✅      |
| Completion through mixins         | `tracked_completion_through_mixins()` | ✅      |
| Self-include edge case            | `tracked_mixin_edge_cases()`          | ✅      |
| Circular include edge case        | `tracked_mixin_edge_cases()`          | ✅      |
| Missing module edge case          | `tracked_mixin_edge_cases()`          | ✅      |
| Deep namespace (A::B::C)          | `tracked_mixin_edge_cases()`          | ✅      |
| String/Integer/Array/Hash types   | `tracked_type_assignments()`          | ✅      |
| Type propagation                  | `tracked_type_assignments()`          | ✅      |
| Type narrowing (`\|\|=`)          | `tracked_type_narrowing()`            | ✅      |
| Type stability across edits       | `tracked_type_stability()`            | ✅      |
| Method chain types                | `tracked_method_chain_types()`        | ✅      |
| Inlay hint types                  | `tracked_inlay_hints()`               | ✅      |

### 5.7 What We Don't Yet Test

| Ruby Construct            | Status     |
| ------------------------- | ---------- | ----- | ---------- |
| Blocks (`{                | x          | x }`) | ❌ Not yet |
| Procs/Lambdas             | ❌ Not yet |
| `if`/`unless`/`case`      | ❌ Not yet |
| `begin`/`rescue`/`ensure` | ❌ Not yet |
| Method aliases            | ❌ Not yet |
| Refinements               | ❌ Not yet |
| `method_missing`          | ❌ Not yet |

---

## 6. Seed Management & Workflow

### 6.1 The Workflow

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Development Workflow                         │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  1. Run: cargo test simulation                                      │
│     └── proptest fuzzes 100s of "trees" (sequences)                │
│                                                                     │
│  2. If PASS: Great! Confidence increased.                          │
│                                                                     │
│  3. If FAIL:                                                        │
│     └── proptest prints:                                           │
│         - Seed: 0x1234567890abcdef                                 │
│         - Minimal failing steps (after shrinking)                  │
│                                                                     │
│  4. Debug:                                                          │
│     └── PROPTEST_SEED=0x1234567890abcdef cargo test <test_name>   │
│                                                                     │
│  5. Fix the bug                                                     │
│                                                                     │
│  6. Document: Add seed to seeds.toml (optional, for notable bugs)  │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

### 6.2 Seed Registry

Create `src/test/simulation/seeds.toml` for documenting interesting failures:

```toml
# Seed Registry for Deterministic Simulation Testing
#
# Each entry documents a seed that reproduces an interesting scenario.
# Run with: PROPTEST_SEED=<seed> cargo test <test_name>

[[seeds]]
seed = "0x1234567890abcdef"
test = "lsp_model_correspondence"
description = "Text sync fails when DidChange spans multiple lines"
found_date = "2024-01-15"
status = "fixed"  # fixed | open | wontfix
issue = "https://github.com/..."
minimal_steps = """
1. DidOpen("a.rb", "class Foo\nend")
2. DidChange("a.rb", Range(0:6..1:0), "Bar\n  def x")
3. Assert: Model has "class Bar\n  def x\nend", LSP has "class Barend"
"""

[[seeds]]
seed = "0xdeadbeef12345678"
test = "goto_definition_finds_markers"
description = "Definition lookup fails with nested modules"
found_date = "2024-01-20"
status = "open"
```

### 6.3 Seed Stability: What Happens When the Model Changes?

**Short answer**: Seeds become invalid when the model changes. This is expected and manageable.

#### Why Seeds Break

A seed is just a number that initializes the PRNG. The PRNG produces a deterministic sequence:

```
Seed: 0x1234 → [0.73, 0.12, 0.89, 0.45, ...]
```

The **model** interprets this sequence:

```rust
// With Model v1:
0.73 → DidOpen("file_abc.rb", "class Xyz...")
0.12 → DidChange(range: 0:5..0:10, "new text")
0.89 → Definition(line: 3, char: 7)

// With Model v2 (added new transition):
0.73 → DidOpen("file_abc.rb", "class Xyz...")
0.12 → DidSave("file_abc.rb")  // NEW! Different interpretation
0.89 → DidChange(...)          // Everything shifts!
```

**Same seed, completely different test sequence.**

#### When Seeds Break

| Change                           | Seeds Valid? | Why                                |
| -------------------------------- | ------------ | ---------------------------------- |
| Bug fix in LSP (no model change) | ✅ Yes       | Same sequence, different behavior  |
| Add new assertion                | ✅ Yes       | Same sequence, more checks         |
| Add new `Transition` variant     | ❌ No        | `prop_oneof!` distribution changes |
| Change transition weights        | ❌ No        | Selection probabilities change     |
| Change Ruby generator            | ❌ No        | Generated content changes          |
| Rename fields                    | ✅ Yes       | Internal only, PRNG unchanged      |

#### Mitigation Strategies

##### Strategy 1: Version the Model (Recommended)

```toml
# seeds.toml
[[seeds]]
seed = "0x1234567890abcdef"
model_version = "v1"  # <-- Track which model version
test = "lsp_model_correspondence"
description = "Text sync fails on multi-line edit"
```

```rust
const MODEL_VERSION: &str = "v1";

#[test]
fn run_registered_seeds() {
    for seed in load_seeds() {
        if seed.model_version != MODEL_VERSION {
            println!("SKIP: Seed {} is for model {}, current is {}",
                seed.seed, seed.model_version, MODEL_VERSION);
            continue;
        }
        // Run test with seed
    }
}
```

When you change the model:

1. Bump `MODEL_VERSION` to `"v2"`
2. Old seeds are skipped (not deleted - they document history)
3. New seeds use `"v2"`

##### Strategy 2: Snapshot the Sequence (For Critical Bugs)

For really important bugs, store the **actual sequence**, not just the seed:

```toml
[[seeds]]
seed = "0x1234567890abcdef"
model_version = "v1"
# Store the actual operations for posterity
sequence = """
1. DidOpen("a.rb", "class Foo\\nend")
2. DidChange("a.rb", 0:6..0:6, "Bar")
3. Definition("a.rb", 1:0)
"""
# This can be converted to a unit test if the seed breaks
```

When the model changes:

```rust
// Convert to explicit unit test
#[test]
fn regression_text_sync_multiline() {
    let mut lsp = setup();
    lsp.did_open("a.rb", "class Foo\nend");
    lsp.did_change("a.rb", Range::new(0, 6, 0, 6), "Bar");
    assert_eq!(lsp.get_content("a.rb"), "class FooBar\nend");
}
```

##### Strategy 3: Separate Seed Registries per Model Version

```
src/test/simulation/
├── seeds_v1.toml  # Old model
├── seeds_v2.toml  # Current model
└── seeds.toml     # Symlink to current
```

##### Strategy 4: Regression Test Extraction (Best Practice)

When a seed finds a bug:

1. **Immediately** extract the minimal failing sequence
2. Write it as a **deterministic unit test** (no randomness)
3. Store the seed in registry for documentation only

```rust
// This test doesn't depend on the model - it's explicit
#[test]
fn regression_issue_123_multiline_edit() {
    // Found by seed 0x1234 on 2024-01-15
    let mut harness = TestHarness::new();
    harness.did_open("test.rb", "class Foo\n  def bar\n  end\nend");
    harness.did_change("test.rb", Range::new(1, 2, 2, 5), "def baz\n    nil\n  ");

    assert_eq!(
        harness.get_content("test.rb"),
        "class Foo\n  def baz\n    nil\n  end\nend"
    );
}
```

**This test survives model changes forever.**

#### Recommended Workflow

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Seed Lifecycle                                    │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│  1. Proptest finds failure                                          │
│     └── Seed: 0x1234, Model: v1                                     │
│                                                                      │
│  2. Extract minimal sequence (proptest shrinking helps)             │
│     └── DidOpen → DidChange → Assert fails                          │
│                                                                      │
│  3. Write EXPLICIT regression test (no seed dependency)             │
│     └── fn regression_issue_123() { ... }                           │
│                                                                      │
│  4. Store seed in registry (documentation only)                     │
│     └── seeds.toml: seed, model_version, description, sequence      │
│                                                                      │
│  5. Fix the bug                                                     │
│                                                                      │
│  6. When model changes to v2:                                       │
│     └── Old seeds skipped (version mismatch)                        │
│     └── Regression tests still run (explicit, no seed)              │
│     └── New fuzzing finds new bugs with v2 seeds                    │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

#### Summary

| Approach                  | Pros                           | Cons                |
| ------------------------- | ------------------------------ | ------------------- |
| **Version the model**     | Simple, seeds still documented | Old seeds don't run |
| **Snapshot sequences**    | Human-readable history         | Manual extraction   |
| **Extract to unit tests** | Survives forever, explicit     | More test code      |
| **Separate registries**   | Clean separation               | File management     |

**Best practice**: Always extract important bugs to explicit regression tests. Seeds are for **discovery**, unit tests are for **prevention**.

---

### 6.4 CI Integration

```yaml
# .github/workflows/simulation-tests.yml
name: Simulation Tests

on:
  push:
    branches: [main]
  pull_request:
  schedule:
    - cron: "0 2 * * *" # Nightly with more iterations

jobs:
  simulation:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-action@stable

      - name: Run simulation tests (PR - quick)
        if: github.event_name == 'pull_request'
        run: cargo test simulation --release -- --test-threads=1
        env:
          PROPTEST_CASES: 50

      - name: Run simulation tests (Nightly - thorough)
        if: github.event_name == 'schedule'
        run: cargo test simulation --release -- --test-threads=1
        env:
          PROPTEST_CASES: 1000

      - name: Archive failure seeds
        if: failure()
        uses: actions/upload-artifact@v4
        with:
          name: failing-seeds
          path: proptest-regressions/
```

---

## 7. Implementation Roadmap

### Phase 1: Foundation (Week 1)

**Goal**: Get the simplest possible simulation test running.

```bash
# Step 1: Add dependencies
cargo add --dev proptest proptest-state-machine
```

**Files to create**:

```
src/test/simulation/
├── mod.rs              # Module exports
├── model.rs            # LspModel (just HashMap<String, String>)
├── transitions.rs      # Transition enum (start with DidOpen, DidClose only)
└── tests.rs            # First proptest
```

**Minimal First Test** (`src/test/simulation/tests.rs`):

```rust
use proptest::prelude::*;
use proptest_state_machine::{prop_state_machine, ReferenceStateMachine};
use std::collections::HashMap;

#[derive(Clone, Debug, Default)]
struct LspModel {
    files: HashMap<String, String>,
}

#[derive(Clone, Debug)]
enum Transition {
    DidOpen { filename: String, content: String },
    DidClose { filename: String },
}

impl ReferenceStateMachine for LspModel {
    type State = Self;
    type Transition = Transition;

    fn init_state() -> BoxedStrategy<Self::State> {
        Just(LspModel::default()).boxed()
    }

    fn transitions(state: &Self::State) -> BoxedStrategy<Self::Transition> {
        let files: Vec<String> = state.files.keys().cloned().collect();
        if files.is_empty() {
            ("[a-z]{1,10}\\.rb", "[a-z \\n]{0,100}")
                .prop_map(|(f, c)| Transition::DidOpen { filename: f, content: c })
                .boxed()
        } else {
            prop_oneof![
                ("[a-z]{1,10}\\.rb", "[a-z \\n]{0,100}")
                    .prop_map(|(f, c)| Transition::DidOpen { filename: f, content: c }),
                prop::sample::select(files)
                    .prop_map(|f| Transition::DidClose { filename: f }),
            ].boxed()
        }
    }

    fn apply(mut state: Self::State, transition: &Self::Transition) -> Self::State {
        match transition {
            Transition::DidOpen { filename, content } => {
                state.files.insert(filename.clone(), content.clone());
            }
            Transition::DidClose { filename } => {
                state.files.remove(filename);
            }
        }
        state
    }

    fn preconditions(state: &Self::State, transition: &Self::Transition) -> bool {
        match transition {
            Transition::DidClose { filename } => state.files.contains_key(filename),
            _ => true,
        }
    }
}

prop_state_machine! {
    #[test]
    fn lsp_text_sync(sequential 1..50 => LspModel);
}

// The actual test implementation
impl LspModel {
    fn test_sequential(self, transition: Transition) -> Self {
        // TODO: Replace with real LSP server
        let mut lsp_files: HashMap<String, String> = HashMap::new();

        match &transition {
            Transition::DidOpen { filename, content } => {
                lsp_files.insert(filename.clone(), content.clone());
            }
            Transition::DidClose { filename } => {
                lsp_files.remove(filename);
            }
        }

        let new_state = Self::apply(self, &transition);

        // CRITICAL ASSERTION: Model == LSP
        assert_eq!(new_state.files, lsp_files, "Text sync failed!");

        new_state
    }
}
```

**Run it**:

```bash
cargo test simulation -- --nocapture
```

**Deliverable**: A test that opens/closes files and verifies text sync. If this works, the foundation is solid.

---

### Phase 2: Edit Testing (Week 2)

**Goal**: Test `DidChange` - the most critical operation.

**Add to `transitions.rs`**:

```rust
Transition::DidChange { filename: String, range: Range, new_text: String }
```

**Key challenge**: Generate valid ranges within document bounds.

```rust
fn valid_edit_for(content: &str) -> impl Strategy<Value = (Range, String)> {
    let line_count = content.lines().count().max(1);

    (0..line_count, 0..line_count).prop_flat_map(move |(sl, el)| {
        let (start_line, end_line) = (sl.min(el), sl.max(el));
        // ... generate valid character positions
    })
}
```

**Deliverable**: Rapid edits don't break text synchronization.

---

### Phase 3: Query Testing (Week 3)

**Goal**: Add read-only queries with Level 1 assertions (no crash).

**Add transitions**:

- `Definition`, `References`, `Completion`, `Hover`
- `DocumentSymbols`, `FoldingRange`, `SemanticTokens`
- `InlayHints`, `CodeLens`, `WorkspaceSymbols`

**Assertions**: Just verify no panic. Don't assert on results yet.

```rust
Transition::Definition { filename, position } => {
    let _ = lsp.goto_definition(&filename, position); // No panic = pass
}
```

**Deliverable**: Server survives any sequence of queries.

---

### Phase 4: Marker Strategy (Week 4)

**Goal**: Add Level 2 assertions using constructed code.

**Create marker generators**:

```rust
fn ruby_with_class_marker() -> impl Strategy<Value = MarkedRuby> {
    class_name().prop_map(|name| {
        let code = format!("class {}\nend\n\n{}.new", name, name);
        MarkedRuby {
            content: code,
            def_line: 0,
            ref_line: 3,
        }
    })
}
```

**Separate test for markers**:

```rust
proptest! {
    #[test]
    fn goto_definition_correct(marked in ruby_with_class_marker()) {
        let lsp = setup_with(&marked.content);
        let result = lsp.goto_definition(Position { line: marked.ref_line, character: 0 });
        assert_eq!(result.unwrap().range.start.line, marked.def_line);
    }
}
```

**Deliverable**: Goto definition works for constructed cases.

---

### Phase 5: CI & Polish (Week 5)

**Goal**: Production-ready simulation testing.

1. **CI Integration**:

   ```yaml
   - name: Simulation tests (quick)
     run: cargo test simulation --release
     env:
       PROPTEST_CASES: 50
   ```

2. **Seed Registry**: Create `seeds.toml` for documenting interesting failures.

3. **Nightly runs**: More iterations to find rare bugs.

**Deliverable**: Simulation tests run on every PR.

---

## 8. Verification: How Do We Know Results Are Correct?

### The Verification Pyramid

```
                    ┌─────────────────┐
                    │   Level 3       │  Mathematical properties
                    │  (Idempotency)  │  f(f(x)) == f(x)
                    └────────┬────────┘
                             │
                    ┌────────▼────────┐
                    │   Level 2       │  Constructed inputs
                    │   (Markers)     │  We KNOW the answer
                    └────────┬────────┘
                             │
         ┌───────────────────▼────────────────────┐
         │              Level 1                   │  Objective truth
         │  (No Crash, Text Sync, Valid Response) │  Model is trivially correct
         └────────────────────────────────────────┘
```

### Level 1: Objective Truth (Always Verifiable)

| What             | Why It's Objectively Correct                                 |
| ---------------- | ------------------------------------------------------------ |
| **No panic**     | Binary: crashed or didn't crash                              |
| **Text sync**    | Model is `HashMap<String, String>` - impossible to get wrong |
| **Valid ranges** | `start.line <= end.line` is mathematical fact                |
| **Valid URIs**   | `file://` scheme is spec requirement                         |

### Level 2: Constructed Truth (We Built the Answer)

We don't ask "is goto-definition correct for arbitrary code?"

We ask "for code WE GENERATED with KNOWN structure, does it find what we put there?"

```rust
// We CONSTRUCT this, so we KNOW line 0 has the definition
let code = "class Foo\nend\n\nFoo.new";
//          ^-- def at 0    ^-- ref at 3

// If LSP says definition is at line 5, it's OBJECTIVELY WRONG
```

### Level 3: Mathematical Properties (Provably Correct)

| Property          | Mathematical Basis                                     |
| ----------------- | ------------------------------------------------------ |
| **Idempotency**   | `f(f(x)) = f(x)` - applying twice equals applying once |
| **Determinism**   | Same input → same output (pure function property)      |
| **Commutativity** | Order shouldn't matter for some operations             |

### What We DON'T Assert (Ambiguous Cases)

```rust
// ❌ DON'T: Assert exact completion count
assert_eq!(completions.len(), 5); // Who says 5 is right?

// ✅ DO: Assert completion INCLUDES what we defined
assert!(completions.iter().any(|c| c.label == "@my_var"));

// ❌ DON'T: Assert hover returns specific text
assert_eq!(hover.contents, "Returns Integer"); // Wording can vary

// ✅ DO: Assert hover doesn't crash and returns valid markdown
assert!(!hover.contents.is_empty());
```

### Decision Tree: Should We Assert This?

```
Can we construct input where we KNOW the answer?
    │
    ├─ YES → Level 2 assertion (Marker strategy)
    │
    └─ NO → Is it a mathematical property?
              │
              ├─ YES → Level 3 assertion (Idempotency, determinism)
              │
              └─ NO → Is it objectively verifiable?
                        │
                        ├─ YES → Level 1 assertion (No crash, valid structure)
                        │
                        └─ NO → DON'T ASSERT (fuzzy/subjective)
```

---

## 9. Testing Complex Mixin Trees (include/extend/prepend)

Mixin resolution is one of the most complex parts of Ruby semantics. The LSP must correctly traverse the ancestor chain for:

- **Goto Definition**: Find methods from included modules
- **Completion**: Suggest methods from ancestor chain
- **CodeLens**: Count include/extend/prepend usages accurately

### 9.1 The Ancestor Chain Model

Ruby's method lookup order (which we must match):

```
For instance methods:
  prepends (reverse order) → self → includes (reverse order) → superclass → ...

For class methods:
  extends (reverse order) → singleton_class → Class → Module → Object → BasicObject
```

### 9.2 Constructing Mixin Test Cases

The key insight: **We generate the mixin tree, so we KNOW the expected ancestor chain.**

```rust
/// A constructed mixin hierarchy with known ancestor chain
#[derive(Debug, Clone)]
struct MixinHierarchy {
    code: String,
    /// Map from class/module name to its expected ancestor chain
    expected_chains: HashMap<String, Vec<String>>,
    /// Map from method name to which class/module defines it
    method_origins: HashMap<String, String>,
    /// Position markers for testing
    markers: Vec<Marker>,
}

#[derive(Debug, Clone)]
struct Marker {
    name: String,
    position: Position,
    expected_definition_in: String,  // Which module/class should definition be in
}
```

### 9.3 Simple Mixin Generators

#### Include Chain

```rust
/// Generate: module M1 with method, class C1 includes M1
fn simple_include() -> impl Strategy<Value = MixinHierarchy> {
    (class_name(), class_name(), identifier()).prop_map(|(mod_name, class_name, method_name)| {
        let code = format!(r#"
module {mod_name}
  def {method_name}
    "from module"
  end
end

class {class_name}
  include {mod_name}
end

{class_name}.new.{method_name}
"#);

        MixinHierarchy {
            code,
            expected_chains: hashmap! {
                class_name.clone() => vec![class_name.clone(), mod_name.clone()],
            },
            method_origins: hashmap! {
                method_name.clone() => mod_name.clone(),
            },
            markers: vec![Marker {
                name: method_name,
                position: Position { line: 11, character: class_name.len() as u32 + 5 },
                expected_definition_in: mod_name,
            }],
        }
    })
}

proptest! {
    #[test]
    fn goto_definition_through_include(hierarchy in simple_include()) {
        let lsp = setup_lsp_with(&hierarchy.code);

        for marker in &hierarchy.markers {
            let result = lsp.goto_definition(marker.position);

            // Definition should be in the expected module
            let def_location = result.expect("Should find definition");
            let def_content = lsp.get_line_at(def_location.range.start.line);

            assert!(def_content.contains(&format!("def {}", marker.name)),
                "Expected definition of '{}' in '{}', but got line: {}",
                marker.name, marker.expected_definition_in, def_content);
        }
    }
}
```

#### Prepend (Method Override)

```rust
/// Generate: module M1 prepended to class C1, both define same method
/// Prepend should win!
fn prepend_override() -> impl Strategy<Value = MixinHierarchy> {
    (class_name(), class_name(), identifier()).prop_map(|(mod_name, class_name, method_name)| {
        let code = format!(r#"
module {mod_name}
  def {method_name}
    "from prepended module"  # LINE 2 - This should be found
  end
end

class {class_name}
  prepend {mod_name}

  def {method_name}
    "from class"  # LINE 10 - This should NOT be found (prepend wins)
  end
end

{class_name}.new.{method_name}
"#);

        MixinHierarchy {
            code,
            expected_chains: hashmap! {
                // Prepend comes BEFORE class in chain
                class_name.clone() => vec![mod_name.clone(), class_name.clone()],
            },
            method_origins: hashmap! {
                // Method should resolve to prepended module
                method_name.clone() => mod_name.clone(),
            },
            markers: vec![Marker {
                name: method_name,
                position: Position { line: 16, character: class_name.len() as u32 + 5 },
                expected_definition_in: mod_name, // NOT class_name!
            }],
        }
    })
}

proptest! {
    #[test]
    fn prepend_overrides_class_method(hierarchy in prepend_override()) {
        let lsp = setup_lsp_with(&hierarchy.code);

        for marker in &hierarchy.markers {
            let result = lsp.goto_definition(marker.position);
            let def_location = result.expect("Should find definition");

            // CRITICAL: Definition should be in the PREPENDED module, not the class
            // This tests that prepend is correctly ordered before the class
            assert!(def_location.range.start.line < 5,
                "Prepend should override class method! Expected line < 5, got {}",
                def_location.range.start.line);
        }
    }
}
```

#### Extend (Class Methods)

```rust
/// Generate: module M1 extended into class C1
/// M1's instance methods become C1's class methods
fn extend_class_methods() -> impl Strategy<Value = MixinHierarchy> {
    (class_name(), class_name(), identifier()).prop_map(|(mod_name, class_name, method_name)| {
        let code = format!(r#"
module {mod_name}
  def {method_name}
    "class method via extend"
  end
end

class {class_name}
  extend {mod_name}
end

{class_name}.{method_name}
"#);

        MixinHierarchy {
            code,
            expected_chains: hashmap! {
                class_name.clone() => vec![class_name.clone()],
            },
            method_origins: hashmap! {
                method_name.clone() => mod_name.clone(),
            },
            markers: vec![Marker {
                name: method_name,
                position: Position { line: 11, character: class_name.len() as u32 + 1 },
                expected_definition_in: mod_name,
            }],
        }
    })
}
```

### 9.4 Complex Mixin Trees (Diamond, Deep Nesting)

#### Diamond Problem

```rust
/// Generate diamond inheritance:
///       M_base
///      /      \
///   M_left   M_right
///      \      /
///       C_final
fn diamond_mixin() -> impl Strategy<Value = MixinHierarchy> {
    (
        class_name(), // base module
        class_name(), // left module
        class_name(), // right module
        class_name(), // final class
        identifier(), // shared method name
    ).prop_map(|(base, left, right, final_class, method)| {
        let code = format!(r#"
module {base}
  def {method}
    "from base"
  end
end

module {left}
  include {base}
  def method_left; end
end

module {right}
  include {base}
  def method_right; end
end

class {final_class}
  include {left}
  include {right}  # Right included LAST, so it comes first in chain
end

{final_class}.new.{method}
"#);

        MixinHierarchy {
            code,
            // Ruby's linearization: final -> right -> left -> base
            // (right comes before left because it was included last)
            expected_chains: hashmap! {
                final_class.clone() => vec![
                    final_class.clone(),
                    right.clone(),
                    left.clone(),
                    base.clone(),
                ],
            },
            method_origins: hashmap! {
                method.clone() => base.clone(),
            },
            markers: vec![Marker {
                name: method,
                position: Position { line: 24, character: final_class.len() as u32 + 5 },
                expected_definition_in: base,
            }],
        }
    })
}
```

#### Deep Nesting (N levels)

```rust
/// Generate N-level deep include chain
fn deep_include_chain(depth: usize) -> impl Strategy<Value = MixinHierarchy> {
    prop::collection::vec(class_name(), depth)
        .prop_flat_map(move |names| {
            identifier().prop_map(move |method_name| {
                let mut code = String::new();

                // Generate modules from deepest to shallowest
                // M0 (has method) <- M1 <- M2 <- ... <- C_final
                for (i, name) in names.iter().enumerate() {
                    if i == 0 {
                        // Deepest module has the method
                        code.push_str(&format!(
                            "module {}\n  def {}\n    \"from deepest\"\n  end\nend\n\n",
                            name, method_name
                        ));
                    } else {
                        // Each module includes the previous
                        code.push_str(&format!(
                            "module {}\n  include {}\nend\n\n",
                            name, names[i - 1]
                        ));
                    }
                }

                // Final class includes the last module
                let final_class = format!("Final{}", depth);
                code.push_str(&format!(
                    "class {}\n  include {}\nend\n\n",
                    final_class, names.last().unwrap()
                ));

                // Method call
                code.push_str(&format!("{}.new.{}", final_class, method_name));

                MixinHierarchy {
                    code,
                    expected_chains: hashmap! {
                        final_class.clone() => {
                            let mut chain = vec![final_class.clone()];
                            chain.extend(names.iter().rev().cloned());
                            chain
                        },
                    },
                    method_origins: hashmap! {
                        method_name.clone() => names[0].clone(),
                    },
                    markers: vec![],  // Complex to compute positions
                }
            })
        })
}

proptest! {
    #[test]
    fn deep_include_chain_resolves(hierarchy in deep_include_chain(5)) {
        let lsp = setup_lsp_with(&hierarchy.code);

        // Verify ancestor chain matches expected
        for (class_name, expected_chain) in &hierarchy.expected_chains {
            let actual_chain = lsp.get_ancestor_chain(class_name);
            assert_eq!(actual_chain, *expected_chain,
                "Ancestor chain mismatch for {}", class_name);
        }
    }
}
```

### 9.5 CodeLens Mixin Count Testing

```rust
/// Generate module with known include/extend/prepend counts
fn module_with_known_mixin_counts() -> impl Strategy<Value = MixinCountTest> {
    (
        class_name(),                    // module name
        1..5usize,                       // include count
        0..3usize,                       // extend count
        0..2usize,                       // prepend count
    ).prop_flat_map(|(mod_name, includes, extends, prepends)| {
        prop::collection::vec(class_name(), includes + extends + prepends)
            .prop_map(move |class_names| {
                let mut code = format!("module {}\n  def shared_method; end\nend\n\n", mod_name);

                let mut include_count = 0;
                let mut extend_count = 0;
                let mut prepend_count = 0;

                for (i, class_name) in class_names.iter().enumerate() {
                    let mixin_type = if i < includes {
                        include_count += 1;
                        "include"
                    } else if i < includes + extends {
                        extend_count += 1;
                        "extend"
                    } else {
                        prepend_count += 1;
                        "prepend"
                    };

                    code.push_str(&format!(
                        "class {}\n  {} {}\nend\n\n",
                        class_name, mixin_type, mod_name
                    ));
                }

                MixinCountTest {
                    code,
                    module_name: mod_name.clone(),
                    expected_includes: include_count,
                    expected_extends: extend_count,
                    expected_prepends: prepend_count,
                }
            })
    })
}

#[derive(Debug)]
struct MixinCountTest {
    code: String,
    module_name: String,
    expected_includes: usize,
    expected_extends: usize,
    expected_prepends: usize,
}

proptest! {
    #[test]
    fn code_lens_counts_mixins_correctly(test in module_with_known_mixin_counts()) {
        let lsp = setup_lsp_with(&test.code);
        let lenses = lsp.code_lens();

        // Find lenses for our module
        let module_lenses: Vec<_> = lenses.iter()
            .filter(|l| l.range.start.line == 0) // Module is on line 0
            .collect();

        // Check include count
        if test.expected_includes > 0 {
            let include_lens = module_lenses.iter()
                .find(|l| l.command.as_ref().map(|c| c.title.contains("include")).unwrap_or(false));
            assert!(include_lens.is_some(), "Expected include CodeLens");
            let title = &include_lens.unwrap().command.as_ref().unwrap().title;
            assert!(title.starts_with(&test.expected_includes.to_string()),
                "Expected {} includes, got: {}", test.expected_includes, title);
        }

        // Check extend count
        if test.expected_extends > 0 {
            let extend_lens = module_lenses.iter()
                .find(|l| l.command.as_ref().map(|c| c.title.contains("extend")).unwrap_or(false));
            assert!(extend_lens.is_some(), "Expected extend CodeLens");
            let title = &extend_lens.unwrap().command.as_ref().unwrap().title;
            assert!(title.starts_with(&test.expected_extends.to_string()),
                "Expected {} extends, got: {}", test.expected_extends, title);
        }

        // Check prepend count
        if test.expected_prepends > 0 {
            let prepend_lens = module_lenses.iter()
                .find(|l| l.command.as_ref().map(|c| c.title.contains("prepend")).unwrap_or(false));
            assert!(prepend_lens.is_some(), "Expected prepend CodeLens");
            let title = &prepend_lens.unwrap().command.as_ref().unwrap().title;
            assert!(title.starts_with(&test.expected_prepends.to_string()),
                "Expected {} prepends, got: {}", test.expected_prepends, title);
        }
    }
}
```

### 9.6 Completion Through Ancestor Chain

```rust
/// Test that completion includes methods from entire ancestor chain
fn completion_through_mixins() -> impl Strategy<Value = CompletionMixinTest> {
    (class_name(), class_name(), class_name(), identifier(), identifier())
        .prop_map(|(mod1, mod2, class_name, method1, method2)| {
            let code = format!(r#"
module {mod1}
  def {method1}; end
end

module {mod2}
  def {method2}; end
end

class {class_name}
  include {mod1}
  include {mod2}

  def foo
    self.  # Completion here should include method1 and method2
  end
end
"#);

            CompletionMixinTest {
                code,
                completion_position: Position { line: 14, character: 9 },
                expected_methods: vec![method1, method2, "foo".to_string()],
            }
        })
}

proptest! {
    #[test]
    fn completion_includes_mixin_methods(test in completion_through_mixins()) {
        let lsp = setup_lsp_with(&test.code);
        let completions = lsp.completion(test.completion_position);

        let labels: HashSet<_> = completions.items.iter()
            .map(|i| i.label.as_str())
            .collect();

        for expected in &test.expected_methods {
            assert!(labels.contains(expected.as_str()),
                "Expected completion to include '{}' from ancestor chain. Got: {:?}",
                expected, labels);
        }
    }
}
```

### 9.7 Edge Cases to Generate

| Edge Case                | What to Test                                                |
| ------------------------ | ----------------------------------------------------------- |
| **Circular includes**    | `M1 includes M2, M2 includes M1` - should not infinite loop |
| **Self-include**         | `M1 includes M1` - should handle gracefully                 |
| **Missing module**       | `include NonExistent` - should not crash                    |
| **Reopened modules**     | Module defined in multiple files with different includes    |
| **Namespaced mixins**    | `include A::B::C` - namespace resolution                    |
| **Conditional includes** | `include M if condition` - may or may not be indexed        |

```rust
/// Generate edge cases that should not crash
fn mixin_edge_cases() -> impl Strategy<Value = String> {
    prop_oneof![
        // Circular include (should not infinite loop)
        Just("module M1; include M2; end\nmodule M2; include M1; end".to_string()),

        // Self-include
        Just("module M1; include M1; end".to_string()),

        // Missing module
        Just("class C1; include NonExistentModule; end".to_string()),

        // Deeply nested namespace
        Just("module A; module B; module C; end; end; end\nclass X; include A::B::C; end".to_string()),
    ]
}

proptest! {
    #[test]
    fn mixin_edge_cases_dont_crash(code in mixin_edge_cases()) {
        let lsp = setup_lsp_with(&code);

        // These should all complete without panic
        let _ = lsp.document_symbols();
        let _ = lsp.code_lens();
        let _ = lsp.completion(Position { line: 0, character: 0 });
    }
}
```

### 9.8 Summary: Mixin Testing Strategy

| What We Test                   | How We Verify                             | Assertion Level       |
| ------------------------------ | ----------------------------------------- | --------------------- |
| **Ancestor chain order**       | Construct hierarchy, compare chain        | Level 2 (Constructed) |
| **Prepend overrides class**    | Method from prepend should be found first | Level 2 (Constructed) |
| **Extend adds class methods**  | Class method from extend should resolve   | Level 2 (Constructed) |
| **Diamond linearization**      | Ruby's C3 linearization order             | Level 2 (Constructed) |
| **Deep chains resolve**        | N-level deep include finds deepest method | Level 2 (Constructed) |
| **CodeLens counts**            | Generate N includes, verify count = N     | Level 2 (Constructed) |
| **Completion includes mixins** | Methods from all ancestors appear         | Level 2 (Constructed) |
| **Edge cases don't crash**     | Circular, missing, self-include           | Level 1 (No crash)    |

---

## 8. Quick Reference

### Running Tests

```bash
# Run all simulation tests
cargo test simulation

# Run with specific seed (reproduce failure)
PROPTEST_SEED=0x1234567890abcdef cargo test lsp_model_correspondence

# Run with more iterations
PROPTEST_CASES=500 cargo test simulation

# Run with verbose output (see all transitions)
PROPTEST_VERBOSE=1 cargo test simulation
```

### Assertion Hierarchy

| Level | Operation         | What We Assert                | If This Fails...           |
| ----- | ----------------- | ----------------------------- | -------------------------- |
| **1** | ALL               | No panics                     | Fix crash bug              |
| **1** | DidOpen/DidChange | Text sync: Model == LSP       | Fix incremental sync       |
| **1** | ALL queries       | Valid response structure      | Fix response serialization |
| **2** | Definition        | Markers point to definitions  | Fix goto definition        |
| **2** | References        | Markers found in references   | Fix find references        |
| **2** | DocumentSymbols   | All generated symbols found   | Fix parser recovery        |
| **2** | InlayHints        | All `end` keywords have hints | Fix inlay hint visitor     |
| **2** | SemanticTokens    | Deterministic output          | Fix token generation       |
| **2** | FoldingRange      | All classes/methods foldable  | Fix folding visitor        |
| **2** | CodeLens          | Mixin counts accurate         | Fix code lens logic        |
| **2** | Completion        | Expected items present        | Fix completion provider    |
| **3** | OnTypeFormatting  | Idempotent                    | Fix formatter              |
| **3** | OnTypeFormatting  | Preserves AST semantics       | Fix formatter              |

### Key Files

```
src/test/simulation/
├── mod.rs              # Module exports
├── model.rs            # LspModel definition
├── transitions.rs      # Transition enum + strategies
├── generators.rs       # Ruby content generators
├── tests.rs            # proptest! macros
└── seeds.toml          # Documented failure seeds
```

---

## 9. References

- [proptest Book](https://proptest-rs.github.io/proptest/)
- [proptest-state-machine](https://docs.rs/proptest-state-machine/)
- [TigerBeetle Simulation Testing](https://tigerbeetle.com/blog/2023-07-11-we-put-a-distributed-database-in-the-browser/)
- [FoundationDB Testing](https://apple.github.io/foundationdb/testing.html)
