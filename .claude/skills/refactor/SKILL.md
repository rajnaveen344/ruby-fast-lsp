# Refactoring Skill

Use this skill when breaking down large modules, extracting abstractions, or simplifying complex code in the Ruby Fast LSP project. Provides strategies for safe refactoring while maintaining test coverage. Triggers: refactor, split module, extract, simplify, break down, too long, complexity, modularize.

---

## Priority Targets

Current modules exceeding complexity limits:

| File                             | Lines | Target | Priority |
| -------------------------------- | ----- | ------ | -------- |
| `analyzer_prism/mod.rs`          | 2420  | 800    | High     |
| `capabilities/completion/mod.rs` | 2231  | 500    | High     |
| `inferrer/return_type.rs`        | 1661  | 500    | Medium   |
| `inferrer/cfg/builder.rs`        | 1467  | 500    | Medium   |
| `inferrer/cfg/engine.rs`         | 1142  | 500    | Medium   |
| `indexer/coordinator.rs`         | 1108  | 500    | Medium   |
| `test/harness/check.rs`          | 1307  | 500    | Low      |

---

## Safe Refactoring Protocol

### Before Starting

1. **Ensure tests pass**: `cargo test`
2. **Check coverage**: Know what's tested
3. **Document current behavior**: Note edge cases
4. **Create branch**: Never refactor on main

### During Refactoring

1. **Small commits**: One logical change per commit
2. **Run tests frequently**: After every extraction
3. **No behavior changes**: Pure structural changes only
4. **Keep public API stable**: Internal changes only

### After Refactoring

1. **All tests pass**: `cargo test`
2. **No new warnings**: `cargo clippy`
3. **Format check**: `cargo fmt --check`
4. **Review diff**: Ensure no accidental changes

---

## Strategy 1: Extract Module

For large files with distinct logical sections.

### Example: analyzer_prism/mod.rs

**Current structure** (2420 lines, monolithic):

```rust
// mod.rs contains everything:
// - Identifier enum (50 lines)
// - MethodReceiver enum (80 lines)
// - RubyPrismAnalyzer struct (2000+ lines)
// - Helper functions (200+ lines)
```

**Target structure**:

```
analyzer_prism/
├── mod.rs              # Re-exports, 50 lines
├── identifier.rs       # Identifier enum + Display, 100 lines
├── method_receiver.rs  # MethodReceiver enum, 100 lines
├── analyzer.rs         # RubyPrismAnalyzer, 800 lines max
├── visitors/
│   ├── mod.rs
│   ├── definition.rs   # Definition extraction
│   ├── reference.rs    # Reference tracking
│   └── scope.rs        # Scope management
└── helpers.rs          # Utility functions, 200 lines
```

**Steps**:

1. **Extract Identifier enum**

```rust
// analyzer_prism/identifier.rs
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Identifier {
    Constant(String, Range),
    Method(String, Range),
    // ...
}

impl std::fmt::Display for Identifier { ... }
```

2. **Update mod.rs**

```rust
// analyzer_prism/mod.rs
mod identifier;
mod method_receiver;
mod analyzer;

pub use identifier::Identifier;
pub use method_receiver::MethodReceiver;
pub use analyzer::RubyPrismAnalyzer;
```

3. **Run tests**: `cargo test analyzer`

4. **Repeat for each extraction**

---

## Strategy 2: Extract Function

For long functions that do multiple things.

### TigerStyle Rule: 70-Line Maximum

**Identify extraction points**:

- Look for comments like "Step 1:", "Now we..."
- Look for blank lines separating logic
- Look for repeated patterns

**Example: Long function**

```rust
// BEFORE: 150 lines
fn process_method_call(&mut self, node: &MethodCall) {
    // Extract receiver (30 lines)
    let receiver = ...;

    // Resolve method (50 lines)
    let method = ...;

    // Check arguments (40 lines)
    for arg in node.args() {
        ...
    }

    // Build result (30 lines)
    ...
}
```

**AFTER: 20 lines + 4 helpers**

```rust
fn process_method_call(&mut self, node: &MethodCall) {
    let receiver = self.extract_receiver(node);
    let method = self.resolve_method(&receiver, node.name());
    self.check_arguments(node, &method);
    self.build_call_result(receiver, method)
}

fn extract_receiver(&self, node: &MethodCall) -> MethodReceiver { ... }
fn resolve_method(&self, receiver: &MethodReceiver, name: &str) -> Option<Method> { ... }
fn check_arguments(&mut self, node: &MethodCall, method: &Method) { ... }
fn build_call_result(&self, receiver: MethodReceiver, method: Option<Method>) -> CallResult { ... }
```

---

## Strategy 3: Extract Trait

For polymorphic behavior or shared interfaces.

### Example: Completion strategies

**Current**: One large module with match statements

```rust
// BEFORE
fn get_completions(context: &Context) -> Vec<CompletionItem> {
    match context.trigger {
        Trigger::Constant => complete_constants(context),
        Trigger::Method => complete_methods(context),
        Trigger::Variable => complete_variables(context),
        // ... more cases
    }
}
```

**AFTER**: Trait-based strategies

```rust
// completion/strategy.rs
pub trait CompletionStrategy {
    fn applies(&self, context: &Context) -> bool;
    fn complete(&self, context: &Context) -> Vec<CompletionItem>;
}

// completion/strategies/constant.rs
pub struct ConstantStrategy;
impl CompletionStrategy for ConstantStrategy { ... }

// completion/strategies/method.rs
pub struct MethodStrategy;
impl CompletionStrategy for MethodStrategy { ... }

// completion/mod.rs
fn get_completions(context: &Context) -> Vec<CompletionItem> {
    STRATEGIES.iter()
        .filter(|s| s.applies(context))
        .flat_map(|s| s.complete(context))
        .collect()
}
```

---

## Strategy 4: Introduce Intermediate Type

For long parameter lists or repeated data groupings.

### Example: Position context

**BEFORE**:

```rust
fn find_at_position(
    index: &Index,
    uri: &Url,
    position: Position,
    document: &Document,
    include_declaration: bool,
) -> Vec<Location> { ... }
```

**AFTER**:

```rust
struct PositionContext<'a> {
    index: &'a Index,
    uri: &'a Url,
    position: Position,
    document: &'a Document,
}

impl PositionContext<'_> {
    fn find_definitions(&self) -> Vec<Location> { ... }
    fn find_references(&self, include_declaration: bool) -> Vec<Location> { ... }
}
```

---

## Strategy 5: Split by Phase

For functions that do sequential phases.

### Example: Two-phase indexing

**BEFORE**: One 500-line function

```rust
fn index_workspace(&mut self) {
    // Phase 1: Collect definitions (200 lines)
    for file in files {
        // ...
    }

    // Phase 2: Resolve references (200 lines)
    for file in files {
        // ...
    }

    // Phase 3: Build graphs (100 lines)
    // ...
}
```

**AFTER**: Separate phase functions

```rust
fn index_workspace(&mut self) {
    let definitions = self.phase1_collect_definitions();
    let references = self.phase2_resolve_references(&definitions);
    self.phase3_build_graphs(&definitions, &references);
}

fn phase1_collect_definitions(&self) -> DefinitionMap { ... }
fn phase2_resolve_references(&self, defs: &DefinitionMap) -> ReferenceMap { ... }
fn phase3_build_graphs(&mut self, defs: &DefinitionMap, refs: &ReferenceMap) { ... }
```

---

## Refactoring Checklist

### Pre-refactoring

- [ ] All tests pass
- [ ] Created feature branch
- [ ] Identified extraction points
- [ ] Documented expected behavior

### During refactoring

- [ ] One extraction per commit
- [ ] Tests pass after each commit
- [ ] No behavior changes
- [ ] Public API unchanged

### Post-refactoring

- [ ] All tests pass
- [ ] No clippy warnings
- [ ] Code formatted
- [ ] Each file under target line count
- [ ] Each function under 70 lines

---

## Common Pitfalls

### 1. Changing behavior while refactoring

**Problem**: "While I'm here, let me fix this bug..."
**Solution**: Separate commits. Refactor first, then fix.

### 2. Breaking public API

**Problem**: Renaming public functions breaks callers.
**Solution**: Use `pub(crate)` for internal APIs. Deprecate before removing.

### 3. Over-extracting

**Problem**: 50 tiny functions that are hard to follow.
**Solution**: Extract for reuse or clarity, not just line count.

### 4. Losing context

**Problem**: Extracted function has unclear purpose.
**Solution**: Name clearly, add doc comments.

```rust
// BAD
fn helper1(x: &Node) -> bool { ... }

// GOOD
/// Returns true if the node represents a method definition.
fn is_method_definition(node: &Node) -> bool { ... }
```

---

## Incremental Approach for Large Modules

For modules like `analyzer_prism/mod.rs` (2420 lines):

### Week 1: Extract data types

- Move `Identifier` enum to `identifier.rs`
- Move `MethodReceiver` to `method_receiver.rs`
- ~200 lines extracted

### Week 2: Extract pure helpers

- Move utility functions to `helpers.rs`
- No struct dependencies
- ~200 lines extracted

### Week 3: Extract visitor logic

- Create `visitors/` submodule
- Move visitor trait implementations
- ~500 lines extracted

### Week 4: Simplify main analyzer

- Analyzer now delegates to visitors
- Main file under 800 lines
